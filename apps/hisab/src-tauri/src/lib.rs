//! Hisab (com.selfkit.hisab): money app. Tauri commands over selfkit-core.
//! Docs: docs/apps/hisab/README.md (code map, data model).

pub mod csvimport;
pub mod currency;
pub mod export;
pub mod fx;
pub mod model;
pub mod photos;
pub mod recover;
pub mod recur;
pub mod stats;

use std::sync::Mutex;

use chrono::{Local, NaiveDate, TimeZone};
use selfkit_core::settings::Settings;
use selfkit_core::{Store, paths};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

use model::{LOANS, R, TRANSACTIONS, f, s};

pub struct AppState {
    store: Mutex<Store>,
    settings: Mutex<Store>,
    /// Set when a damaged DB was set aside at startup (shown once by the UI).
    notice: Mutex<Option<String>>,
    /// Last main currency read (also kept in kv `hisab.main`): used when the
    /// shared settings can't be read, instead of silently switching to USD.
    last_main: Mutex<String>,
}

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// The global main currency. Only the `main_currency` field is read (see
/// currency.rs); if it can't be read, the last known value is kept.
fn main_currency(state: &AppState) -> String {
    let read = currency::read_main(&state.settings.lock().unwrap());
    let mut last = state.last_main.lock().unwrap();
    match read {
        Some(c) => {
            if *last != c {
                *last = c.clone();
                let _ = state.store.lock().unwrap().kv_set("hisab.main", &c);
            }
            c
        }
        None => last.clone(),
    }
}

fn changed(app: &AppHandle) {
    let _ = app.emit("data-changed", ());
}

/// Startup info: main currency, today, app prefs.
#[tauri::command]
fn boot(state: State<AppState>) -> R<Value> {
    let main = main_currency(&state);
    let store = state.store.lock().unwrap();
    model::seed_defaults(&store, &main)?;
    let _ = model::merge_seeded_duplicates(&store); // C94
    currency::follow_main(&store, &main)?;
    let made = recur::generate(&store, &main, today())?;
    let prefs = store.kv_get("hisab.prefs").map_err(e)?.and_then(|v| serde_json::from_str::<Value>(&v).ok()).unwrap_or_else(|| json!({}));
    let draft = store.kv_get("hisab.draft").map_err(e)?.and_then(|v| serde_json::from_str::<Value>(&v).ok());
    Ok(json!({"main": main, "today": today().format("%Y-%m-%d").to_string(), "prefs": prefs, "generated": made, "draft": draft}))
}

/// Everything the screens need except the transaction list.
// async: runs off the main (UI) thread, so a busy database never freezes typing.
#[tauri::command(async)]
fn snapshot(state: State<AppState>, month: String) -> R<Value> {
    let main = main_currency(&state);
    let store = state.store.lock().unwrap();
    let d = stats::Data::load(&store, &main)?;
    let t = today();
    // Recent = last entered (created_at), falling back to the entry's own date/time.
    let mut recent = stats::filter_tx(&d, &stats::Filter { limit: Some(60), ..Default::default() });
    let key = |v: &Value| {
        v.get("created_at").and_then(Value::as_i64).unwrap_or_else(|| {
            let dt = format!("{} {}", s(v, "date"), if s(v, "time").is_empty() { "12:00" } else { s(v, "time") });
            chrono::NaiveDateTime::parse_from_str(&dt, "%Y-%m-%d %H:%M").ok().and_then(|x| Local.from_local_datetime(&x).earliest()).map(|x| x.timestamp_millis()).unwrap_or(0)
        })
    };
    recent.sort_by_key(|v| std::cmp::Reverse(key(v)));
    recent.truncate(8); // desktop Home shows 8 since it has no number pad (2026-11-21)
    Ok(json!({
        "main": main,
        "today": t.format("%Y-%m-%d").to_string(),
        "accounts": stats::accounts_view(&d),
        "categories": stats::categories_view(&d, &month),
        "loans": stats::loans_view(&d, t),
        "goals": stats::goals_view(&d),
        "recurring": stats::recurring_view(&d),
        "overview": stats::overview(&d, &month, t),
        "recent": recent,
        "rates": fx::raw(&store)?,
        "tags": all_tags(&d),
        "missing_rates": d.missing(),
        "stale_accounts": currency::stale_accounts(&store, &main)?,
    }))
}

fn all_tags(d: &stats::Data) -> Vec<String> {
    let mut m: std::collections::BTreeMap<String, usize> = Default::default();
    for t in &d.txs {
        for x in model::arr(t, "tags").iter().filter_map(Value::as_str) {
            *m.entry(x.to_string()).or_default() += 1;
        }
    }
    let mut v: Vec<(String, usize)> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.into_iter().map(|x| x.0).collect()
}

// async: runs off the main (UI) thread, so a busy database never freezes typing.
#[tauri::command(async)]
fn transactions(state: State<AppState>, filter: stats::Filter) -> R<Vec<Value>> {
    let main = main_currency(&state);
    let store = state.store.lock().unwrap();
    let d = stats::Data::load(&store, &main)?;
    Ok(stats::filter_tx(&d, &filter))
}

// async: runs off the main (UI) thread, so a busy database never freezes typing.
#[tauri::command(async)]
fn trends(state: State<AppState>, month: String) -> R<Value> {
    let main = main_currency(&state);
    let store = state.store.lock().unwrap();
    let d = stats::Data::load(&store, &main)?;
    Ok(stats::trends(&d, &month, today()))
}

/// Create (id = None) or replace a record after validation.
// async: runs off the main (UI) thread, so a busy database never freezes typing.
#[tauri::command(async)]
fn save(app: AppHandle, state: State<AppState>, collection: String, id: Option<String>, data: Value, draft: Option<Value>) -> R<Value> {
    let main = main_currency(&state);
    let rec = {
        let store = state.store.lock().unwrap();
        let mut data = data;
        if collection == TRANSACTIONS && s(&data, "kind") == "transfer" && f(&data, "to_amount") <= 0.0 {
            // Different-currency transfer without a typed amount: convert.
            let fx = fx::Fx::load(&store)?;
            let to_cur = model::get(&store, s(&data, "to_account"))?.map(|a| s(&a, "currency").to_string()).unwrap_or_else(|| main.clone());
            let cur = if s(&data, "currency").is_empty() { main.clone() } else { s(&data, "currency").to_string() };
            data["to_amount"] = json!(model::round2(fx.conv(f(&data, "amount"), &cur, &to_cur)));
        }
        if collection == TRANSACTIONS && data.get("created_at").is_none() {
            data["created_at"] = json!(selfkit_core::time::now_ms());
        }
        let v = model::normalize(&collection, &data, &main)?;
        let r = store.put(&collection, id.as_deref(), &v).map_err(e)?;
        if let Some(d) = draft.filter(|d| d.is_object()) {
            store.kv_set("hisab.draft", &d.to_string()).map_err(e)?;
        }
        model::flat(&r.id, &r.data)
    };
    if collection == LOANS {
        reminders::schedule(&app, &rec);
    }
    changed(&app);
    Ok(rec)
}

// async: runs off the main (UI) thread, so a busy database never freezes typing.
#[tauri::command(async)]
fn get(state: State<AppState>, id: String) -> R<Option<Value>> {
    model::get(&state.store.lock().unwrap(), &id)
}

// async: runs off the main (UI) thread, so a busy database never freezes typing.
#[tauri::command(async)]
fn remove(app: AppHandle, state: State<AppState>, id: String) -> R<()> {
    state.store.lock().unwrap().delete(&id).map_err(e)?;
    reminders::cancel(&app, &id);
    changed(&app);
    Ok(())
}

// async: runs off the main (UI) thread, so a busy database never freezes typing.
#[tauri::command(async)]
fn restore(app: AppHandle, state: State<AppState>, id: String) -> R<()> {
    let rec = {
        let store = state.store.lock().unwrap();
        store.restore(&id).map_err(e)?;
        store.get(&id).map_err(e)?
    };
    if let Some(r) = rec.filter(|r| r.collection == LOANS) {
        reminders::schedule(&app, &model::flat(&r.id, &r.data));
    }
    changed(&app);
    Ok(())
}

#[tauri::command]
fn generate_recurring(app: AppHandle, state: State<AppState>) -> R<usize> {
    let main = main_currency(&state);
    let n = recur::generate(&state.store.lock().unwrap(), &main, today())?;
    if n > 0 {
        changed(&app);
    }
    Ok(n)
}

/// CSV import, step 1: read the file text, guess (or use) the column mapping
/// and show what would be imported into `account`. Nothing is written.
#[tauri::command]
fn csv_preview(state: State<AppState>, text: String, mapping: Option<csvimport::Mapping>, account: String) -> R<Value> {
    let main = main_currency(&state);
    let t = csvimport::parse(&text)?;
    let m = mapping.unwrap_or_else(|| csvimport::guess(&t));
    let p = csvimport::preview(&state.store.lock().unwrap(), &t, &m, &account, &main)?;
    Ok(json!({"headers": t.headers, "sample": t.rows.iter().take(3).collect::<Vec<_>>(), "delimiter": t.delimiter, "count": t.rows.len(), "mapping": m, "preview": p}))
}

/// CSV import, step 2: store the previewed rows (errors and duplicates are
/// skipped). Returns the new ids; `csv_import_undo` removes them all at once.
#[tauri::command]
fn csv_import(app: AppHandle, state: State<AppState>, rows: Vec<csvimport::Row>) -> R<Vec<String>> {
    let main = main_currency(&state);
    let batch = format!("import-{}", selfkit_core::time::now_ms());
    let ids = csvimport::import(&state.store.lock().unwrap(), &rows, &main, &batch)?;
    changed(&app);
    Ok(ids)
}

#[tauri::command]
fn csv_import_undo(app: AppHandle, state: State<AppState>, ids: Vec<String>) -> R<usize> {
    let store = state.store.lock().unwrap();
    for id in &ids {
        store.delete(id).map_err(e)?;
    }
    drop(store);
    changed(&app);
    Ok(ids.len())
}

#[tauri::command]
fn rates_save(app: AppHandle, state: State<AppState>, base: String, rates: Value) -> R<Value> {
    let v = fx::save_fetched(&state.store.lock().unwrap(), &base, &rates)?;
    changed(&app);
    Ok(v)
}

#[tauri::command]
fn rates_manual(app: AppHandle, state: State<AppState>, currency: String, rate: Option<f64>) -> R<Value> {
    let v = fx::set_manual(&state.store.lock().unwrap(), &currency, rate)?;
    changed(&app);
    Ok(v)
}

fn export_dir() -> std::path::PathBuf {
    #[cfg(target_os = "android")]
    {
        let p = std::path::PathBuf::from("/storage/emulated/0/Download");
        if p.is_dir() {
            return p;
        }
    }
    #[cfg(not(target_os = "android"))]
    if let Some(d) = dirs::download_dir().filter(|d| d.is_dir()) {
        return d;
    }
    // Without xdg-user-dirs (bare i3) download_dir() is None, but ~/Downloads exists (L2-14).
    #[cfg(not(target_os = "android"))]
    if let Some(d) = dirs::home_dir().map(|h| h.join("Downloads")).filter(|d| d.is_dir()) {
        return d;
    }
    paths::data_dir().join("exports")
}

/// Write a plain CSV or PDF export; returns the file path.
#[tauri::command]
fn export_file(state: State<AppState>, kind: String, from: String, to: String) -> R<String> {
    let main = main_currency(&state);
    let bytes = {
        let store = state.store.lock().unwrap();
        let d = stats::Data::load(&store, &main)?;
        match kind.as_str() {
            "pdf" => export::pdf(&d, &from, &to, &today().format("%Y-%m-%d").to_string()),
            _ => export::csv(&d, &from, &to),
        }
    };
    let ext = if kind == "pdf" { "pdf" } else { "csv" };
    let range = match (from.as_str(), to.as_str()) {
        ("", "") => "all".to_string(),
        (a, b) => format!("{}_{}", if a.is_empty() { "start" } else { a }, if b.is_empty() { "today" } else { b }),
    };
    let write = |dir: std::path::PathBuf| -> std::io::Result<std::path::PathBuf> {
        std::fs::create_dir_all(&dir)?;
        let mut p = dir.join(format!("hisab-{range}.{ext}"));
        let mut i = 2;
        while p.exists() {
            p = dir.join(format!("hisab-{range}-{i}.{ext}"));
            i += 1;
        }
        std::fs::write(&p, &bytes)?;
        Ok(p)
    };
    let p = write(export_dir()).or_else(|_| write(paths::data_dir().join("exports"))).map_err(e)?;
    Ok(p.display().to_string())
}

/// Raw image bytes in the request body → photo id.
#[tauri::command]
fn photo_save(request: tauri::ipc::Request<'_>) -> R<String> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(b) => photos::save(b),
        tauri::ipc::InvokeBody::Json(v) => {
            let b: Vec<u8> = serde_json::from_value(v.clone()).map_err(e)?;
            photos::save(&b)
        }
    }
}

#[tauri::command]
fn photo_get(id: String) -> R<tauri::ipc::Response> {
    photos::load(&id).map(tauri::ipc::Response::new)
}

#[tauri::command]
fn photo_export(id: String) -> Result<String, String> {
    let bytes = photos::load(&id)?;
    let ext = id.rsplit_once('.').map(|(_, x)| x).unwrap_or("jpg");
    export_to_pictures(&id, ext, &bytes)
}

/// "Save to Pictures" from the photo viewer on desktop (Android saves through
/// the SelfkitPhotos bridge instead): copy into ~/Pictures/Selfkit/Hisab.
/// Docs: docs/components/photo-source.md.
fn export_to_pictures(id: &str, ext: &str, bytes: &[u8]) -> Result<String, String> {
    // `dirs` is a desktop-only dependency here; Android saves through the bridge.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let base = dirs::picture_dir().or_else(|| dirs::home_dir().map(|h| h.join("Pictures")));
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let base: Option<std::path::PathBuf> = None;
    let dir = std::env::var_os("SELFKIT_PICTURES_DIR")
        .map(std::path::PathBuf::from)
        .or(base)
        .ok_or("No Pictures folder")?
        .join("Selfkit")
        .join("Hisab");
    std::fs::create_dir_all(&dir).map_err(e)?;
    let stem: String = id.split('.').next().unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(40).collect();
    let path = dir.join(format!("Hisab_{stem}.{ext}"));
    std::fs::write(&path, bytes).map_err(e)?;
    Ok(path.display().to_string())
}

// async: runs off the main (UI) thread, so a busy database never freezes typing.
#[tauri::command(async)]
fn prefs_set(app: AppHandle, state: State<AppState>, prefs: Value) -> R<()> {
    state.store.lock().unwrap().kv_set("hisab.prefs", &prefs.to_string()).map_err(e)?;
    let _ = app.emit("prefs-changed", &prefs);
    Ok(())
}

/// The add-screen draft, kept in SQLite so Back, a kill or a restart never
/// loses typed input (webview localStorage may not be flushed in time).
// async: runs off the main (UI) thread, so a busy database never freezes typing.
#[tauri::command(async)]
fn draft_set(state: State<AppState>, draft: Value) -> R<()> {
    state.store.lock().unwrap().kv_set("hisab.draft", &draft.to_string()).map_err(e)
}

#[tauri::command]
fn settings_get(state: State<AppState>) -> R<Settings> {
    let mut st = Settings::load(&state.settings.lock().unwrap()).map_err(e)?;
    st.main_currency = main_currency(&state);
    Ok(st)
}

/// Save Hisab's quick-capture shortcut. Only that field is changed (core
/// `Settings::update`), so a stale copy can't undo the main currency or other
/// apps' settings; the main currency changes via `main_currency_set`.
#[tauri::command]
fn settings_set(app: AppHandle, state: State<AppState>, settings: Settings) -> R<()> {
    let accel = settings.shortcuts.get("add_expense").cloned().unwrap_or_default();
    let settings = Settings::update(&state.settings.lock().unwrap(), |s| {
        s.shortcuts.insert("add_expense".into(), accel);
    })
    .map_err(e)?;
    #[cfg(desktop)]
    shortcut::register(&app, &settings);
    let _ = app.emit("settings-changed", &settings);
    changed(&app);
    Ok(())
}

/// Change the global main currency; unused accounts that followed the old one
/// switch with it. Returns the used accounts still in the old currency so the
/// UI can offer to relabel them.
#[tauri::command]
fn main_currency_set(app: AppHandle, state: State<AppState>, code: String) -> R<currency::MainChange> {
    let old = main_currency(&state);
    let ch = currency::set_main(&state.settings.lock().unwrap(), &state.store.lock().unwrap(), &code, &old)?;
    main_currency(&state);
    if let Ok(st) = Settings::load(&state.settings.lock().unwrap()) {
        let _ = app.emit("settings-changed", &st);
    }
    changed(&app);
    Ok(ch)
}

/// Relabel an account (and its entries in the old currency) to another
/// currency, after the user said yes. Numbers are not converted.
#[tauri::command]
fn account_relabel(app: AppHandle, state: State<AppState>, id: String, currency: String) -> R<usize> {
    let n = currency::relabel_account(&state.store.lock().unwrap(), &id, &currency)?;
    changed(&app);
    Ok(n)
}

/// "Keep USD" on the relabel offer: the account's currency counts as chosen.
#[tauri::command]
fn account_keep_currency(app: AppHandle, state: State<AppState>, id: String) -> R<()> {
    currency::keep_currency(&state.store.lock().unwrap(), &id)?;
    changed(&app);
    Ok(())
}

#[tauri::command]
fn close_capture(app: AppHandle) {
    if let Some(w) = app.get_webview_window("capture") {
        let _ = w.close();
    }
}

/// Re-schedule every loan reminder (after turning notifications on).
#[tauri::command]
fn reschedule(app: AppHandle) {
    reminders::reschedule_all(&app);
}

/// Test hook for screenshots/automation: HISAB_DEMO="…". Unset in normal use.
#[tauri::command]
fn dev_hooks() -> Option<String> {
    std::env::var("HISAB_DEMO").ok()
}

/// Loan due-date reminders: `remind_days` before `due` at `remind_time`
/// (local). Android: exact alarms via the notification plugin (fire when the
/// app is closed). Desktop: a 20 s timer thread while the app runs.
mod reminders {
    use super::*;

    #[cfg_attr(desktop, allow(dead_code))]
    pub fn notification_id(id: &str) -> i32 {
        let mut h: u32 = 2166136261;
        for b in id.bytes() {
            h = (h ^ b as u32).wrapping_mul(16777619);
        }
        (h & 0x7fff_ffff) as i32
    }

    pub fn fire_at(l: &Value) -> Option<chrono::DateTime<chrono::Utc>> {
        if l.get("remind").and_then(Value::as_bool) == Some(false) {
            return None;
        }
        let due = model::date(l, "due")?;
        let repaid: f64 = model::arr(l, "repayments").iter().map(|r| f(r, "amount")).sum();
        if f(l, "amount") - repaid <= 0.005 {
            return None;
        }
        let t = chrono::NaiveTime::parse_from_str(s(l, "remind_time"), "%H:%M").unwrap_or(chrono::NaiveTime::from_hms_opt(9, 0, 0)?);
        let day = due - chrono::Duration::days(f(l, "remind_days") as i64);
        Local.from_local_datetime(&day.and_time(t)).earliest().map(|d| d.with_timezone(&chrono::Utc))
    }

    /// Loan reminders are off until the user turns them on (prefs.notify).
    pub fn notify_on(app: &AppHandle) -> bool {
        let state = app.state::<AppState>();
        let v = state.store.lock().unwrap().kv_get("hisab.prefs").ok().flatten();
        v.and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|p| p.get("notify").and_then(Value::as_bool)).unwrap_or(false)
    }

    pub fn title(l: &Value) -> String {
        let who = s(l, "person");
        if s(l, "direction") == "lent" { format!("{who} owes you") } else { format!("You owe {who}") }
    }

    pub fn body(l: &Value) -> String {
        let repaid: f64 = model::arr(l, "repayments").iter().map(|r| f(r, "amount")).sum();
        let rem = f(l, "amount") - repaid;
        let due = s(l, "due");
        let t = s(l, "title");
        let what = if t.is_empty() { String::new() } else { format!("{t} · ") };
        format!("{what}{:.2} {} left · due {due}", rem, s(l, "currency"))
    }

    #[cfg(mobile)]
    pub fn schedule(app: &AppHandle, l: &Value) {
        use tauri_plugin_notification::Schedule;
        let id = notification_id(s(l, "id"));
        let _ = app.notification().cancel(vec![id]);
        if !notify_on(app) {
            return;
        }
        let Some(at) = fire_at(l) else { return };
        if at <= chrono::Utc::now() {
            return;
        }
        let Ok(date) = time::OffsetDateTime::from_unix_timestamp(at.timestamp()) else { return };
        let r = app
            .notification()
            .builder()
            .id(id)
            .title(title(l))
            .body(body(l))
            .icon("ic_stat_hisab") // monochrome wallet, res/drawable/ic_stat_hisab.xml
            .schedule(Schedule::At { date, repeating: false, allow_while_idle: true })
            .show();
        if let Err(err) = r {
            eprintln!("schedule reminder: {err}");
        }
    }

    #[cfg(mobile)]
    pub fn cancel(app: &AppHandle, id: &str) {
        let _ = app.notification().cancel(vec![notification_id(id)]);
    }

    #[cfg(mobile)]
    pub fn reschedule_all(app: &AppHandle) {
        let list = {
            let state = app.state::<AppState>();
            model::all(&state.store.lock().unwrap(), LOANS).unwrap_or_default()
        };
        for l in &list {
            schedule(app, l);
        }
    }

    #[cfg(desktop)]
    pub fn schedule(_app: &AppHandle, _l: &Value) {}

    #[cfg(desktop)]
    pub fn cancel(_app: &AppHandle, _id: &str) {}

    #[cfg(desktop)]
    pub fn reschedule_all(_app: &AppHandle) {}

    #[cfg(desktop)]
    pub fn start_desktop_timer(app: AppHandle) {
        std::thread::spawn(move || {
            let mut last = chrono::Utc::now();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(20));
                let now = chrono::Utc::now();
                if !notify_on(&app) {
                    last = now;
                    continue;
                }
                let due: Vec<Value> = {
                    let state = app.state::<AppState>();
                    let store = state.store.lock().unwrap();
                    model::all(&store, LOANS)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|l| fire_at(l).is_some_and(|at| at > last && at <= now))
                        .collect()
                };
                for l in due {
                    let _ = app.notification().builder().title(title(&l)).body(body(&l)).show();
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

    pub fn open_capture(app: &AppHandle) {
        if let Some(w) = app.get_webview_window("capture") {
            let _ = w.show();
            let _ = w.set_focus();
            return;
        }
        let r = WebviewWindowBuilder::new(app, "capture", WebviewUrl::App("index.html?view=capture".into()))
            .title("Add expense")
            .inner_size(420.0, 390.0) // no number pad on desktop (2026-11-21), keyboard only
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
        if let Some(accel) = s.shortcuts.get("add_expense").filter(|a| !a.is_empty())
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
    let builder = tauri::Builder::default().plugin(selfkit_appsync::tauri_cmds::nav_guard());
    // Desktop: one instance (a second launch focuses the first, or opens its
    // popup for --capture), so reminders don't fire twice and the shortcut stays
    // registered. Skipped when SELFKIT_DATA_DIR is set (tests run several apps on
    // separate data dirs at once).
    #[cfg(desktop)]
    let builder = if std::env::var_os("SELFKIT_DATA_DIR").is_none() {
        builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|a| a == "--capture") {
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
            let (store, n1) = recover::open(paths::app_db("hisab"), "hisab", &device, model::MIGRATIONS)?;
            let (settings, n2) = recover::open(paths::app_db(selfkit_core::settings::APP), selfkit_core::settings::APP, &device, &[])?;
            // First start ever: the shared default (USD) until the user picks one.
            let last = currency::read_main(&settings)
                .or_else(|| store.kv_get("hisab.main").ok().flatten())
                .unwrap_or_else(|| Settings::default().main_currency);
            app.manage(AppState { store: Mutex::new(store), settings: Mutex::new(settings), notice: Mutex::new(n1.or(n2)), last_main: Mutex::new(last) });
            // Selfkit sync (selfkit-appsync): status, Sync now, settings, conflicts; desktop syncs on open.
            let sync = selfkit_appsync::tauri_cmds::default_handle("hisab", paths::data_dir())?;
            selfkit_appsync::tauri_cmds::spawn_on_open(app.handle(), &sync);
            app.manage(sync);
            // App lock (docs/components/app-lock.md): local PIN/passphrase; locked on start when set.
            app.manage(selfkit_appsync::lock_cmds::LockHandle::new("hisab", &paths::data_dir()));
            #[cfg(desktop)]
            {
                let s = Settings::load(&app.state::<AppState>().settings.lock().unwrap()).unwrap_or_default();
                shortcut::register(app.handle(), &s);
                if std::env::args().any(|a| a == "--capture") {
                    shortcut::open_capture(app.handle());
                }
                if let (Ok(sz), Some(w)) = (std::env::var("HISAB_WINDOW_SIZE"), app.get_webview_window("main"))
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
                std::thread::spawn(move || reminders::reschedule_all(&h));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            boot, snapshot, transactions, trends, save, get, remove, restore, generate_recurring, csv_preview, csv_import, csv_import_undo,
            rates_save, rates_manual, export_file, photo_save, photo_get, photo_export, prefs_set, draft_set, settings_get, settings_set, main_currency_set, account_relabel, account_keep_currency, close_capture,
            reschedule, dev_hooks, startup_notice,
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
        .expect("error while running Hisab");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_time() {
        let l = json!({"id":"x","person":"Sam","direction":"lent","amount":50,"currency":"USD","due":"2026-12-10","remind_days":2,"remind_time":"08:30","repayments":[]});
        let at = reminders::fire_at(&l).unwrap().with_timezone(&Local);
        assert_eq!(at.format("%Y-%m-%d %H:%M").to_string(), "2026-12-08 08:30");
        let mut paid = l.clone();
        paid["repayments"] = json!([{"date":"2026-11-01","amount":50}]);
        assert!(reminders::fire_at(&paid).is_none());
        assert_eq!(reminders::title(&l), "Sam owes you");
        assert!(reminders::body(&l).contains("50.00 USD left"));
    }
}
