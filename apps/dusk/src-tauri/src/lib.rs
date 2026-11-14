//! Dusk (com.selfkit.dusk): sleep estimated from device use, plus a manual log.
//! Docs: docs/apps/dusk/README.md (code map).

pub mod data;
pub mod recover;
pub mod health;
pub mod model;
pub mod sources;

use std::sync::Mutex;

use chrono::{Duration, NaiveDate};
use selfkit_core::settings::Settings;
use selfkit_core::{Store, paths};
use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};

pub use data::MIGRATIONS;
use model::{HOUR, Inputs, Ms, NightView};

pub struct AppState {
    store: Mutex<Store>,
    settings: Mutex<Store>,
    /// Set when a damaged DB was set aside at startup (shown once by the UI).
    notice: Mutex<Option<String>>,
}

type Res<T> = Result<T, String>;

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn now() -> Ms {
    selfkit_core::time::now_ms()
}

fn tz_now() -> i32 {
    selfkit_core::time::local_offset_min()
}

fn day_start(state: &AppState) -> u16 {
    Settings::load(&state.settings.lock().unwrap()).map(|s| s.day_start_min).unwrap_or(0)
}

/// Every night in [from, to], from all activity sources plus the log.
pub fn compute(store: &Store, from: NaiveDate, to: NaiveDate, now_active: bool) -> data::Result<Vec<NightView>> {
    let tz = tz_now();
    let (fs, te) = (model::window(from, tz).0 - 14 * HOUR, model::window(to, tz).1 + 14 * HOUR);
    let mut spans = sources::device_use_spans(store, fs, te);
    spans.extend(sources::chrono_spans(fs, te).0);
    let spans = model::merge_spans(spans);
    let logs = data::logs(store)?;
    let ext = health::sessions(store, fs, te);
    let inp = Inputs { spans: &spans, logs: &logs, external: &ext, now: now(), now_active, fallback_tz: tz };
    Ok(model::nights(&inp, from, to))
}

#[derive(Serialize)]
struct Dashboard {
    /// The night shown first ("last night"), respecting the day-start setting.
    current: String,
    /// The last 90 nights, oldest first. Activity is kept for the last 14 only.
    nights: Vec<NightView>,
    s7: model::Stats,
    s14: model::Stats,
    s30: model::Stats,
    s90: model::Stats,
    weekly: Vec<model::WeekAvg>,
    goals: data::GoalsRec,
    day_start_min: u16,
    now: Ms,
}

#[tauri::command]
fn dashboard(state: State<AppState>) -> Res<Dashboard> {
    let ds = day_start(&state);
    let cur = model::current_night(now(), tz_now(), ds);
    let store = state.store.lock().unwrap();
    let goals = data::goals(&store).map_err(e)?;
    let mut nights = compute(&store, cur - Duration::days(89), cur, true).map_err(e)?;
    let _ = data::cache_nights(&store, &nights);
    let g = goals.model();
    let (s7, s14, s30, s90) =
        (model::stats(&nights, 7, &g), model::stats(&nights, 14, &g), model::stats(&nights, 30, &g), model::stats(&nights, 90, &g));
    let weekly = model::weekly(&nights);
    let keep = nights.len().saturating_sub(14);
    for n in &mut nights[..keep] {
        n.estimate.activity.clear();
    }
    Ok(Dashboard { current: model::date_str(cur), nights, s7, s14, s30, s90, weekly, goals, day_start_min: ds, now: now() })
}

/// One night with full detail (any date).
#[tauri::command]
fn night(state: State<AppState>, night: String) -> Res<NightView> {
    let n = model::parse_date(&night).ok_or("bad date")?;
    let store = state.store.lock().unwrap();
    let v = compute(&store, n, n, true).map_err(e)?;
    let _ = data::cache_nights(&store, &v);
    v.into_iter().next().ok_or_else(|| "no night".into())
}

#[derive(Serialize)]
struct Sources {
    chrono: bool,
    agenda: bool,
    hisab: bool,
    mobile: bool,
    phone_nights: i64,
    data_dir: String,
}

#[tauri::command]
fn sources_status(state: State<AppState>) -> Res<Sources> {
    let store = state.store.lock().unwrap();
    Ok(Sources {
        chrono: paths::app_db("chrono").exists(),
        agenda: paths::app_db("agenda").exists(),
        hisab: paths::app_db("hisab").exists(),
        mobile: cfg!(mobile),
        phone_nights: store.count("device_use").unwrap_or(0),
        data_dir: paths::data_dir().display().to_string(),
    })
}

fn currency_symbol(code: &str) -> String {
    match code {
        "USD" | "CAD" | "AUD" => "$".into(),
        "EUR" => "€".into(),
        "GBP" => "£".into(),
        "INR" => "₹".into(),
        "JPY" => "¥".into(),
        "BDT" => "৳".into(),
        c => format!("{c} "),
    }
}

/// Connections over the last 3 months.
#[tauri::command]
fn connections(state: State<AppState>) -> Res<Vec<model::Connection>> {
    let ds = day_start(&state);
    let currency = Settings::load(&state.settings.lock().unwrap()).map(|s| s.main_currency).unwrap_or_else(|_| "USD".into());
    let cur = model::current_night(now(), tz_now(), ds);
    let nights = {
        let store = state.store.lock().unwrap();
        compute(&store, cur - Duration::days(89), cur, true).map_err(e)?
    };
    let today = model::logical_day(now(), tz_now(), ds);
    let mut out = vec![model::late_use_vs_quality(&nights, if cfg!(mobile) { "this phone" } else { "Chrono" })];
    let agenda = sources::open_other("agenda");
    let tasks = agenda.as_ref().map(|a| sources::tasks_done_per_day(a, ds, today)).unwrap_or_default();
    out.push(model::sleep_vs_next_day(&nights, &tasks, ds, "tasks", "tasks", agenda.is_some()));
    let hisab = sources::open_other("hisab");
    let spent = hisab.as_ref().map(|h| sources::spent_per_day(h, ds, today, &currency)).unwrap_or_default();
    out.push(model::sleep_vs_next_day(&nights, &spent, ds, "money", &currency_symbol(&currency), hisab.is_some()));
    Ok(out)
}

/// Change the night's main entry (correction or rating). See `data::set_main`.
#[tauri::command]
fn log_set(app: AppHandle, state: State<AppState>, night: String, patch: Map<String, Value>) -> Res<NightView> {
    let n = model::parse_date(&night).ok_or("bad date")?;
    let store = state.store.lock().unwrap();
    // Place typed times with the night's own offset (travel).
    let cur = compute(&store, n, n, false).map_err(e)?.into_iter().next();
    let tz = cur.as_ref().map(|v| v.tz_offset).unwrap_or_else(tz_now);
    data::set_main(&store, &night, &patch, tz).map_err(e)?;
    let v = compute(&store, n, n, true).map_err(e)?;
    // Changing only one time keeps the estimate for the other: check the pair
    // that will show (QA: bedtime 03:00 with an estimated wake of 02:52 was saved).
    if let Some((b, w)) = v.first().and_then(|x| Some((x.bed?, x.wake?)))
        && (w <= b || w - b > 20 * HOUR)
    {
        let _ = data::replace_main(&store, &night, cur.and_then(|c| c.log).as_ref());
        return Err(if w <= b { "Wake time must be after bedtime".into() } else { "That's over 20 hours of sleep. Check the times.".into() });
    }
    let _ = data::cache_nights(&store, &v);
    let _ = app.emit("sleep-changed", ());
    v.into_iter().next().ok_or_else(|| "no night".into())
}

/// Undo: put the main entry back as it was (null = remove it).
#[tauri::command]
fn log_replace(app: AppHandle, state: State<AppState>, night: String, before: Option<model::LogEntry>) -> Res<()> {
    data::replace_main(&state.store.lock().unwrap(), &night, before.as_ref()).map_err(e)?;
    let _ = app.emit("sleep-changed", ());
    Ok(())
}

#[tauri::command]
fn nap_add(app: AppHandle, state: State<AppState>, night: String, start: String, end: String) -> Res<model::LogEntry> {
    let l = data::add_nap(&state.store.lock().unwrap(), &night, &start, &end, tz_now()).map_err(e)?;
    let _ = app.emit("sleep-changed", ());
    Ok(l)
}

#[tauri::command]
fn nap_delete(app: AppHandle, state: State<AppState>, id: String) -> Res<()> {
    state.store.lock().unwrap().delete(&id).map_err(e)?;
    let _ = app.emit("sleep-changed", ());
    Ok(())
}

#[tauri::command]
fn nap_restore(app: AppHandle, state: State<AppState>, id: String) -> Res<()> {
    state.store.lock().unwrap().restore(&id).map_err(e)?;
    let _ = app.emit("sleep-changed", ());
    Ok(())
}

#[tauri::command]
fn goals_set(app: AppHandle, state: State<AppState>, goals: data::GoalsRec) -> Res<data::GoalsRec> {
    let g = data::save_goals(&state.store.lock().unwrap(), &goals).map_err(e)?;
    reminders::reschedule(&app);
    let _ = app.emit("sleep-changed", ());
    Ok(g)
}

#[tauri::command]
fn device_get(state: State<AppState>) -> Res<data::DevicePrefs> {
    data::device(&state.store.lock().unwrap()).map_err(e)
}

#[tauri::command]
fn device_set(app: AppHandle, state: State<AppState>, prefs: data::DevicePrefs) -> Res<()> {
    {
        let store = state.store.lock().unwrap();
        let mut p = prefs.clone();
        // The import cursor is owned by android_import.
        p.import_cursor = data::device(&store).map(|d| d.import_cursor).unwrap_or(0);
        data::save_device(&store, &p).map_err(e)?;
    }
    reminders::reschedule(&app);
    let _ = app.emit("prefs-changed", &prefs);
    Ok(())
}

/// Shared Selfkit settings (day start, currency, shortcuts).
#[tauri::command]
fn settings_get(state: State<AppState>) -> Res<Settings> {
    Settings::load(&state.settings.lock().unwrap()).map_err(e)
}

#[tauri::command]
fn settings_set(app: AppHandle, state: State<AppState>, settings: Settings) -> Res<()> {
    settings.save(&state.settings.lock().unwrap()).map_err(e)?;
    #[cfg(desktop)]
    shortcut::register(&app, &settings);
    let _ = app.emit("sleep-changed", ());
    Ok(())
}

/// Android: raw UsageStatsManager events `[time, type]` for [from, to)
/// from the Kotlin bridge → phone intervals in `device_use`.
#[tauri::command]
fn android_import(state: State<AppState>, events: Vec<(Ms, i32)>, from: Ms, to: Ms) -> Res<Vec<String>> {
    let store = state.store.lock().unwrap();
    let iv = sources::intervals_from_events(events, from, to);
    let dev = store.device_id().to_string();
    let touched = sources::save_device_use(&store, &dev, tz_now(), &iv).map_err(e)?;
    let mut p = data::device(&store).map_err(e)?;
    p.import_cursor = p.import_cursor.max(to);
    data::save_device(&store, &p).map_err(e)?;
    Ok(touched)
}

#[tauri::command]
fn close_capture(app: AppHandle) {
    if let Some(w) = app.get_webview_window("capture") {
        let _ = w.close();
    }
}

/// Test hook for screenshots/automation: DUSK_DEMO="…". Unset in normal use.
#[tauri::command]
fn dev_hooks() -> Option<String> {
    std::env::var("DUSK_DEMO").ok()
}

/// Wind-down reminder: off by default; the user turns it on in Settings.
/// Fires `winddown_min` before the bedtime goal every day.
/// Android: exact alarms through the notification plugin for the next 14
/// days (re-planned on every start and settings change). Desktop: a timer
/// thread while the app runs.
mod reminders {
    use super::*;
    use chrono::{Local, TimeZone};
    use tauri_plugin_notification::NotificationExt;

    #[cfg_attr(desktop, allow(dead_code))]
    pub const BASE_ID: i32 = 7100;
    #[cfg_attr(desktop, allow(dead_code))]
    pub const DAYS: i32 = 14;

    pub fn text(g: &data::GoalsRec) -> (String, String) {
        ("Time to wind down".into(), format!("Bedtime goal {}. Dim the lights and put the screens away.", g.bedtime))
    }

    /// (enabled, goals) from the store.
    fn plan(app: &AppHandle) -> (bool, data::GoalsRec) {
        let state = app.state::<AppState>();
        let store = state.store.lock().unwrap();
        (data::device(&store).map(|d| d.winddown_on).unwrap_or(false), data::goals(&store).unwrap_or_default())
    }

    /// Local wind-down time on a given date.
    pub fn at_on(g: &data::GoalsRec, d: NaiveDate) -> Option<chrono::DateTime<Local>> {
        let m = g.model();
        let mins = (m.bedtime_min - m.winddown_min).rem_euclid(1440);
        let t = chrono::NaiveTime::from_hms_opt((mins / 60) as u32, (mins % 60) as u32, 0)?;
        Local.from_local_datetime(&d.and_time(t)).earliest()
    }

    #[cfg(mobile)]
    pub fn reschedule(app: &AppHandle) {
        use tauri_plugin_notification::Schedule;
        let n = app.notification();
        let _ = n.cancel((0..DAYS).map(|i| BASE_ID + i).collect());
        let (on, g) = plan(app);
        if !on {
            return;
        }
        let (title, body) = text(&g);
        let today = Local::now().date_naive();
        let mut i = 0;
        for k in 0..=DAYS {
            let Some(at) = at_on(&g, today + Duration::days(k as i64)) else { continue };
            if at <= Local::now() || i >= DAYS {
                continue;
            }
            let Ok(date) = time::OffsetDateTime::from_unix_timestamp(at.timestamp()) else { continue };
            let r = n
                .builder()
                .id(BASE_ID + i)
                .title(&title)
                .body(&body)
                .schedule(Schedule::At { date, repeating: false, allow_while_idle: true })
                .show();
            if let Err(err) = r {
                eprintln!("schedule wind-down: {err}");
            }
            i += 1;
        }
    }

    #[cfg(desktop)]
    pub fn reschedule(_app: &AppHandle) {}

    /// Desktop: every 20 s, notify when today's wind-down time just passed.
    #[cfg(desktop)]
    pub fn start_desktop_timer(app: AppHandle) {
        std::thread::spawn(move || {
            let mut last = Local::now();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(20));
                let now = Local::now();
                let (on, g) = plan(&app);
                if on
                    && let Some(at) = at_on(&g, now.date_naive())
                    && at > last
                    && at <= now
                {
                    let (title, body) = text(&g);
                    let _ = app.notification().builder().title(title).body(body).show();
                }
                last = now;
            }
        });
    }
}

#[cfg(desktop)]
mod shortcut {
    use selfkit_core::settings::Settings;
    use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    /// Small "How did you sleep?" popup (rate last night in two key presses/taps).
    pub fn open_capture(app: &AppHandle) {
        if let Some(w) = app.get_webview_window("capture") {
            let _ = w.show();
            let _ = w.set_focus();
            return;
        }
        let r = WebviewWindowBuilder::new(app, "capture", WebviewUrl::App("index.html?view=capture".into()))
            .title("Rate sleep")
            .inner_size(420.0, 250.0)
            .resizable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .center()
            .focused(true)
            .build();
        if let Err(e) = r {
            eprintln!("capture window: {e}");
        }
    }

    pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    open_capture(app);
                }
            })
            .build()
    }

    pub fn register(app: &AppHandle, s: &Settings) {
        let gs = app.global_shortcut();
        let _ = gs.unregister_all();
        if let Some(accel) = s.shortcuts.get("rate_sleep").filter(|a| !a.is_empty())
            && let Err(e) = gs.register(accel.as_str())
        {
            eprintln!("could not register shortcut {accel}: {e}");
        }
    }
}

/// A one-time startup message (damaged data file recovered), or None.
#[tauri::command]
fn startup_notice(state: State<AppState>) -> Option<String> {
    state.notice.lock().unwrap().take()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // Desktop: one instance (a second launch focuses the first, or opens its
    // popup for --rate), so reminders don't fire twice and the shortcut stays
    // registered. Skipped when SELFKIT_DATA_DIR is set (tests run several apps on
    // separate data dirs at once).
    #[cfg(desktop)]
    let builder = if std::env::var_os("SELFKIT_DATA_DIR").is_none() {
        builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|a| a == "--rate") {
                shortcut::open_capture(app);
            } else if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
    } else {
        builder
    };
    let builder = builder.plugin(tauri_plugin_notification::init());
    #[cfg(desktop)]
    let builder = builder.plugin(shortcut::plugin());
    builder
        .setup(|app| {
            #[cfg(mobile)]
            paths::set_data_dir(app.path().app_data_dir()?);
            let device = paths::device_id()?;
            let (store, n1) = recover::open(paths::app_db("dusk"), "dusk", &device, MIGRATIONS)?;
            let (settings, n2) = recover::open(paths::app_db(selfkit_core::settings::APP), selfkit_core::settings::APP, &device, &[])?;
            app.manage(AppState { store: Mutex::new(store), settings: Mutex::new(settings), notice: Mutex::new(n1.or(n2)) });
            #[cfg(desktop)]
            {
                let s = Settings::load(&app.state::<AppState>().settings.lock().unwrap()).unwrap_or_default();
                shortcut::register(app.handle(), &s);
                if std::env::args().any(|a| a == "--rate") {
                    shortcut::open_capture(app.handle());
                }
                if let (Ok(sz), Some(w)) = (std::env::var("DUSK_WINDOW_SIZE"), app.get_webview_window("main"))
                    && let Some((a, b)) = sz.split_once('x')
                    && let (Ok(wd), Ok(ht)) = (a.parse::<f64>(), b.parse::<f64>())
                {
                    let _ = w.set_size(tauri::LogicalSize::new(wd, ht));
                }
                reminders::start_desktop_timer(app.handle().clone());
            }
            #[cfg(mobile)]
            {
                let h = app.handle().clone();
                std::thread::spawn(move || reminders::reschedule(&h));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dashboard,
            night,
            sources_status,
            connections,
            log_set,
            log_replace,
            nap_add,
            nap_delete,
            nap_restore,
            goals_set,
            device_get,
            device_set,
            settings_get,
            settings_set,
            android_import,
            close_capture,
            dev_hooks, startup_notice
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dusk");
}
