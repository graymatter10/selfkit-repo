//! Reflect (com.selfkit.reflect): read-only stats across Chrono, Hisab,
//! Agenda and Dusk, plus the health of the agent and sync.
//! Docs: docs/apps/reflect/README.md (code map).

pub mod health;
pub mod model;
pub mod sources;

use std::sync::Mutex;

use selfkit_core::settings::Settings;
use selfkit_core::{Store, paths};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{Manager, State};

use model::{Ctx, Has, Ms, StreakDef};
use sources::{AgendaData, ChronoCache, HisabData, Night};

/// Reflect's own settings (synced record `reflect_settings` / id `settings`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RSettings {
    pub streaks: Vec<StreakDef>,
    /// Connections look back this many days (30, 90 or 365).
    pub lookback_days: u32,
    /// 0 = weeks start on Monday, 6 = on Sunday.
    pub week_start: u8,
}
impl Default for RSettings {
    fn default() -> Self {
        RSettings { streaks: model::default_streaks(), lookback_days: 90, week_start: 0 }
    }
}
pub const SETTINGS_COLL: &str = "reflect_settings";
pub const SETTINGS_ID: &str = "settings";
pub const PREFS_KV: &str = "reflect.prefs";

#[derive(Default)]
struct Cache {
    chrono: ChronoCache,
    hisab: Option<((u128, u64), String, HisabData)>,
    agenda: Option<((u128, u64), u16, AgendaData)>,
    dusk: Option<((u128, u64), Vec<Night>)>,
}

pub struct AppState {
    store: Mutex<Store>,
    settings: Mutex<Store>,
    cache: Mutex<Cache>,
}

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}
/// Now, or REFLECT_NOW="YYYY-MM-DDTHH:MM" (local; test hook for screenshots).
fn now() -> Ms {
    std::env::var("REFLECT_NOW")
        .ok()
        .and_then(|v| chrono::NaiveDateTime::parse_from_str(&v, "%Y-%m-%dT%H:%M").ok())
        .and_then(|t| chrono::TimeZone::from_local_datetime(&chrono::Local, &t).earliest())
        .map(|t| t.timestamp_millis())
        .unwrap_or_else(selfkit_core::time::now_ms)
}

pub fn currency_symbol(code: &str) -> String {
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

pub fn load_settings(store: &Store) -> RSettings {
    store
        .get(SETTINGS_ID)
        .ok()
        .flatten()
        .filter(|r| !r.deleted)
        .and_then(|r| serde_json::from_value(r.data).ok())
        .unwrap_or_default()
}

/// Refresh the per-app caches (cheap when nothing changed) and run `f` on a context.
fn with_ctx<T>(state: &AppState, f: impl FnOnce(&Ctx, &RSettings) -> T) -> T {
    let shared = Settings::load(&state.settings.lock().unwrap()).unwrap_or_default();
    let ds = shared.day_start_min;
    let rs = load_settings(&state.store.lock().unwrap());
    let mut c = state.cache.lock().unwrap();
    let mut has = Has::default();
    if let Some(s) = sources::open_other("chrono") {
        c.chrono.update(&s, ds);
        has.chrono = c.chrono.first.is_some();
        has.phone = c.chrono.has_phone;
    } else {
        c.chrono = ChronoCache::default();
    }
    match sources::stamp("hisab") {
        Some(st) => {
            let key = format!("{}|{ds}", shared.main_currency);
            if c.hisab.as_ref().is_none_or(|(s, k, _)| *s != st || *k != key)
                && let Some(s) = sources::open_other("hisab")
            {
                c.hisab = Some((st, key, sources::read_hisab(&s, &shared.main_currency, ds)));
            }
        }
        None => c.hisab = None,
    }
    match sources::stamp("agenda") {
        Some(st) => {
            if c.agenda.as_ref().is_none_or(|(s, d, _)| *s != st || *d != ds)
                && let Some(s) = sources::open_other("agenda")
            {
                c.agenda = Some((st, ds, sources::read_agenda(&s, ds)));
            }
        }
        None => c.agenda = None,
    }
    match sources::stamp("dusk") {
        Some(st) => {
            if c.dusk.as_ref().is_none_or(|(s, _)| *s != st)
                && let Some(s) = sources::open_other("dusk")
            {
                c.dusk = Some((st, sources::read_dusk(&s)));
            }
        }
        None => c.dusk = None,
    }
    has.hisab = c.hisab.as_ref().is_some_and(|h| h.2.first.is_some());
    has.agenda = c.agenda.as_ref().is_some_and(|a| a.2.first.is_some());
    has.dusk = c.dusk.as_ref().is_some_and(|d| !d.1.is_empty());
    let tz = selfkit_core::time::local_offset_min();
    let t = now();
    let today = sources::day_idx(t, tz, ds);
    let chrono = has.chrono.then_some(&c.chrono);
    let hisab = c.hisab.as_ref().map(|h| &h.2).filter(|_| has.hisab);
    let agenda = c.agenda.as_ref().map(|a| &a.2).filter(|_| has.agenda);
    let nights: &[Night] = c.dusk.as_ref().map(|d| d.1.as_slice()).unwrap_or(&[]);
    let days = model::build_days(today, 400, chrono, hisab, agenda, nights);
    let cx = Ctx { days, today, now: t, tz, day_start: ds, sym: currency_symbol(&shared.main_currency), has, chrono, hisab, agenda, nights };
    f(&cx, &rs)
}

#[derive(Serialize)]
struct Overview {
    today: model::Today,
    sym: String,
    now: Ms,
    mobile: bool,
}

#[tauri::command]
fn overview(state: State<AppState>) -> Res<Overview> {
    let ds = Settings::load(&state.settings.lock().unwrap()).map(|s| s.day_start_min).unwrap_or(0);
    let tz = selfkit_core::time::local_offset_min();
    let t = now();
    let from = sources::day_begin(sources::day_idx(t, tz, ds), tz, ds);
    let sess = sources::open_other("chrono").map(|s| sources::sessions_between(&s, from, from + model::DAY)).unwrap_or_default();
    Ok(with_ctx(&state, |cx, _| Overview { today: model::today(cx, &sess), sym: cx.sym.clone(), now: t, mobile: cfg!(mobile) }))
}

#[tauri::command]
fn streaks(state: State<AppState>) -> Res<Vec<model::StreakView>> {
    Ok(with_ctx(&state, |cx, rs| rs.streaks.iter().filter(|d| d.on).map(|d| model::streak(cx, d)).collect()))
}

#[derive(Serialize)]
struct ConnOut {
    items: Vec<model::Conn>,
    lookback_days: u32,
    sym: String,
}

#[tauri::command]
fn connections(state: State<AppState>) -> Res<ConnOut> {
    Ok(with_ctx(&state, |cx, rs| ConnOut { items: model::connections(cx, rs.lookback_days as i32), lookback_days: rs.lookback_days, sym: cx.sym.clone() }))
}

#[tauri::command]
fn review(state: State<AppState>, kind: String, offset: i32) -> Res<model::Review> {
    let kind = if kind == "month" { "month" } else { "week" };
    Ok(with_ctx(&state, |cx, rs| model::review(cx, kind, offset.min(0), rs.week_start, &rs.streaks)))
}

#[tauri::command]
fn health() -> Res<health::Health> {
    Ok(health::read(now().max(selfkit_core::time::now_ms())))
}

#[tauri::command]
async fn agent_cmd(cmd: String) -> Res<Value> {
    tauri::async_runtime::spawn_blocking(move || health::agent_cmd(&cmd)).await.map_err(e)?
}

#[derive(Serialize)]
struct SourcesOut {
    has: Has,
    found: Vec<(String, bool)>,
    first: Vec<(String, Option<String>)>,
    mobile: bool,
}

#[tauri::command]
fn sources_status(state: State<AppState>) -> Res<SourcesOut> {
    let found: Vec<(String, bool)> = ["chrono", "hisab", "agenda", "dusk"].iter().map(|a| (a.to_string(), paths::app_db(a).exists())).collect();
    Ok(with_ctx(&state, |cx, _| {
        let first = vec![
            ("chrono".into(), cx.chrono.and_then(|c| c.first).map(model::date_str)),
            ("hisab".into(), cx.hisab.and_then(|h| h.first).map(model::date_str)),
            ("agenda".into(), cx.agenda.and_then(|a| a.first).map(model::date_str)),
            ("dusk".into(), cx.nights.first().map(|n| n.night.clone())),
        ];
        SourcesOut { has: cx.has, found, first, mobile: cfg!(mobile) }
    }))
}

#[tauri::command]
fn settings_get(state: State<AppState>) -> Res<RSettings> {
    Ok(load_settings(&state.store.lock().unwrap()))
}

#[tauri::command]
fn settings_set(state: State<AppState>, settings: RSettings) -> Res<RSettings> {
    let mut s = settings;
    s.lookback_days = match s.lookback_days {
        0..=45 => 30,
        46..=180 => 90,
        _ => 365,
    };
    s.week_start = if s.week_start == 6 { 6 } else { 0 };
    for d in &mut s.streaks {
        d.value = match d.kind.as_str() {
            "phone_under" | "screen_under" => d.value.clamp(15.0, 16.0 * 60.0),
            "asleep_by" => (d.value as i64).rem_euclid(1440) as f64,
            "tasks_min" => d.value.clamp(1.0, 50.0).round(),
            _ => 0.0,
        };
    }
    state.store.lock().unwrap().put(SETTINGS_COLL, Some(SETTINGS_ID), &serde_json::to_value(&s).map_err(e)?).map_err(e)?;
    Ok(s)
}

#[derive(Serialize)]
struct Shared {
    day_start_min: u16,
    main_currency: String,
}

#[tauri::command]
fn shared_get(state: State<AppState>) -> Res<Shared> {
    let s = Settings::load(&state.settings.lock().unwrap()).map_err(e)?;
    Ok(Shared { day_start_min: s.day_start_min, main_currency: s.main_currency })
}

/// Per-device prefs (not synced): `{theme: "system"|"light"|"dark"}`.
#[tauri::command]
fn prefs_get(state: State<AppState>) -> Res<Value> {
    let v = state.store.lock().unwrap().kv_get(PREFS_KV).map_err(e)?;
    Ok(v.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({"theme": "system"})))
}

#[tauri::command]
fn prefs_set(state: State<AppState>, prefs: Value) -> Res<()> {
    state.store.lock().unwrap().kv_set(PREFS_KV, &prefs.to_string()).map_err(e)
}

/// Test hook for screenshots/automation: REFLECT_DEMO="…". Unset in normal use.
#[tauri::command]
fn dev_hooks() -> Option<String> {
    std::env::var("REFLECT_DEMO").ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(selfkit_appsync::tauri_cmds::nav_guard())
        .setup(|app| {
            #[cfg(mobile)]
            paths::set_data_dir(app.path().app_data_dir()?);
            let device = paths::device_id()?;
            let store = Store::open(paths::app_db("reflect"), "reflect", &device, &[])?;
            let settings = Store::open(paths::app_db(selfkit_core::settings::APP), selfkit_core::settings::APP, &device, &[])?;
            app.manage(AppState { store: Mutex::new(store), settings: Mutex::new(settings), cache: Mutex::new(Cache::default()) });
            // Selfkit sync (selfkit-appsync): status, Sync now, settings, conflicts; desktop syncs on open.
            let sync = selfkit_appsync::tauri_cmds::default_handle("reflect", paths::data_dir())?;
            selfkit_appsync::tauri_cmds::spawn_on_open(app.handle(), &sync);
            app.manage(sync);
            // App lock (docs/components/app-lock.md): local PIN/passphrase; locked on start when set.
            app.manage(selfkit_appsync::lock_cmds::LockHandle::new("reflect", &paths::data_dir()));
            #[cfg(desktop)]
            if let (Ok(sz), Some(w)) = (std::env::var("REFLECT_WINDOW_SIZE"), app.get_webview_window("main"))
                && let Some((a, b)) = sz.split_once('x')
                && let (Ok(wd), Ok(ht)) = (a.parse::<f64>(), b.parse::<f64>())
            {
                let _ = w.set_size(tauri::LogicalSize::new(wd, ht));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            overview,
            streaks,
            connections,
            review,
            health,
            agent_cmd,
            sources_status,
            settings_get,
            settings_set,
            shared_get,
            prefs_get,
            prefs_set,
            dev_hooks,
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
        .expect("error while running Reflect");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_and_roundtrip() {
        let s = Store::open_in_memory("reflect", "d", &[]).unwrap();
        let d = load_settings(&s);
        assert_eq!(d.lookback_days, 90);
        assert_eq!(d.streaks.len(), 5);
        let mut x = d.clone();
        x.week_start = 6;
        s.put(SETTINGS_COLL, Some(SETTINGS_ID), &serde_json::to_value(&x).unwrap()).unwrap();
        assert_eq!(load_settings(&s).week_start, 6);
    }
}
