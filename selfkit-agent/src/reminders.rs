//! Desktop reminders for Agenda tasks while the Agenda app is closed (Agenda's
//! own timer covers the time it is open, so the agent stays quiet then).
//! Reads `agenda.db` read-only every 20 s: `tasks` with `reminder_min`, a
//! `start` ("YYYY-MM-DDTHH:MM" local, or "YYYY-MM-DD" all-day → 09:00) and no
//! `done_at`. Fired reminders are remembered in `<data_dir>/agent-reminders.json`
//! so a restart never repeats one. Reminders missed by up to 30 min (boot,
//! suspend) still fire.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use selfkit_core::{Store, paths, time};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::Shared;

const TICK_SECS: u64 = 20;
const LATE_MS: i64 = 30 * 60_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Due {
    pub id: String,
    pub at: i64,
    pub title: String,
    pub body: String,
}

/// UTC ms when a task's reminder fires (same rule as Agenda's `reminder_at`).
pub fn fire_at(data: &Value) -> Option<i64> {
    if data.get("done_at").is_some_and(|v| !v.is_null()) {
        return None;
    }
    let mins = data.get("reminder_min").and_then(Value::as_i64)?;
    let start = data.get("start").and_then(Value::as_str)?;
    let base = if let Ok(dt) = NaiveDateTime::parse_from_str(start, "%Y-%m-%dT%H:%M") {
        dt
    } else {
        NaiveDate::parse_from_str(start, "%Y-%m-%d").ok()?.and_time(NaiveTime::from_hms_opt(9, 0, 0)?)
    };
    let local = base - Duration::minutes(mins);
    Local.from_local_datetime(&local).earliest().map(|d| d.timestamp_millis())
}

fn body(data: &Value) -> String {
    let start = data.get("start").and_then(Value::as_str).unwrap_or("");
    match start.split('T').nth(1) {
        Some(hm) => format!("Starts at {hm}"),
        None => "Today".into(),
    }
}

/// Tasks whose reminder time is in (after, now].
pub fn due_between(store: &Store, after: i64, now: i64) -> Result<Vec<Due>> {
    let mut st = store.conn().prepare(
        "SELECT id, data FROM records WHERE collection='tasks' AND deleted=0
         AND json_extract(data,'$.reminder_min') IS NOT NULL",
    )?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = vec![];
    for (id, data) in rows.flatten() {
        let Ok(d) = serde_json::from_str::<Value>(&data) else { continue };
        if let Some(at) = fire_at(&d)
            && at > after
            && at <= now
        {
            let title = d.get("title").and_then(Value::as_str).unwrap_or("Task").to_string();
            out.push(Due { id, at, title, body: body(&d) });
        }
    }
    out.sort_by_key(|d| d.at);
    Ok(out)
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct RemState {
    last_check: i64,
    /// task id → fire time already shown.
    fired: BTreeMap<String, i64>,
}

fn state_path() -> std::path::PathBuf {
    paths::data_dir().join("agent-reminders.json")
}

impl RemState {
    fn load() -> Self {
        std::fs::read(state_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    fn save(&self) {
        if let Ok(b) = serde_json::to_vec(self) {
            let _ = std::fs::write(state_path(), b);
        }
    }
}

/// Is the Agenda app itself running (it shows its own reminders then)?
pub fn agenda_running() -> bool {
    #[cfg(target_os = "linux")]
    {
        let Ok(rd) = std::fs::read_dir("/proc") else { return false };
        return rd.flatten().any(|e| {
            e.file_name().to_str().is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
                && std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "agenda")
        });
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        return std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq agenda.exe", "/NH"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("agenda.exe"));
    }
    #[allow(unreachable_code)]
    false
}

/// Show a desktop notification (Linux: notify-send; Windows: a toast via PowerShell).
pub fn notify(title: &str, body: &str) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\'', "''");
        let script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime]>$null;\
             $x=[Windows.Data.Xml.Dom.XmlDocument,Windows.Data.Xml.Dom,ContentType=WindowsRuntime]::new();\
             $x.LoadXml('<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>');\
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Selfkit Agenda').Show([Windows.UI.Notifications.ToastNotification]::new($x))",
            esc(title),
            esc(body)
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x0800_0000)
            .status()?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let st = std::process::Command::new("notify-send").args(["-a", "Agenda", "-u", "normal", title, body]).status()?;
        anyhow::ensure!(st.success(), "notify-send exited with {st}");
        Ok(())
    }
}

fn tick(st: &mut RemState, now: i64) -> Result<Vec<Due>> {
    let p = paths::app_db("agenda");
    if !p.exists() {
        st.last_check = now;
        return Ok(vec![]);
    }
    let after = st.last_check.max(now - LATE_MS);
    let store = Store::open_read_only(p, "agenda")?;
    let due: Vec<Due> = due_between(&store, after, now)?.into_iter().filter(|d| st.fired.get(&d.id) != Some(&d.at)).collect();
    st.last_check = now;
    st.fired.retain(|_, at| *at > now - 2 * 86_400_000);
    if due.is_empty() || agenda_running() {
        return Ok(vec![]);
    }
    for d in &due {
        st.fired.insert(d.id.clone(), d.at);
    }
    Ok(due)
}

pub async fn run(shared: Arc<Shared>) {
    let mut st = RemState::load();
    loop {
        if crate::config::Config::load().reminders.enabled {
            let now = time::now_ms();
            match tick(&mut st, now) {
                Ok(due) => {
                    for d in &due {
                        match notify(&d.title, &d.body) {
                            Ok(()) => tracing::info!(task = %d.id, "reminder shown"),
                            Err(e) => tracing::warn!("reminder notification failed: {e:#}"),
                        }
                    }
                    if !due.is_empty() {
                        let mut g = shared.inner.lock().unwrap();
                        g.reminders_fired += due.len() as u64;
                        g.reminders_last_fired_at = Some(now);
                    }
                    st.save();
                }
                Err(e) => tracing::debug!("reminders: {e:#}"),
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(TICK_SECS)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn local_ms(s: &str) -> i64 {
        let dt = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").unwrap();
        Local.from_local_datetime(&dt).earliest().unwrap().timestamp_millis()
    }

    #[test]
    fn fire_times() {
        assert_eq!(fire_at(&json!({"start": "2026-11-20T10:30", "reminder_min": 15})), Some(local_ms("2026-11-20T10:15")));
        assert_eq!(fire_at(&json!({"start": "2026-11-20", "reminder_min": 0})), Some(local_ms("2026-11-20T09:00")));
        assert_eq!(fire_at(&json!({"start": "2026-11-20T10:30"})), None);
        assert_eq!(fire_at(&json!({"start": "2026-11-20T10:30", "reminder_min": 5, "done_at": 1})), None);
        assert_eq!(fire_at(&json!({"start": "2026-11-20T10:30", "reminder_min": 5, "done_at": null})), Some(local_ms("2026-11-20T10:25")));
    }

    #[test]
    fn due_window() {
        let st = Store::open_in_memory("agenda", "d", &[]).unwrap();
        st.put("tasks", Some("t1"), &json!({"title": "Call", "start": "2026-11-20T10:30", "reminder_min": 10})).unwrap();
        st.put("tasks", Some("t2"), &json!({"title": "Gym", "start": "2026-11-20T18:00", "reminder_min": 0})).unwrap();
        st.put("tasks", Some("t3"), &json!({"title": "Gone", "start": "2026-11-20T10:25", "reminder_min": 0})).unwrap();
        st.delete("t3").unwrap();
        let at = local_ms("2026-11-20T10:20");
        let d = due_between(&st, at - 60_000, at + 1).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].title.as_str(), d[0].body.as_str()), ("Call", "Starts at 10:30"));
        assert!(due_between(&st, at, at + 60_000).unwrap().is_empty());
    }
}
