//! Agenda (com.selfkit.agenda): Tauri commands over selfkit-core.
//! Docs: docs/apps/agenda/README.md (code map).

pub mod focus;
pub mod ics;
pub mod photos;
pub mod recover;
pub mod tasks;

use std::sync::Mutex;

use base64::Engine;
use chrono::{Local, TimeZone};
use selfkit_core::settings::Settings;
use selfkit_core::{Store, paths};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

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


/// Notify windows and (re)schedule the task's reminder.
fn changed(app: &AppHandle, task: Option<&tasks::TaskView>, removed_id: Option<&str>) {
    let _ = app.emit("tasks-changed", ());
    if let Some(t) = task {
        reminders::schedule(app, t);
    }
    if let Some(id) = removed_id {
        reminders::cancel(app, id);
    }
}

#[tauri::command]
fn list_tasks(state: State<AppState>, filter: tasks::Filter) -> Res<Vec<tasks::TaskView>> {
    tasks::list(&state.store.lock().unwrap(), filter).map_err(e)
}

#[tauri::command]
fn create_task(app: AppHandle, state: State<AppState>, task: tasks::NewTask) -> Res<tasks::TaskView> {
    let t = tasks::create(&state.store.lock().unwrap(), task).map_err(e)?;
    changed(&app, Some(&t), None);
    Ok(t)
}

#[tauri::command]
fn update_task(app: AppHandle, state: State<AppState>, id: String, patch: Map<String, Value>) -> Res<tasks::TaskView> {
    let t = tasks::update(&state.store.lock().unwrap(), &id, &patch).map_err(e)?;
    changed(&app, Some(&t), None);
    Ok(t)
}

#[tauri::command]
fn replace_task(app: AppHandle, state: State<AppState>, id: String, data: Value) -> Res<tasks::TaskView> {
    let t = tasks::replace(&state.store.lock().unwrap(), &id, &data).map_err(e)?;
    changed(&app, Some(&t), None);
    Ok(t)
}

#[tauri::command]
fn toggle_task(app: AppHandle, state: State<AppState>, id: String) -> Res<tasks::TaskView> {
    let t = tasks::toggle(&state.store.lock().unwrap(), &id, Local::now().naive_local()).map_err(e)?;
    changed(&app, Some(&t), None);
    Ok(t)
}

#[tauri::command]
fn get_task(state: State<AppState>, id: String) -> Res<Option<tasks::TaskView>> {
    tasks::get(&state.store.lock().unwrap(), &id).map_err(e)
}

#[tauri::command]
fn delete_task(app: AppHandle, state: State<AppState>, id: String) -> Res<()> {
    state.store.lock().unwrap().delete(&id).map_err(e)?;
    changed(&app, None, Some(&id));
    Ok(())
}

#[tauri::command]
fn restore_task(app: AppHandle, state: State<AppState>, id: String) -> Res<()> {
    let t = {
        let store = state.store.lock().unwrap();
        store.restore(&id).map_err(e)?;
        tasks::get(&store, &id).map_err(e)?
    };
    changed(&app, t.as_ref(), None);
    Ok(())
}

#[tauri::command]
fn search(state: State<AppState>, q: String) -> Res<Vec<tasks::TaskView>> {
    tasks::search(&state.store.lock().unwrap(), &q).map_err(e)
}

#[tauri::command]
fn lists(state: State<AppState>) -> Res<Vec<tasks::ListInfo>> {
    tasks::lists(&state.store.lock().unwrap()).map_err(e)
}

#[tauri::command]
fn add_list(app: AppHandle, state: State<AppState>, name: String) -> Res<String> {
    let id = tasks::add_list(&state.store.lock().unwrap(), &name).map_err(e)?;
    let _ = app.emit("tasks-changed", ());
    Ok(id)
}

/// Completed-task history (newest first).
#[tauri::command]
fn history(state: State<AppState>) -> Res<Vec<tasks::Completion>> {
    tasks::history(&state.store.lock().unwrap()).map_err(e)
}

// ---- Focus (Pomodoro) ----

/// Current session; a session that reached its end is logged and returned as `finished`.
#[derive(serde::Serialize)]
struct FocusState {
    current: Option<focus::Session>,
    finished: Option<focus::Session>,
}

#[tauri::command]
fn focus_state(app: AppHandle, state: State<AppState>) -> Res<FocusState> {
    let store = state.store.lock().unwrap();
    let finished = focus::finish_if_due(&store, selfkit_core::time::now_ms()).map_err(e)?;
    if finished.is_some() {
        let _ = app.emit("focus-changed", ());
    }
    Ok(FocusState { current: focus::current(&store).map_err(e)?, finished })
}

#[tauri::command]
fn focus_start(app: AppHandle, state: State<AppState>, task: Option<String>, title: String, kind: String, minutes: i64) -> Res<focus::Session> {
    let s = focus::start(&state.store.lock().unwrap(), task, &title, &kind, minutes, selfkit_core::time::now_ms()).map_err(e)?;
    reminders::schedule_focus(&app, &s);
    let _ = app.emit("focus-changed", ());
    Ok(s)
}

#[tauri::command]
fn focus_stop(app: AppHandle, state: State<AppState>) -> Res<()> {
    focus::stop(&state.store.lock().unwrap(), selfkit_core::time::now_ms()).map_err(e)?;
    reminders::cancel_focus(&app);
    let _ = app.emit("focus-changed", ());
    Ok(())
}

#[tauri::command]
fn focus_sessions(state: State<AppState>) -> Res<Vec<focus::Logged>> {
    focus::sessions(&state.store.lock().unwrap()).map_err(e)
}

// ---- Photos ----

/// Save a photo (base64 of the already scaled image); returns its id.
#[tauri::command]
fn photo_save(state: State<AppState>, data: String, mime: String, w: u32, h: u32) -> Res<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.as_bytes()).map_err(e)?;
    photos::save(&state.store.lock().unwrap(), &bytes, &mime, w, h).map_err(e)
}

/// A photo as a data: URL.
#[tauri::command]
fn photo_get(state: State<AppState>, id: String) -> Res<String> {
    let (mime, bytes) = photos::load(&state.store.lock().unwrap(), &id).map_err(e)?;
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[tauri::command]
fn photo_export(state: State<AppState>, id: String) -> Res<String> {
    let (mime, bytes) = photos::load(&state.store.lock().unwrap(), &id).map_err(e)?;
    export_to_pictures(&id, ext_for(&mime), &bytes)
}

/// "Save to Pictures" from the photo viewer on desktop (Android saves through
/// the SelfkitPhotos bridge instead): copy into ~/Pictures/Selfkit/Agenda.
/// Docs: docs/components/photo-source.md.
fn export_to_pictures(id: &str, ext: &str, bytes: &[u8]) -> Result<String, String> {
    let dir = std::env::var_os("SELFKIT_PICTURES_DIR")
        .map(std::path::PathBuf::from)
        .or_else(dirs::picture_dir)
        .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
        .ok_or("No Pictures folder")?
        .join("Selfkit")
        .join("Agenda");
    std::fs::create_dir_all(&dir).map_err(e)?;
    let stem: String = id.split('.').next().unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(40).collect();
    let path = dir.join(format!("Agenda_{stem}.{ext}"));
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

// ---- .ics ----

#[tauri::command]
fn ics_export(state: State<AppState>, include_done: bool) -> Res<String> {
    let store = state.store.lock().unwrap();
    let filter = if include_done { tasks::Filter::All } else { tasks::Filter::Active };
    let list = tasks::list(&store, filter).map_err(e)?;
    Ok(ics::export(&list, &tasks::lists(&store).map_err(e)?))
}

#[tauri::command]
fn ics_import(app: AppHandle, state: State<AppState>, text: String) -> Res<ics::ImportResult> {
    let r = ics::import(&state.store.lock().unwrap(), &text).map_err(e)?;
    let _ = app.emit("tasks-changed", ());
    #[cfg(mobile)]
    {
        let h = app.clone();
        std::thread::spawn(move || reminders::reschedule_all(&h));
    }
    Ok(r)
}

/// Desktop: write an export into the Downloads folder; returns the path.
/// (Android saves through the `AgendaFiles` bridge in MainActivity.kt.)
#[tauri::command]
fn save_export(name: String, text: String) -> Res<String> {
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    let dir = std::env::var("AGENDA_EXPORT_DIR")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(dirs::download_dir)
        .or_else(dirs::home_dir)
        .ok_or("No Downloads folder")?;
    std::fs::create_dir_all(&dir).map_err(e)?;
    let path = dir.join(safe);
    std::fs::write(&path, text).map_err(e)?;
    Ok(path.display().to_string())
}

/// App preferences (theme, default reminder, default severity) in agenda.db kv.
#[tauri::command]
fn prefs_get(state: State<AppState>) -> Res<Value> {
    let v = state.store.lock().unwrap().kv_get("agenda.prefs").map_err(e)?;
    Ok(v.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_else(|| serde_json::json!({})))
}

#[tauri::command]
fn prefs_set(app: AppHandle, state: State<AppState>, prefs: Value) -> Res<()> {
    state.store.lock().unwrap().kv_set("agenda.prefs", &prefs.to_string()).map_err(e)?;
    let _ = app.emit("prefs-changed", &prefs);
    Ok(())
}

/// Shared Selfkit settings (used here for the quick-capture shortcut).
#[tauri::command]
fn settings_get(state: State<AppState>) -> Res<Settings> {
    Settings::load(&state.settings.lock().unwrap()).map_err(e)
}

#[tauri::command]
fn settings_set(app: AppHandle, state: State<AppState>, settings: Settings) -> Res<()> {
    // Agenda only edits its own shortcut; reload + change just that, so a copy
    // loaded at startup can't undo other apps' changes (e.g. main currency).
    let accel = settings.shortcuts.get("add_task").cloned().unwrap_or_default();
    let settings = Settings::update(&state.settings.lock().unwrap(), |s| {
        s.shortcuts.insert("add_task".into(), accel);
    })
    .map_err(e)?;
    #[cfg(desktop)]
    shortcut::register(&app, &settings);
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

#[tauri::command]
fn close_capture(app: AppHandle) {
    if let Some(w) = app.get_webview_window("capture") {
        let _ = w.close();
    }
}

/// A one-time startup message (damaged data file recovered), or None.
#[tauri::command]
fn startup_notice(state: State<AppState>) -> Option<String> {
    state.notice.lock().unwrap().take()
}

/// Test hook for screenshots/automation: AGENDA_DEMO="…". Unset in normal use.
#[tauri::command]
fn dev_hooks() -> Option<String> {
    std::env::var("AGENDA_DEMO").ok()
}

/// Reminder notifications. Android: scheduled with AlarmManager through the
/// notification plugin (fires even when the app is closed). Desktop: a timer
/// thread while the app runs (the agent will take this over later).
mod reminders {
    use super::*;

    fn fire_at_utc(t: &tasks::TaskView) -> Option<chrono::DateTime<chrono::Utc>> {
        let local = tasks::reminder_at(t)?;
        Local.from_local_datetime(&local).earliest().map(|d| d.with_timezone(&chrono::Utc))
    }

    pub fn body(t: &tasks::TaskView) -> String {
        let start = t.data.get("start").and_then(Value::as_str).unwrap_or("");
        match start.split('T').nth(1) {
            Some(hm) => format!("Starts at {hm}"),
            None => "Today".into(),
        }
    }

    fn title(t: &tasks::TaskView) -> String {
        t.data.get("title").and_then(Value::as_str).unwrap_or("Task").to_string()
    }

    #[cfg(mobile)]
    pub fn schedule(app: &AppHandle, t: &tasks::TaskView) {
        use tauri_plugin_notification::Schedule;
        let id = tasks::notification_id(&t.id);
        let _ = app.notification().cancel(vec![id]);
        let Some(at) = fire_at_utc(t) else { return };
        if at <= chrono::Utc::now() {
            return;
        }
        let Ok(date) = time::OffsetDateTime::from_unix_timestamp(at.timestamp()) else { return };
        let r = app
            .notification()
            .builder()
            .id(id)
            .title(title(t))
            .body(body(t))
            .icon("ic_stat_agenda") // monochrome checkbox, res/drawable/ic_stat_agenda.xml (P3)
            .schedule(Schedule::At { date, repeating: false, allow_while_idle: true })
            .show();
        if let Err(err) = r {
            eprintln!("schedule reminder: {err}");
        }
    }

    #[cfg(mobile)]
    pub fn cancel(app: &AppHandle, task_id: &str) {
        let _ = app.notification().cancel(vec![tasks::notification_id(task_id)]);
    }

    /// Android: bring the scheduled alarms in line with the tasks — at startup and after
    /// every sync that brought changes (a task completed, deleted or moved on another
    /// device must stop notifying here; new ones get scheduled). Alarms of tasks that are
    /// no longer active are cancelled first. If the same task has a reminder on two
    /// phones, both phones notify (each schedules its own alarm).
    #[cfg(mobile)]
    pub fn reschedule_all(app: &AppHandle) {
        let list = {
            let state = app.state::<AppState>();
            tasks::list(&state.store.lock().unwrap(), tasks::Filter::Active).unwrap_or_default()
        };
        let wanted: std::collections::HashSet<i32> = list.iter().map(|t| tasks::notification_id(&t.id)).collect();
        if let Ok(pending) = app.notification().pending() {
            let stale: Vec<i32> = pending.iter().map(|p| p.id()).filter(|id| *id != FOCUS_NOTIFICATION && !wanted.contains(id)).collect();
            if !stale.is_empty() {
                let _ = app.notification().cancel(stale);
            }
        }
        for t in &list {
            schedule(app, t);
        }
    }

    #[cfg_attr(desktop, allow(dead_code))]
    pub const FOCUS_NOTIFICATION: i32 = 1_999_000_001;

    pub fn focus_text(s: &focus::Session) -> (String, String) {
        if s.kind == "break" {
            ("Break is over".into(), "Ready for the next focus session?".into())
        } else {
            let t = if s.title.is_empty() { "Focus session".to_string() } else { s.title.clone() };
            (format!("Focus done · {t}"), format!("{} min of focus. Time for a break.", s.planned_min))
        }
    }

    #[cfg(mobile)]
    pub fn schedule_focus(app: &AppHandle, s: &focus::Session) {
        use tauri_plugin_notification::Schedule;
        let _ = app.notification().cancel(vec![FOCUS_NOTIFICATION]);
        let Ok(date) = time::OffsetDateTime::from_unix_timestamp(s.end_ms / 1000) else { return };
        let (title, body) = focus_text(s);
        let r = app
            .notification()
            .builder()
            .id(FOCUS_NOTIFICATION)
            .title(title)
            .body(body)
            .icon("ic_stat_agenda") // monochrome checkbox, res/drawable/ic_stat_agenda.xml (P3)
            .schedule(Schedule::At { date, repeating: false, allow_while_idle: true })
            .show();
        if let Err(err) = r {
            eprintln!("schedule focus: {err}");
        }
    }

    #[cfg(mobile)]
    pub fn cancel_focus(app: &AppHandle) {
        let _ = app.notification().cancel(vec![FOCUS_NOTIFICATION]);
    }

    /// Desktop: the timer thread shows the notification when the session ends.
    #[cfg(desktop)]
    pub fn schedule_focus(_app: &AppHandle, _s: &focus::Session) {}

    #[cfg(desktop)]
    pub fn cancel_focus(_app: &AppHandle) {}

    #[cfg(desktop)]
    pub fn schedule(_app: &AppHandle, _t: &tasks::TaskView) {}

    #[cfg(desktop)]
    pub fn cancel(_app: &AppHandle, _task_id: &str) {}

    /// Desktop: every 5 s, show reminders whose time passed since the last tick,
    /// and finish (log + notify) a focus session that reached its end.
    #[cfg(desktop)]
    pub fn start_desktop_timer(app: AppHandle) {
        std::thread::spawn(move || {
            let mut last = chrono::Utc::now();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let now = chrono::Utc::now();
                let (due, finished): (Vec<tasks::TaskView>, _) = {
                    let state = app.state::<AppState>();
                    let store = state.store.lock().unwrap();
                    let finished = focus::finish_if_due(&store, now.timestamp_millis()).ok().flatten();
                    (
                    tasks::list(&store, tasks::Filter::Active)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|t| fire_at_utc(t).is_some_and(|at| at > last && at <= now))
                        .collect(),
                    finished)
                };
                if let Some(s) = finished {
                    let (title, body) = focus_text(&s);
                    let _ = app.notification().builder().title(title).body(body).show();
                    let _ = app.emit("focus-changed", ());
                }
                for t in due {
                    let _ = app.notification().builder().title(title(&t)).body(body(&t)).show();
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
            .title("New task")
            .inner_size(520.0, 360.0)
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
        if let Some(accel) = s.shortcuts.get("add_task").filter(|a| !a.is_empty())
            && let Err(e) = gs.register(accel.as_str())
        {
            eprintln!("could not register shortcut {accel}: {e}");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(selfkit_appsync::tauri_cmds::nav_guard());
    // Desktop: one instance (a second launch focuses the first or opens its
    // capture popup), so reminders don't fire twice and the shortcut stays
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
    let builder = builder.plugin(tauri_plugin_notification::init()).plugin(tauri_plugin_opener::init());
    #[cfg(desktop)]
    let builder = builder.plugin(shortcut::plugin());
    builder
        .setup(|app| {
            #[cfg(mobile)]
            paths::set_data_dir(app.path().app_data_dir()?);
            let device = paths::device_id()?;
            let (store, n1) = recover::open(paths::app_db("agenda"), "agenda", &device, tasks::MIGRATIONS)?;
            let (settings, n2) = recover::open(paths::app_db(selfkit_core::settings::APP), selfkit_core::settings::APP, &device, &[])?;
            app.manage(AppState { store: Mutex::new(store), settings: Mutex::new(settings), notice: Mutex::new(n1.or(n2)) });
            // Selfkit sync (selfkit-appsync): status, Sync now, settings, conflicts; desktop syncs on open.
            let sync = selfkit_appsync::tauri_cmds::default_handle("agenda", paths::data_dir())?;
            selfkit_appsync::tauri_cmds::spawn_on_open(app.handle(), &sync);
            app.manage(sync);
            // App lock (docs/components/app-lock.md): local PIN/passphrase; locked on start when set.
            app.manage(selfkit_appsync::lock_cmds::LockHandle::new("agenda", &paths::data_dir()));
            #[cfg(desktop)]
            {
                let s = Settings::load(&app.state::<AppState>().settings.lock().unwrap()).unwrap_or_default();
                shortcut::register(app.handle(), &s);
                if std::env::args().any(|a| a == "--capture") {
                    shortcut::open_capture(app.handle());
                }
                if let (Ok(sz), Some(w)) = (std::env::var("AGENDA_WINDOW_SIZE"), app.get_webview_window("main"))
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
                // After a sync pass brought changes (selfkit-appsync emits "data-changed"):
                // reconcile the alarms with the synced tasks.
                use tauri::Listener;
                let h = app.handle().clone();
                app.listen("data-changed", move |_| {
                    let h = h.clone();
                    std::thread::spawn(move || reminders::reschedule_all(&h));
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_tasks, create_task, update_task, replace_task, toggle_task, get_task, delete_task, restore_task, search,
            lists, add_list, prefs_get, prefs_set, settings_get, settings_set, close_capture, dev_hooks, history,
            focus_state, focus_start, focus_stop, focus_sessions, photo_save, photo_get, photo_export, ics_export, ics_import, save_export, startup_notice,
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
        .expect("error while running Agenda");
}
