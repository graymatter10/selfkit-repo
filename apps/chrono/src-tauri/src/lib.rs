//! Chrono (com.selfkit.chrono): activity dashboard over chrono.db.
//! Docs: docs/apps/chrono/README.md (code map).

pub mod alerts;
pub mod android;
pub mod categories;
pub mod notes;
pub mod recover;
pub mod settings;
pub mod stats;

use std::sync::Mutex;

use selfkit_core::{Store, paths, time};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

use settings::ChronoSettings;
use stats::Engine;

pub struct AppState {
    store: Mutex<Store>,
    shared: Mutex<Store>,
    /// A damaged DB was set aside at start (recover.rs): told once to the page (L25).
    notice: Mutex<Option<String>>,
}

/// A one-time startup message (damaged data file set aside), or None.
#[tauri::command]
fn startup_notice(state: State<AppState>) -> Option<String> {
    state.notice.lock().unwrap().take()
}

type Res<T> = Result<T, String>;

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

/// Open chrono.db with Chrono's migrations.
pub fn open_store() -> selfkit_core::Result<Store> {
    let device = paths::device_id()?;
    Store::open(paths::app_db("chrono"), "chrono", &device, stats::MIGRATIONS)
}

pub fn open_shared() -> selfkit_core::Result<Store> {
    let device = paths::device_id()?;
    Store::open(paths::app_db(selfkit_core::settings::APP), selfkit_core::settings::APP, &device, &[])
}

pub fn day_start(shared: &Store) -> i64 {
    selfkit_core::settings::Settings::load(shared).map(|s| s.day_start_min as i64).unwrap_or(0)
}

fn engine<'a>(store: &'a Store, shared: &Store) -> Res<Engine<'a>> {
    let set = ChronoSettings::load(store).map_err(e)?;
    Ok(Engine::new(store, set, day_start(shared), time::local_offset_min()))
}

fn day_arg(eng: &Engine, day: Option<String>, now: i64) -> i64 {
    day.as_deref().and_then(stats::str_to_day).unwrap_or_else(|| eng.today(now))
}

macro_rules! with_engine {
    ($state:expr, |$eng:ident, $now:ident| $body:expr) => {{
        let store = $state.store.lock().unwrap();
        let shared = $state.shared.lock().unwrap();
        let $eng = engine(&store, &shared)?;
        let $now = time::now_ms();
        $body.map_err(e)
    }};
}

#[tauri::command]
fn day_view(state: State<AppState>, day: Option<String>) -> Res<stats::DayView> {
    with_engine!(state, |eng, now| stats::day_view(&eng, day_arg(&eng, day, now), now))
}

#[tauri::command]
fn timeline(state: State<AppState>, day: Option<String>) -> Res<stats::TimelineView> {
    with_engine!(state, |eng, now| stats::timeline(&eng, day_arg(&eng, day, now)))
}

#[tauri::command]
fn trends(state: State<AppState>, period: String, anchor: Option<String>) -> Res<stats::TrendsView> {
    with_engine!(state, |eng, now| stats::trends(&eng, &period, day_arg(&eng, anchor, now), now))
}

#[tauri::command]
fn report(state: State<AppState>, anchor: Option<String>) -> Res<stats::Report> {
    with_engine!(state, |eng, now| stats::report(&eng, day_arg(&eng, anchor, now), now))
}

#[tauri::command]
fn websites(state: State<AppState>, period: String, anchor: Option<String>) -> Res<stats::WebsitesView> {
    with_engine!(state, |eng, now| stats::websites(&eng, &period, day_arg(&eng, anchor, now), now))
}

#[tauri::command]
fn site(state: State<AppState>, name: String, period: String, anchor: Option<String>) -> Res<stats::SiteDetail> {
    with_engine!(state, |eng, now| stats::site_detail(&eng, &name, &period, day_arg(&eng, anchor, now), now))
}

#[tauri::command]
fn names(state: State<AppState>) -> Res<Vec<stats::Total>> {
    with_engine!(state, |eng, now| stats::names(&eng, 90, now))
}

#[tauri::command]
fn sources(state: State<AppState>) -> Res<Vec<stats::Source>> {
    with_engine!(state, |eng, now| stats::sources(&eng, now))
}

#[tauri::command]
fn app_changes(state: State<AppState>) -> Res<Vec<stats::AppChange>> {
    with_engine!(state, |eng, _now| stats::app_changes(&eng))
}

#[tauri::command]
fn note_get(state: State<AppState>, day: String) -> Res<Option<notes::DayNote>> {
    notes::get(&state.store.lock().unwrap(), &day).map_err(e)
}

#[tauri::command]
fn note_save(app: AppHandle, state: State<AppState>, day: String, text: String, photos: Vec<String>) -> Res<Option<notes::DayNote>> {
    let n = notes::save(&state.store.lock().unwrap(), &day, &text, &photos).map_err(e)?;
    let _ = app.emit("data-changed", ());
    Ok(n)
}

/// Save a photo (base64 of the already scaled image); returns its id.
#[tauri::command]
fn photo_save(state: State<AppState>, data: String, mime: String, w: u32, h: u32) -> Res<String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.as_bytes()).map_err(e)?;
    notes::save_photo(&state.store.lock().unwrap(), &bytes, &mime, w, h).map_err(e)
}

/// A photo as a data: URL.
#[tauri::command]
fn photo_get(state: State<AppState>, id: String) -> Res<String> {
    use base64::Engine as _;
    let (mime, bytes) = notes::load_photo(&state.store.lock().unwrap(), &id).map_err(e)?;
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[tauri::command]
fn photo_export(state: State<AppState>, id: String) -> Res<String> {
    let (mime, bytes) = notes::load_photo(&state.store.lock().unwrap(), &id).map_err(e)?;
    export_to_pictures(&id, ext_for(&mime), &bytes)
}

/// "Save to Pictures" from the photo viewer on desktop (Android saves through
/// the SelfkitPhotos bridge instead): copy into ~/Pictures/Selfkit/Chrono.
/// Docs: docs/components/photo-source.md.
fn export_to_pictures(id: &str, ext: &str, bytes: &[u8]) -> Result<String, String> {
    let dir = std::env::var_os("SELFKIT_PICTURES_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|h| std::path::PathBuf::from(h).join("Pictures")))
        .ok_or("No Pictures folder")?
        .join("Selfkit")
        .join("Chrono");
    std::fs::create_dir_all(&dir).map_err(e)?;
    let stem: String = id.split('.').next().unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(40).collect();
    let path = dir.join(format!("Chrono_{stem}.{ext}"));
    std::fs::write(&path, bytes).map_err(e)?;
    Ok(path.display().to_string())
}

fn ext_for(mime: &str) -> &'static str {
    match mime {
        "image/webp" => "webp",
        "image/png" => "png",
        _ => "jpg",
    }
}

#[derive(serde::Serialize)]
struct Info {
    device: String,
    platform: &'static str,
    day_start: i64,
    today: String,
}

#[tauri::command]
fn info(state: State<AppState>) -> Res<Info> {
    let store = state.store.lock().unwrap();
    let shared = state.shared.lock().unwrap();
    let eng = engine(&store, &shared)?;
    let platform = if cfg!(target_os = "android") { "android" } else if cfg!(windows) { "windows" } else { "linux" };
    Ok(Info { device: store.device_id().into(), platform, day_start: eng.ds, today: stats::day_to_str(eng.today(time::now_ms())) })
}

#[tauri::command]
fn settings_get(state: State<AppState>) -> Res<ChronoSettings> {
    ChronoSettings::load(&state.store.lock().unwrap()).map_err(e)
}

#[tauri::command]
fn settings_save(app: AppHandle, state: State<AppState>, mut settings: ChronoSettings) -> Res<ChronoSettings> {
    settings.sanitize();
    settings.save(&state.store.lock().unwrap()).map_err(e)?;
    let _ = app.emit("data-changed", ());
    Ok(settings)
}

/// Pause tracking for `minutes` (null = until resumed). On Android, import
/// first so nothing recorded before the pause is lost.
#[tauri::command]
async fn pause(app: AppHandle, minutes: Option<i64>) -> Res<ChronoSettings> {
    #[cfg(target_os = "android")]
    let _ = mobile::import(&app);
    let state = app.state::<AppState>();
    let store = state.store.lock().unwrap();
    let mut s = ChronoSettings::load(&store).map_err(e)?;
    s.pause(time::now_ms(), minutes);
    s.save(&store).map_err(e)?;
    let _ = app.emit("data-changed", ());
    Ok(s)
}

#[tauri::command]
fn resume(app: AppHandle, state: State<AppState>) -> Res<ChronoSettings> {
    let store = state.store.lock().unwrap();
    let mut s = ChronoSettings::load(&store).map_err(e)?;
    s.resume(time::now_ms());
    s.save(&store).map_err(e)?;
    let _ = app.emit("data-changed", ());
    Ok(s)
}

/// Appearance from the shared Selfkit settings: "system" | "light" | "dark".
#[tauri::command]
fn theme_get(state: State<AppState>) -> Res<String> {
    use selfkit_core::settings::{Settings, Theme, ThemeMode};
    let s = Settings::load(&state.shared.lock().unwrap()).map_err(e)?;
    Ok(match s.theme.mode {
        ThemeMode::System => "system",
        ThemeMode::Fixed(Theme::SoftDark) => "dark",
        ThemeMode::Fixed(_) => "light",
    }
    .into())
}

#[tauri::command]
fn theme_set(state: State<AppState>, mode: String) -> Res<()> {
    use selfkit_core::settings::{Settings, Theme, ThemeMode};
    // Reload + change only the theme (other apps share settings.db).
    Settings::update(&state.shared.lock().unwrap(), |s| {
        s.theme.mode = match mode.as_str() {
            "dark" => ThemeMode::Fixed(Theme::SoftDark),
            "light" => ThemeMode::Fixed(s.theme.light),
            _ => ThemeMode::System,
        };
    })
    .map(|_| ())
    .map_err(e)
}

/// App preferences (theme) in chrono.db kv (local, not synced).
#[tauri::command]
fn prefs_get(state: State<AppState>) -> Res<Value> {
    let v = state.store.lock().unwrap().kv_get("chrono.prefs").map_err(e)?;
    Ok(v.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_else(|| serde_json::json!({})))
}

#[tauri::command]
fn prefs_set(state: State<AppState>, prefs: Value) -> Res<()> {
    state.store.lock().unwrap().kv_set("chrono.prefs", &prefs.to_string()).map_err(e)
}

/// Android permission/import state. Desktop: `{supported: false}`.
#[tauri::command]
async fn android_status(app: AppHandle) -> Res<Value> {
    #[cfg(target_os = "android")]
    {
        mobile::status(&app)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({"supported": false}))
    }
}

/// Open the system settings page: "usage" or "notifications".
#[tauri::command]
async fn android_open(app: AppHandle, which: String) -> Res<()> {
    #[cfg(target_os = "android")]
    {
        mobile::open(&app, &which)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, which);
        Ok(())
    }
}

/// Import new Android usage data now (on open/resume and pull-to-refresh).
#[tauri::command]
async fn android_import(app: AppHandle) -> Res<Value> {
    #[cfg(target_os = "android")]
    {
        mobile::import(&app)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({"supported": false}))
    }
}

/// Test hook for screenshots/automation: CHRONO_DEMO="…". Unset in normal use.
#[tauri::command]
fn dev_hooks() -> Option<String> {
    std::env::var("CHRONO_DEMO").ok()
}

/// Check goals and show due alerts (desktop timer, Android after import).
pub fn check_alerts(app: &AppHandle) {
    let due = {
        let state = app.state::<AppState>();
        let store = state.store.lock().unwrap();
        let shared = state.shared.lock().unwrap();
        match engine(&store, &shared) {
            Ok(eng) => alerts::due(&eng, &store, time::now_ms()).unwrap_or_default(),
            Err(_) => vec![],
        }
    };
    for a in due {
        let b = app.notification().builder().title(&a.title).body(&a.body);
        // Android: monochrome Chrono clock (res/drawable/ic_stat_chrono.xml)
        #[cfg(mobile)]
        let b = b.icon("ic_stat_chrono");
        if let Err(err) = b.show() {
            eprintln!("alert: {err}");
        }
    }
}

#[cfg(desktop)]
fn start_desktop_timer(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            check_alerts(&app);
            let _ = app.emit("tick", ());
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
}

#[cfg(target_os = "android")]
mod mobile {
    use super::*;
    use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
    use tauri::{Runtime, Wry};

    pub struct Usage<R: Runtime>(pub PluginHandle<R>);

    pub fn plugin() -> TauriPlugin<Wry> {
        Builder::new("chronousage")
            .setup(|app, api| {
                let h = api.register_android_plugin("com.selfkit.chrono", "UsagePlugin")?;
                app.manage(Usage(h));
                Ok(())
            })
            .build()
    }

    fn handle(app: &AppHandle) -> Res<PluginHandle<Wry>> {
        Ok(app.try_state::<Usage<Wry>>().ok_or("usage plugin not ready")?.0.clone())
    }

    pub fn status(app: &AppHandle) -> Res<Value> {
        let mut v: Value = handle(app)?.run_mobile_plugin("status", serde_json::json!({})).map_err(e)?;
        let state = app.state::<AppState>();
        let store = state.store.lock().unwrap();
        v["supported"] = true.into();
        v["cursor"] = store.kv_get(android::CURSOR_KEY).ok().flatten().and_then(|s| s.parse::<i64>().ok()).into();
        v["last_import"] = store.kv_get("chrono.import.at").ok().flatten().and_then(|s| s.parse::<i64>().ok()).into();
        Ok(v)
    }

    pub fn open(app: &AppHandle, which: &str) -> Res<()> {
        let _: Value = handle(app)?.run_mobile_plugin("open", serde_json::json!({"which": which})).map_err(e)?;
        Ok(())
    }

    pub fn import(app: &AppHandle) -> Res<Value> {
        let since = {
            let state = app.state::<AppState>();
            let store = state.store.lock().unwrap();
            android::cursor(&store, time::now_ms())
        };
        let v: Value = handle(app)?.run_mobile_plugin("collect", serde_json::json!({"since": since})).map_err(e)?;
        if v.get("granted").and_then(Value::as_bool) == Some(false) {
            return Ok(serde_json::json!({"granted": false}));
        }
        let payload = v.get("payload").and_then(Value::as_str).unwrap_or("{}");
        let batch: android::Batch = serde_json::from_str(payload).map_err(e)?;
        let res = {
            let state = app.state::<AppState>();
            let store = state.store.lock().unwrap();
            let set = ChronoSettings::load(&store).map_err(e)?;
            let r = android::ingest(&store, &set, &batch).map_err(e)?;
            let _ = store.kv_set("chrono.import.at", &time::now_ms().to_string());
            r
        };
        check_alerts(app);
        if res.sessions > 0 || res.hours > 0 {
            let _ = app.emit("data-changed", ());
        }
        Ok(serde_json::json!({"granted": true, "sessions": res.sessions, "hours": res.hours, "events": res.events}))
    }
}

/// JNI entry points for the background WorkManager job (`ImportWorker.kt`),
/// which runs without the webview. Same import + alert code as the app.
#[cfg(target_os = "android")]
mod jni_bridge {
    use super::*;
    use jni::JNIEnv;
    use jni::objects::{JClass, JString};
    use jni::sys::{jlong, jstring};

    fn open(env: &mut JNIEnv, dir: &JString) -> Option<(Store, Store)> {
        let dir: String = env.get_string(dir).ok()?.into();
        paths::set_data_dir(std::path::PathBuf::from(dir));
        Some((open_store().ok()?, open_shared().ok()?))
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_selfkit_chrono_ChronoNative_cursor(mut env: JNIEnv, _c: JClass, dir: JString) -> jlong {
        match open(&mut env, &dir) {
            Some((s, _)) => android::cursor(&s, time::now_ms()),
            None => -1,
        }
    }

    /// Returns JSON `{ok, sessions, alerts: [{id,title,body}]}`.
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_selfkit_chrono_ChronoNative_ingest(
        mut env: JNIEnv,
        _c: JClass,
        dir: JString,
        payload: JString,
    ) -> jstring {
        let out = (|| -> Result<Value, String> {
            let (store, shared) = open(&mut env, &dir).ok_or("open failed")?;
            let payload: String = env.get_string(&payload).map_err(e)?.into();
            let batch: android::Batch = serde_json::from_str(&payload).map_err(e)?;
            let set = ChronoSettings::load(&store).map_err(e)?;
            let r = android::ingest(&store, &set, &batch).map_err(e)?;
            let _ = store.kv_set("chrono.import.at", &time::now_ms().to_string());
            let eng = Engine::new(&store, set, day_start(&shared), batch.tz_now);
            let due = alerts::due(&eng, &store, time::now_ms()).map_err(e)?;
            Ok(serde_json::json!({"ok": true, "sessions": r.sessions, "hours": r.hours, "alerts": due}))
        })()
        .unwrap_or_else(|err| serde_json::json!({"ok": false, "error": err}));
        env.new_string(out.to_string()).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
    }
}

/// Sync for chrono.db + settings.db (photos: `photos` records, files in chrono-photos/).
fn sync_handle() -> selfkit_core::Result<selfkit_appsync::tauri_cmds::SyncHandle> {
    selfkit_appsync::tauri_cmds::default_handle("chrono", paths::data_dir())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(selfkit_appsync::tauri_cmds::nav_guard()).plugin(tauri_plugin_notification::init());
    // Desktop: one instance (C23): a second launch focuses the first, so two sync passes
    // or lock screens never race on the same data. Skipped when SELFKIT_DATA_DIR is set
    // (tests run several apps on separate data dirs), like Agenda.
    #[cfg(desktop)]
    let builder = if std::env::var_os("SELFKIT_DATA_DIR").is_none() {
        builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
    } else {
        builder
    };
    #[cfg(target_os = "android")]
    let builder = builder.plugin(mobile::plugin());
    builder
        .setup(|app| {
            #[cfg(mobile)]
            paths::set_data_dir(app.path().app_data_dir()?);
            // A damaged chrono.db / settings.db is set aside and started fresh instead of
            // failing the setup hook on every start (L25), like Agenda/Hisab/Dusk.
            let device = paths::device_id()?;
            let (store, n1) = recover::open(paths::app_db("chrono"), "chrono", &device, stats::MIGRATIONS)?;
            let (shared, n2) = recover::open(paths::app_db(selfkit_core::settings::APP), selfkit_core::settings::APP, &device, &[])?;
            app.manage(AppState { store: Mutex::new(store), shared: Mutex::new(shared), notice: Mutex::new(n1.or(n2)) });
            // Selfkit sync (selfkit-appsync): status, Sync now, settings, conflicts; desktop syncs on open.
            let sync = sync_handle()?;
            selfkit_appsync::tauri_cmds::spawn_on_open(app.handle(), &sync);
            app.manage(sync);
            // App lock (docs/components/app-lock.md): local PIN/passphrase; locked on start when set.
            app.manage(selfkit_appsync::lock_cmds::LockHandle::new("chrono", &paths::data_dir()));
            #[cfg(desktop)]
            {
                if let (Ok(sz), Some(w)) = (std::env::var("CHRONO_WINDOW_SIZE"), app.get_webview_window("main"))
                    && let Some((a, b)) = sz.split_once('x')
                    && let (Ok(wd), Ok(ht)) = (a.parse::<f64>(), b.parse::<f64>())
                {
                    let _ = w.set_size(tauri::LogicalSize::new(wd, ht));
                }
                start_desktop_timer(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            day_view, timeline, trends, report, websites, site, names, info, settings_get, settings_save, pause, resume, prefs_get, prefs_set,
            android_status, android_open, android_import, dev_hooks, startup_notice, sources, note_get, note_save, photo_save,
            photo_get, photo_export, theme_get, theme_set, app_changes,
            selfkit_appsync::tauri_cmds::sync_status, selfkit_appsync::tauri_cmds::sync_now,
            selfkit_appsync::tauri_cmds::sync_settings_set, selfkit_appsync::tauri_cmds::sync_server_get,
            selfkit_appsync::tauri_cmds::sync_server_set, selfkit_appsync::tauri_cmds::sync_server_test,
            selfkit_appsync::tauri_cmds::sync_encryption_setup, selfkit_appsync::tauri_cmds::sync_unlock,
            selfkit_appsync::tauri_cmds::sync_change_passphrase, selfkit_appsync::tauri_cmds::sync_conflicts,
            selfkit_appsync::tauri_cmds::sync_resolve, selfkit_appsync::tauri_cmds::sync_log,
            selfkit_appsync::lock_cmds::lock_status, selfkit_appsync::lock_cmds::lock_unlock,
            selfkit_appsync::lock_cmds::lock_unlock_biometric, selfkit_appsync::lock_cmds::lock_test_unlock,
            selfkit_appsync::lock_cmds::lock_enable, selfkit_appsync::lock_cmds::lock_change,
            selfkit_appsync::lock_cmds::lock_disable, selfkit_appsync::lock_cmds::lock_set_options,
            selfkit_appsync::lock_cmds::lock_away, selfkit_appsync::lock_cmds::lock_now, selfkit_appsync::lock_cmds::lock_reset
        ])
        .run(tauri::generate_context!())
        .expect("error while running Chrono");
}
