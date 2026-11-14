//! Agenda (com.selfkit.agenda): Tauri commands over selfkit-core.
//! Docs: docs/apps/agenda/README.md (code map).

pub mod tasks;

use std::sync::Mutex;

use chrono::{Local, NaiveDate, TimeZone};
use selfkit_core::settings::Settings;
use selfkit_core::{Store, paths};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

pub struct AppState {
    store: Mutex<Store>,
    settings: Mutex<Store>,
}

type Res<T> = Result<T, String>;

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

fn today() -> NaiveDate {
    Local::now().date_naive()
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
    let t = tasks::toggle(&state.store.lock().unwrap(), &id, today()).map_err(e)?;
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
    settings.save(&state.settings.lock().unwrap()).map_err(e)?;
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

    /// Android: re-schedule all reminders at startup (after updates/restores).
    #[cfg(mobile)]
    pub fn reschedule_all(app: &AppHandle) {
        let list = {
            let state = app.state::<AppState>();
            tasks::list(&state.store.lock().unwrap(), tasks::Filter::Active).unwrap_or_default()
        };
        for t in &list {
            schedule(app, t);
        }
    }

    #[cfg(desktop)]
    pub fn schedule(_app: &AppHandle, _t: &tasks::TaskView) {}

    #[cfg(desktop)]
    pub fn cancel(_app: &AppHandle, _task_id: &str) {}

    /// Desktop: every 20 s, show reminders whose time passed since the last tick.
    #[cfg(desktop)]
    pub fn start_desktop_timer(app: AppHandle) {
        std::thread::spawn(move || {
            let mut last = chrono::Utc::now();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(20));
                let now = chrono::Utc::now();
                let due: Vec<tasks::TaskView> = {
                    let state = app.state::<AppState>();
                    let store = state.store.lock().unwrap();
                    tasks::list(&store, tasks::Filter::Active)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|t| fire_at_utc(t).is_some_and(|at| at > last && at <= now))
                        .collect()
                };
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
    let builder = tauri::Builder::default().plugin(tauri_plugin_notification::init());
    #[cfg(desktop)]
    let builder = builder.plugin(shortcut::plugin());
    builder
        .setup(|app| {
            #[cfg(mobile)]
            paths::set_data_dir(app.path().app_data_dir()?);
            let device = paths::device_id()?;
            let store = Store::open(paths::app_db("agenda"), "agenda", &device, tasks::MIGRATIONS)?;
            let settings = Store::open(paths::app_db(selfkit_core::settings::APP), selfkit_core::settings::APP, &device, &[])?;
            app.manage(AppState { store: Mutex::new(store), settings: Mutex::new(settings) });
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
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_tasks, create_task, update_task, replace_task, toggle_task, get_task, delete_task, restore_task, search,
            lists, add_list, prefs_get, prefs_set, settings_get, settings_set, close_capture, dev_hooks
        ])
        .run(tauri::generate_context!())
        .expect("error while running Agenda");
}
