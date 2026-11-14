//! Pomodoro ("Focus") sessions. Docs: docs/apps/agenda/focus.md.
//!
//! The running session lives in local kv `agenda.focus` (not synced):
//! `{task?, title, kind: "focus"|"break", start_ms, end_ms, planned_min}`.
//! Finished or stopped sessions are logged to the `focus` collection:
//! `task?, title, kind, start (UTC ms), end (UTC ms), minutes, planned_min,
//!  completed (ran to the end), tz_offset_min`.

use selfkit_core::{Result, Store, time};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const FOCUS: &str = "focus";
const KV: &str = "agenda.focus";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub task: Option<String>,
    pub title: String,
    pub kind: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub planned_min: i64,
}

pub fn current(store: &Store) -> Result<Option<Session>> {
    Ok(store.kv_get(KV)?.and_then(|s| serde_json::from_str(&s).ok()))
}

fn clear(store: &Store) -> Result<()> {
    store.kv_set(KV, "null")
}

fn log(store: &Store, s: &Session, end_ms: i64, completed: bool) -> Result<()> {
    let minutes = ((end_ms - s.start_ms) as f64 / 60_000.0).round() as i64;
    if minutes < 1 {
        return Ok(()); // a mis-tap, not a session
    }
    let mut d = Map::new();
    if let Some(t) = &s.task {
        d.insert("task".into(), json!(t));
    }
    d.insert("title".into(), json!(s.title));
    d.insert("kind".into(), json!(s.kind));
    d.insert("start".into(), json!(s.start_ms));
    d.insert("end".into(), json!(end_ms));
    d.insert("minutes".into(), json!(minutes));
    d.insert("planned_min".into(), json!(s.planned_min));
    d.insert("completed".into(), json!(completed));
    d.insert("tz_offset_min".into(), json!(time::local_offset_min()));
    store.put(FOCUS, None, &Value::Object(d))?;
    Ok(())
}

/// Start a session (a running one is stopped and logged first).
pub fn start(store: &Store, task: Option<String>, title: &str, kind: &str, minutes: i64, now_ms: i64) -> Result<Session> {
    stop(store, now_ms)?;
    let minutes = minutes.clamp(1, 240);
    let s = Session {
        task,
        title: title.trim().to_string(),
        kind: if kind == "break" { "break".into() } else { "focus".into() },
        start_ms: now_ms,
        end_ms: now_ms + minutes * 60_000,
        planned_min: minutes,
    };
    store.kv_set(KV, &serde_json::to_string(&s).unwrap_or_default())?;
    Ok(s)
}

/// Stop early; logged as not completed.
pub fn stop(store: &Store, now_ms: i64) -> Result<Option<Session>> {
    let Some(s) = current(store)? else { return Ok(None) };
    if now_ms >= s.end_ms {
        log(store, &s, s.end_ms, true)?;
    } else {
        log(store, &s, now_ms, false)?;
    }
    clear(store)?;
    Ok(Some(s))
}

/// If the running session has reached its end, log it as completed and return it.
pub fn finish_if_due(store: &Store, now_ms: i64) -> Result<Option<Session>> {
    match current(store)? {
        Some(s) if now_ms >= s.end_ms => {
            log(store, &s, s.end_ms, true)?;
            clear(store)?;
            Ok(Some(s))
        }
        _ => Ok(None),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Logged {
    pub id: String,
    #[serde(flatten)]
    pub data: Map<String, Value>,
}

/// Logged sessions, newest first.
pub fn sessions(store: &Store) -> Result<Vec<Logged>> {
    let mut v: Vec<Logged> =
        store.list(FOCUS, u32::MAX, 0)?.into_iter().map(|r| Logged { id: r.id, data: r.data.as_object().cloned().unwrap_or_default() }).collect();
    v.sort_by_key(|l| std::cmp::Reverse(l.data.get("start").and_then(Value::as_i64).unwrap_or(0)));
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_lifecycle() {
        let s = Store::open_in_memory("agenda", "dev", crate::tasks::MIGRATIONS).unwrap();
        let t0 = 1_800_000_000_000;
        let a = start(&s, Some("t1".into()), "Write", "focus", 25, t0).unwrap();
        assert_eq!(a.end_ms - a.start_ms, 25 * 60_000);
        assert_eq!(finish_if_due(&s, t0 + 60_000).unwrap(), None);
        assert!(finish_if_due(&s, a.end_ms).unwrap().is_some());
        assert_eq!(current(&s).unwrap(), None);
        // Stopping early logs a partial session; a mis-tap (< 1 min) is not logged.
        start(&s, None, "Break", "break", 5, t0 + 10_000_000).unwrap();
        stop(&s, t0 + 10_000_000 + 120_000).unwrap();
        start(&s, None, "x", "focus", 25, t0 + 20_000_000).unwrap();
        stop(&s, t0 + 20_000_000 + 5_000).unwrap();
        let v = sessions(&s).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].data["kind"].as_str(), v[0].data["minutes"].as_i64(), v[0].data["completed"].as_bool()), (Some("break"), Some(2), Some(false)));
        assert_eq!((v[1].data["minutes"].as_i64(), v[1].data["completed"].as_bool()), (Some(25), Some(true)));
    }
}
