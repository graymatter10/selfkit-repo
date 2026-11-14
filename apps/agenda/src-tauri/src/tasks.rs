//! Agenda v2 data model on top of selfkit-core records.
//!
//! Collection `tasks`, JSON fields:
//! `title, notes, severity 1–5 (Low, Minor, Medium, High, Critical),
//!  start ("YYYY-MM-DDTHH:MM" local, or "YYYY-MM-DD" = all day), stop (same format),
//!  reminder_min (minutes before start; absent = off), repeat
//!  ("daily"|"weekdays"|"weekly"|"monthly"|"yearly"), list, steps [{id,title,done}],
//!  photos [ids], done_at (UTC ms), created_at, last_done_at`.
//! v1 records (due_date/due_time/priority/subtasks) are converted on read and on write.
//! Grouping/sorting/status labels happen in the UI (local time); this module stores,
//! validates and computes reminder times. Docs: docs/apps/agenda/README.md.

use chrono::{Datelike, Duration, Months, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use selfkit_core::{Error, Result, Store, store::Migration, time};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const TASKS: &str = "tasks";
pub const LISTS: &str = "lists";
pub const INBOX: &str = "inbox";
pub const DEFAULT_SEVERITY: u8 = 3;

pub const MIGRATIONS: &[Migration] = &[Migration {
    name: "agenda/001_indexes",
    sql: "CREATE INDEX IF NOT EXISTS agenda_due ON records(json_extract(data, '$.due_date')) WHERE collection = 'tasks';",
}];

#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    pub id: String,
    #[serde(flatten)]
    pub data: Map<String, Value>,
    pub updated_at: i64,
}

/// Convert v1 fields to v2 (idempotent).
pub fn normalize(mut d: Map<String, Value>) -> Map<String, Value> {
    if !d.contains_key("start")
        && let Some(date) = d.get("due_date").and_then(Value::as_str).map(str::to_owned)
    {
        let start = match d.get("due_time").and_then(Value::as_str) {
            Some(t) => format!("{date}T{t}"),
            None => date,
        };
        d.insert("start".into(), json!(start));
    }
    d.remove("due_date");
    d.remove("due_time");
    if !d.contains_key("severity") {
        let sev = match d.get("priority").and_then(Value::as_i64) {
            Some(1) => 5,
            Some(2) => 4,
            _ => DEFAULT_SEVERITY as i64,
        };
        d.insert("severity".into(), json!(sev));
    }
    d.remove("priority");
    if let Some(subs) = d.remove("subtasks")
        && !d.contains_key("steps")
    {
        d.insert("steps".into(), subs);
    }
    d.remove("status");
    d
}

fn view(r: &selfkit_core::Record) -> TaskView {
    TaskView { id: r.id.clone(), data: normalize(r.data.as_object().cloned().unwrap_or_default()), updated_at: r.updated_at }
}

/// "2026-11-15T10:30" → datetime; "2026-11-15" → (date, all-day).
pub fn parse_when(s: &str) -> Option<(NaiveDateTime, bool)> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
        return Some((dt, false));
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().map(|d| (d.and_time(NaiveTime::MIN), true))
}

fn fmt_when(dt: NaiveDateTime, all_day: bool) -> String {
    if all_day { dt.format("%Y-%m-%d").to_string() } else { dt.format("%Y-%m-%dT%H:%M").to_string() }
}

fn str_of<'a>(d: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    d.get(k).and_then(Value::as_str)
}

fn is_done(d: &Map<String, Value>) -> bool {
    d.get("done_at").is_some_and(|v| !v.is_null())
}

/// Validate and clean a task document before saving.
fn validate(mut d: Map<String, Value>) -> Result<Map<String, Value>> {
    let title = str_of(&d, "title").unwrap_or("").trim().to_string();
    if title.is_empty() {
        return Err(Error::Format("A task needs a title.".into()));
    }
    d.insert("title".into(), json!(title));
    let sev = d.get("severity").and_then(Value::as_i64).unwrap_or(DEFAULT_SEVERITY as i64).clamp(1, 5);
    d.insert("severity".into(), json!(sev));
    for k in ["start", "stop"] {
        match d.get(k) {
            Some(Value::String(s)) if s.is_empty() => {
                d.remove(k);
            }
            Some(Value::String(s)) if parse_when(s).is_none() => return Err(Error::Format(format!("Bad {k} time: {s}"))),
            Some(Value::Null) => {
                d.remove(k);
            }
            _ => {}
        }
    }
    if d.contains_key("stop") && !d.contains_key("start") {
        return Err(Error::Format("Set a start before a stop.".into()));
    }
    if let (Some((a, _)), Some((b, _))) = (str_of(&d, "start").and_then(parse_when), str_of(&d, "stop").and_then(parse_when))
        && b < a
    {
        return Err(Error::Format("Stop must be after start.".into()));
    }
    if str_of(&d, "notes").is_some_and(|n| n.trim().is_empty()) {
        d.remove("notes");
    }
    if matches!(d.get("reminder_min"), Some(Value::Null)) {
        d.remove("reminder_min");
    }
    if matches!(d.get("repeat"), Some(Value::Null)) || str_of(&d, "repeat") == Some("") {
        d.remove("repeat");
    }
    Ok(d)
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    Active,
    Done,
    All,
}

pub fn list(store: &Store, filter: Filter) -> Result<Vec<TaskView>> {
    Ok(store
        .list(TASKS, u32::MAX, 0)?
        .iter()
        .map(view)
        .filter(|t| match filter {
            Filter::All => true,
            Filter::Active => !is_done(&t.data),
            Filter::Done => is_done(&t.data),
        })
        .collect())
}

#[derive(Debug, Deserialize, Default)]
pub struct NewTask {
    pub title: String,
    pub notes: Option<String>,
    pub severity: Option<u8>,
    pub start: Option<String>,
    pub stop: Option<String>,
    pub reminder_min: Option<i64>,
    pub repeat: Option<String>,
    pub list: Option<String>,
    pub steps: Option<Vec<Value>>,
}

pub fn create(store: &Store, t: NewTask) -> Result<TaskView> {
    let mut d = Map::new();
    d.insert("title".into(), json!(t.title));
    d.insert("severity".into(), json!(t.severity.unwrap_or(DEFAULT_SEVERITY)));
    d.insert("list".into(), json!(t.list.unwrap_or_else(|| INBOX.into())));
    d.insert("created_at".into(), json!(time::now_ms()));
    for (k, v) in [("notes", t.notes), ("start", t.start), ("stop", t.stop), ("repeat", t.repeat)] {
        if let Some(v) = v {
            d.insert(k.into(), json!(v));
        }
    }
    if let Some(r) = t.reminder_min {
        d.insert("reminder_min".into(), json!(r));
    }
    if let Some(s) = t.steps {
        d.insert("steps".into(), Value::Array(s));
    }
    let d = validate(d)?;
    Ok(view(&store.put(TASKS, None, &Value::Object(d))?))
}

/// Merge `patch` (null removes a field), validate, save.
pub fn update(store: &Store, id: &str, patch: &Map<String, Value>) -> Result<TaskView> {
    let r = store.get(id)?.filter(|r| !r.deleted).ok_or_else(|| Error::NotFound(id.into()))?;
    let mut d = normalize(r.data.as_object().cloned().unwrap_or_default());
    for (k, v) in patch {
        if v.is_null() {
            d.remove(k);
        } else {
            d.insert(k.clone(), v.clone());
        }
    }
    let d = validate(d)?;
    Ok(view(&store.put(TASKS, Some(id), &Value::Object(d))?))
}

/// Replace a task's whole data (undo).
pub fn replace(store: &Store, id: &str, data: &Value) -> Result<TaskView> {
    let d = normalize(data.as_object().cloned().unwrap_or_default());
    Ok(view(&store.put(TASKS, Some(id), &Value::Object(d))?))
}

fn next_occurrence(repeat: &str, from: NaiveDate) -> Option<NaiveDate> {
    Some(match repeat {
        "daily" => from + Duration::days(1),
        "weekly" => from + Duration::days(7),
        "weekdays" => {
            let mut d = from + Duration::days(1);
            while matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
                d += Duration::days(1);
            }
            d
        }
        "monthly" => from.checked_add_months(Months::new(1))?,
        "yearly" => from.checked_add_months(Months::new(12))?,
        _ => return None,
    })
}

/// Complete / un-complete. A repeating task with a start moves to its next
/// future occurrence (stop moves by the same amount; steps reset) instead.
pub fn toggle(store: &Store, id: &str, today: NaiveDate) -> Result<TaskView> {
    let r = store.get(id)?.ok_or_else(|| Error::NotFound(id.into()))?;
    let mut d = normalize(r.data.as_object().cloned().unwrap_or_default());
    let repeat = str_of(&d, "repeat").map(str::to_owned);
    let start = str_of(&d, "start").and_then(parse_when);
    if is_done(&d) {
        d.remove("done_at");
    } else if let (Some(rep), Some((start, all_day))) = (repeat, start) {
        let mut next = start.date();
        while let Some(n) = next_occurrence(&rep, next) {
            next = n;
            if next > today {
                break;
            }
        }
        if next == start.date() {
            d.insert("done_at".into(), json!(time::now_ms()));
        } else {
            let shift = next - start.date();
            d.insert("start".into(), json!(fmt_when(start + shift, all_day)));
            if let Some((stop, sad)) = str_of(&d, "stop").and_then(parse_when) {
                d.insert("stop".into(), json!(fmt_when(stop + shift, sad)));
            }
            d.insert("last_done_at".into(), json!(time::now_ms()));
            if let Some(Value::Array(steps)) = d.get_mut("steps") {
                for s in steps {
                    s["done"] = json!(false);
                }
            }
        }
    } else {
        d.insert("done_at".into(), json!(time::now_ms()));
    }
    Ok(view(&store.put(TASKS, Some(id), &Value::Object(d))?))
}

pub fn get(store: &Store, id: &str) -> Result<Option<TaskView>> {
    Ok(store.get(id)?.filter(|r| !r.deleted).map(|r| view(&r)))
}

pub fn search(store: &Store, q: &str) -> Result<Vec<TaskView>> {
    let fts: String = q.split_whitespace().map(|w| format!("\"{}\"*", w.replace('"', ""))).collect::<Vec<_>>().join(" ");
    if fts.is_empty() {
        return Ok(vec![]);
    }
    Ok(store.search(Some(TASKS), &fts, 50)?.iter().map(view).collect())
}

#[derive(Debug, Serialize)]
pub struct ListInfo {
    pub id: String,
    pub name: String,
}

pub fn lists(store: &Store) -> Result<Vec<ListInfo>> {
    let mut out = vec![ListInfo { id: INBOX.into(), name: "Personal".into() }];
    let mut user = store.list(LISTS, 1000, 0)?;
    user.sort_by(|a, b| a.id.cmp(&b.id));
    out.extend(user.into_iter().map(|r| ListInfo { name: r.data["name"].as_str().unwrap_or("List").into(), id: r.id }));
    Ok(out)
}

pub fn add_list(store: &Store, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Format("A list needs a name.".into()));
    }
    Ok(store.put(LISTS, None, &json!({ "name": name }))?.id)
}

/// Local time when the reminder should fire (None: off, done, or no start).
/// All-day tasks remind relative to 09:00.
pub fn reminder_at(t: &TaskView) -> Option<NaiveDateTime> {
    if is_done(&t.data) {
        return None;
    }
    let mins = t.data.get("reminder_min").and_then(Value::as_i64)?;
    let (start, all_day) = str_of(&t.data, "start").and_then(parse_when)?;
    let base = if all_day { start.date().and_hms_opt(9, 0, 0)? } else { start };
    Some(base - Duration::minutes(mins))
}

/// Stable positive notification id for a task (FNV-1a).
pub fn notification_id(task_id: &str) -> i32 {
    let mut h: u32 = 2166136261;
    for b in task_id.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    (h & 0x7fff_ffff) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory("agenda", "dev", MIGRATIONS).unwrap()
    }
    fn nt(title: &str) -> NewTask {
        NewTask { title: title.into(), ..Default::default() }
    }

    #[test]
    fn create_validates() {
        let s = store();
        assert!(create(&s, nt("  ")).is_err());
        let t = create(&s, NewTask { severity: Some(9), start: Some("2026-11-15T10:30".into()), reminder_min: Some(30), ..nt("Call") }).unwrap();
        assert_eq!(t.data["severity"], 5);
        assert_eq!(t.data["list"], INBOX);
        assert!(create(&s, NewTask { start: Some("2026-11-15T10:30".into()), stop: Some("2026-11-15T09:00".into()), ..nt("x") }).is_err());
        assert!(create(&s, NewTask { stop: Some("2026-11-15T09:00".into()), ..nt("x") }).is_err());
        assert!(create(&s, NewTask { start: Some("tomorrow".into()), ..nt("x") }).is_err());
        assert_eq!(reminder_at(&t).unwrap().format("%H:%M").to_string(), "10:00");
    }

    #[test]
    fn every_task_stays_listed() {
        // v1 bug: tasks with other dates or no date vanished from view.
        let s = store();
        create(&s, nt("no date")).unwrap();
        create(&s, NewTask { start: Some("2027-01-05".into()), ..nt("far away") }).unwrap();
        let t = create(&s, NewTask { start: Some("2026-11-15T10:00".into()), ..nt("today") }).unwrap();
        assert_eq!(list(&s, Filter::Active).unwrap().len(), 3);
        let mut p = Map::new();
        p.insert("start".into(), json!("2026-12-25T08:00"));
        update(&s, &t.id, &p).unwrap();
        assert_eq!(list(&s, Filter::Active).unwrap().len(), 3, "editing the date keeps it listed");
        toggle(&s, &t.id, NaiveDate::from_ymd_opt(2026, 11, 15).unwrap()).unwrap();
        assert_eq!((list(&s, Filter::Active).unwrap().len(), list(&s, Filter::Done).unwrap().len()), (2, 1));
        assert_eq!(list(&s, Filter::All).unwrap().len(), 3);
    }

    #[test]
    fn v1_records_convert() {
        let s = store();
        let r = s
            .put(TASKS, None, &json!({"title": "old", "due_date": "2026-11-14", "due_time": "17:00", "priority": 1, "subtasks": [{"id":"a","title":"x","done":false}], "status": "doing"}))
            .unwrap();
        let t = get(&s, &r.id).unwrap().unwrap();
        assert_eq!(t.data["start"], "2026-11-14T17:00");
        assert_eq!(t.data["severity"], 5);
        assert!(t.data["steps"].is_array() && !t.data.contains_key("due_date") && !t.data.contains_key("status"));
        let r2 = s.put(TASKS, None, &json!({"title": "all day", "due_date": "2026-11-14"})).unwrap();
        assert_eq!(get(&s, &r2.id).unwrap().unwrap().data["start"], "2026-11-14");
    }

    #[test]
    fn repeat_moves_start_and_stop() {
        let s = store();
        let today = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap(); // Saturday
        let t = create(&s, NewTask { start: Some("2026-11-13T09:00".into()), stop: Some("2026-11-13T09:30".into()), repeat: Some("weekdays".into()), ..nt("standup") }).unwrap();
        let t = toggle(&s, &t.id, today).unwrap();
        assert_eq!((t.data["start"].as_str(), t.data["stop"].as_str()), (Some("2026-11-16T09:00"), Some("2026-11-16T09:30")));
        assert!(!t.data.contains_key("done_at"));
        let m = create(&s, NewTask { start: Some("2026-11-30".into()), repeat: Some("monthly".into()), ..nt("rent") }).unwrap();
        assert_eq!(toggle(&s, &m.id, today).unwrap().data["start"], "2026-12-30");
        let once = create(&s, nt("once")).unwrap();
        assert!(toggle(&s, &once.id, today).unwrap().data.contains_key("done_at"));
    }

    #[test]
    fn patch_null_removes_and_search() {
        let s = store();
        let t = create(&s, NewTask { notes: Some("buy **milk**".into()), reminder_min: Some(10), ..nt("Groceries") }).unwrap();
        let mut p = Map::new();
        p.insert("reminder_min".into(), Value::Null);
        assert!(!update(&s, &t.id, &p).unwrap().data.contains_key("reminder_min"));
        assert_eq!(search(&s, "groc").unwrap().len(), 1);
        assert_eq!(search(&s, "\"(").unwrap().len(), 0);
        assert_ne!(notification_id("a"), notification_id("b"));
        assert!(notification_id("x") >= 0);
    }
}
