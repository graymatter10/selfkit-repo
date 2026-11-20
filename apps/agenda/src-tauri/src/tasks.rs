//! Agenda v2 data model on top of selfkit-core records.
//!
//! Collection `tasks`, JSON fields:
//! `title, notes, severity 1–5 (Low, Minor, Medium, High, Critical),
//!  start ("YYYY-MM-DDTHH:MM" local, or "YYYY-MM-DD" = all day), stop (same format),
//!  reminder_min (minutes before start; absent = off), repeat
//!  ("daily"|"weekdays"|"weekly"|"monthly"|"yearly"; repeat_day = day of month a
//!  monthly/yearly task returns to after a short month clamped it), list, steps [{id,title,done}],
//!  tags [lowercase words], stage ("doing" = Board column Doing; absent = To do),
//!  photos [photo record ids], done_at (UTC ms), created_at, last_done_at,
//!  last_completion (id of the `completions` record written when it was last completed)`.
//! Collection `completions` (history/stats): `task, title, done_at (UTC ms), tz_offset_min,
//!  due ("YYYY-MM-DDTHH:MM" local end it was due, absent if undated), on_time (bool, absent if undated)`.
//! v1 records (due_date/due_time/priority/subtasks) are converted on read and on write.
//! Grouping/sorting/status labels happen in the UI (local time); this module stores,
//! validates and computes reminder times. Docs: docs/apps/agenda/README.md.

use chrono::{Datelike, Duration, Months, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use selfkit_core::{Error, Result, Store, store::Migration, time};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const TASKS: &str = "tasks";
pub const LISTS: &str = "lists";
pub const COMPLETIONS: &str = "completions";
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
    if d.get("status").and_then(Value::as_str) == Some("doing") && !d.contains_key("stage") {
        d.insert("stage".into(), json!("doing"));
    }
    d.remove("status");
    d
}

/// Clean tag list: lowercase, no '#', no spaces, unique, max 32 chars each.
pub fn clean_tags(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(a) = v.as_array() {
        for t in a.iter().filter_map(Value::as_str) {
            let t: String = t.trim().trim_start_matches('#').to_lowercase().split_whitespace().collect::<Vec<_>>().join("-");
            let t: String = t.chars().take(32).collect();
            if !t.is_empty() && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
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
    match d.get("tags").map(clean_tags) {
        Some(t) if !t.is_empty() => {
            d.insert("tags".into(), json!(t));
        }
        _ => {
            d.remove("tags");
        }
    }
    if str_of(&d, "stage") != Some("doing") {
        d.remove("stage");
    }
    match d.get("photos") {
        Some(Value::Array(a)) if !a.is_empty() => {
            let ids: Vec<Value> = a.iter().filter(|v| v.is_string()).cloned().collect();
            d.insert("photos".into(), Value::Array(ids));
        }
        _ => {
            d.remove("photos");
        }
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
    pub tags: Option<Vec<String>>,
    pub stage: Option<String>,
    pub photos: Option<Vec<String>>,
}

pub fn create(store: &Store, t: NewTask) -> Result<TaskView> {
    let mut d = Map::new();
    d.insert("title".into(), json!(t.title));
    d.insert("severity".into(), json!(t.severity.unwrap_or(DEFAULT_SEVERITY)));
    d.insert("list".into(), json!(t.list.unwrap_or_else(|| INBOX.into())));
    d.insert("created_at".into(), json!(time::now_ms()));
    if let Some(v) = t.tags {
        d.insert("tags".into(), json!(v));
    }
    if let Some(v) = t.photos {
        d.insert("photos".into(), json!(v));
    }
    for (k, v) in [("notes", t.notes), ("start", t.start), ("stop", t.stop), ("repeat", t.repeat), ("stage", t.stage)] {
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

/// Validate and store a full task document under `id` (import).
pub fn put_raw(store: &Store, id: &str, d: Map<String, Value>) -> Result<()> {
    let d = validate(normalize(d))?;
    store.put(TASKS, Some(id), &Value::Object(d))?;
    Ok(())
}

/// Replace a task's whole data (undo). Undoing a completion also removes its
/// history entry (the completion id differs between the two versions).
pub fn replace(store: &Store, id: &str, data: &Value) -> Result<TaskView> {
    let d = normalize(data.as_object().cloned().unwrap_or_default());
    if let Some(cur) = store.get(id)?.filter(|r| !r.deleted)
        && let Some(c) = cur.data.get("last_completion").and_then(Value::as_str)
        && str_of(&d, "last_completion") != Some(c)
    {
        let _ = store.delete(c);
    }
    // Undoing a "reopen" brings its history entry back.
    if let Some(c) = str_of(&d, "last_completion")
        && store.get(c)?.is_some_and(|r| r.deleted)
    {
        let _ = store.restore(c);
    }
    Ok(view(&store.put(TASKS, Some(id), &Value::Object(d))?))
}

/// Local end the task is due by: stop, else timed start, else end of the all-day start.
pub fn due_at(d: &Map<String, Value>) -> Option<NaiveDateTime> {
    if let Some((stop, _)) = str_of(d, "stop").and_then(parse_when) {
        return Some(stop);
    }
    let (start, all_day) = str_of(d, "start").and_then(parse_when)?;
    Some(if all_day { start + Duration::days(1) } else { start })
}

/// Write a history entry for completing `d` now; returns its id.
fn log_completion(store: &Store, task_id: &str, d: &Map<String, Value>, now: NaiveDateTime) -> Result<String> {
    let mut c = Map::new();
    c.insert("task".into(), json!(task_id));
    c.insert("title".into(), d.get("title").cloned().unwrap_or(json!("")));
    c.insert("done_at".into(), json!(time::now_ms()));
    c.insert("tz_offset_min".into(), json!(time::local_offset_min()));
    if let Some(due) = due_at(d) {
        c.insert("due".into(), json!(due.format("%Y-%m-%dT%H:%M").to_string()));
        c.insert("on_time".into(), json!(now <= due));
    }
    if let Some(t) = d.get("tags") {
        c.insert("tags".into(), t.clone());
    }
    // One id per task occurrence (its start), so the same occurrence completed on two
    // devices before syncing is one history entry, not two (C30).
    let id = format!("c-{task_id}-{}", str_of(d, "start").unwrap_or("none"));
    Ok(store.put(COMPLETIONS, Some(&id), &Value::Object(c))?.id)
}

#[derive(Debug, Clone, Serialize)]
pub struct Completion {
    pub id: String,
    #[serde(flatten)]
    pub data: Map<String, Value>,
}

/// Completed-task history, newest first.
pub fn history(store: &Store) -> Result<Vec<Completion>> {
    let mut v: Vec<Completion> = store
        .list(COMPLETIONS, u32::MAX, 0)?
        .into_iter()
        .map(|r| Completion { id: r.id, data: r.data.as_object().cloned().unwrap_or_default() })
        .collect();
    v.sort_by_key(|c| std::cmp::Reverse(c.data.get("done_at").and_then(Value::as_i64).unwrap_or(0)));
    Ok(v)
}

/// `months` after `from`, on day `anchor` clamped to that month's length (Jan 31 →
/// Feb 28 → Mar 31, not Mar 28; Feb 29 yearly → Feb 28 → Feb 29 in leap years — C21).
fn add_months_on(from: NaiveDate, months: u32, anchor: u32) -> Option<NaiveDate> {
    let first = from.with_day(1)?.checked_add_months(Months::new(months))?;
    let last = (first.checked_add_months(Months::new(1))? - Duration::days(1)).day();
    first.with_day(anchor.clamp(1, last))
}

/// The day of month a monthly/yearly task repeats on: `repeat_day` (kept when a short
/// month clamped the start) while the start still matches it, else the start's day.
fn repeat_anchor(d: &Map<String, Value>, start: NaiveDate) -> u32 {
    let last = add_months_on(start, 0, 31).map_or(31, |x| x.day());
    match d.get("repeat_day").and_then(Value::as_u64).map(|x| x as u32) {
        Some(a) if (1..=31).contains(&a) && a.min(last) == start.day() => a,
        _ => start.day(),
    }
}

fn next_occurrence(repeat: &str, from: NaiveDate, anchor: u32) -> Option<NaiveDate> {
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
        "monthly" => add_months_on(from, 1, anchor)?,
        "yearly" => add_months_on(from, 12, anchor)?,
        _ => return None,
    })
}

/// Complete / un-complete. A repeating task with a start moves to its next
/// future occurrence (stop moves by the same amount; steps reset) instead.
pub fn toggle(store: &Store, id: &str, now: NaiveDateTime) -> Result<TaskView> {
    let today = now.date();
    let r = store.get(id)?.ok_or_else(|| Error::NotFound(id.into()))?;
    let mut d = normalize(r.data.as_object().cloned().unwrap_or_default());
    let repeat = str_of(&d, "repeat").map(str::to_owned);
    let start = str_of(&d, "start").and_then(parse_when);
    if is_done(&d) {
        d.remove("done_at");
        if let Some(c) = d.remove("last_completion").and_then(|v| v.as_str().map(str::to_owned)) {
            let _ = store.delete(&c);
        }
        return Ok(view(&store.put(TASKS, Some(id), &Value::Object(d))?));
    }
    let cid = log_completion(store, id, &d, now)?;
    d.insert("last_completion".into(), json!(cid));
    d.remove("stage");
    if let (Some(rep), Some((start, all_day))) = (repeat, start) {
        let anchor = repeat_anchor(&d, start.date());
        let mut next = start.date();
        while let Some(n) = next_occurrence(&rep, next, anchor) {
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
            if next.day() != anchor && matches!(rep.as_str(), "monthly" | "yearly") {
                d.insert("repeat_day".into(), json!(anchor));
            } else {
                d.remove("repeat_day");
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
    // The FTS index covers the whole JSON (keys, ids, "doing"…): keep only tasks whose
    // own text (title, notes, tags, step titles) has every word as a word prefix (C26).
    let toks = |s: &str| s.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).map(str::to_lowercase).collect::<Vec<_>>();
    let words = toks(q);
    Ok(store
        .search(Some(TASKS), &fts, 500)?
        .iter()
        .map(view)
        .filter(|t| {
            let mut text = String::new();
            for k in ["title", "notes"] {
                text.push_str(str_of(&t.data, k).unwrap_or(""));
                text.push(' ');
            }
            for v in t.data.get("tags").and_then(Value::as_array).into_iter().flatten() {
                text.push_str(v.as_str().unwrap_or(""));
                text.push(' ');
            }
            for v in t.data.get("steps").and_then(Value::as_array).into_iter().flatten() {
                text.push_str(v.get("title").and_then(Value::as_str).unwrap_or(""));
                text.push(' ');
            }
            let have = toks(&text);
            words.iter().all(|w| have.iter().any(|h| h.starts_with(w.as_str())))
        })
        .take(50)
        .collect())
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
        toggle(&s, &t.id, NaiveDate::from_ymd_opt(2026, 11, 15).unwrap().and_hms_opt(12, 0, 0).unwrap()).unwrap();
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
        let today = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap().and_hms_opt(12, 0, 0).unwrap(); // Saturday
        let t = create(&s, NewTask { start: Some("2026-11-13T09:00".into()), stop: Some("2026-11-13T09:30".into()), repeat: Some("weekdays".into()), ..nt("standup") }).unwrap();
        let t = toggle(&s, &t.id, today).unwrap();
        assert_eq!((t.data["start"].as_str(), t.data["stop"].as_str()), (Some("2026-11-16T09:00"), Some("2026-11-16T09:30")));
        assert!(!t.data.contains_key("done_at"));
        let m = create(&s, NewTask { start: Some("2026-11-30".into()), repeat: Some("monthly".into()), ..nt("rent") }).unwrap();
        assert_eq!(toggle(&s, &m.id, today).unwrap().data["start"], "2026-12-30");
        // Month-end anchor survives a short month (C21): Jan 31 → Feb 28 → Mar 31.
        let e = create(&s, NewTask { start: Some("2027-01-31".into()), repeat: Some("monthly".into()), ..nt("card bill") }).unwrap();
        let jan = NaiveDate::from_ymd_opt(2027, 1, 31).unwrap().and_hms_opt(12, 0, 0).unwrap();
        let e = toggle(&s, &e.id, jan).unwrap();
        assert_eq!((e.data["start"].as_str(), e.data["repeat_day"].as_u64()), (Some("2027-02-28"), Some(31)));
        let e = toggle(&s, &e.id, NaiveDate::from_ymd_opt(2027, 2, 28).unwrap().and_hms_opt(12, 0, 0).unwrap()).unwrap();
        assert_eq!((e.data["start"].as_str(), e.data.contains_key("repeat_day")), (Some("2027-03-31"), false));
        let y = create(&s, NewTask { start: Some("2028-02-29".into()), repeat: Some("yearly".into()), ..nt("leap") }).unwrap();
        let y = toggle(&s, &y.id, NaiveDate::from_ymd_opt(2028, 2, 29).unwrap().and_hms_opt(12, 0, 0).unwrap()).unwrap();
        assert_eq!(y.data["start"], "2029-02-28");
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
        assert_eq!(search(&s, "milk").unwrap().len(), 1);
        // Keys and ids are not task text (C26).
        assert_eq!(search(&s, "title").unwrap().len(), 0);
        assert_eq!(search(&s, "inbox").unwrap().len(), 0);
        assert_eq!(search(&s, "reminder").unwrap().len(), 0);
        assert_eq!(search(&s, "\"(").unwrap().len(), 0);
        assert_ne!(notification_id("a"), notification_id("b"));
        assert!(notification_id("x") >= 0);
    }

    #[test]
    fn tags_stage_photos_clean() {
        let s = store();
        let t = create(&s, NewTask { tags: Some(vec!["#Home".into(), "home".into(), " deep work ".into(), "".into()]), stage: Some("doing".into()), photos: Some(vec!["p1".into()]), ..nt("x") }).unwrap();
        assert_eq!(t.data["tags"], json!(["home", "deep-work"]));
        assert_eq!(t.data["stage"], "doing");
        assert_eq!(t.data["photos"], json!(["p1"]));
        let mut p = Map::new();
        p.insert("stage".into(), json!("todo"));
        p.insert("tags".into(), json!([]));
        let t = update(&s, &t.id, &p).unwrap();
        assert!(!t.data.contains_key("stage") && !t.data.contains_key("tags"));
        // v1 status "doing" becomes the Board's Doing column.
        let r = s.put(TASKS, None, &json!({"title": "old", "status": "doing"})).unwrap();
        assert_eq!(get(&s, &r.id).unwrap().unwrap().data["stage"], "doing");
    }

    #[test]
    fn completions_history_and_undo() {
        let s = store();
        let now = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap().and_hms_opt(12, 0, 0).unwrap();
        let late = create(&s, NewTask { start: Some("2026-11-14T09:00".into()), stage: Some("doing".into()), ..nt("late") }).unwrap();
        let ok = create(&s, NewTask { start: Some("2026-11-15".into()), ..nt("on time") }).unwrap();
        let undated = create(&s, nt("undated")).unwrap();
        let before = late.data.clone();
        let done = toggle(&s, &late.id, now).unwrap();
        assert!(!done.data.contains_key("stage"), "completing leaves the Doing column");
        toggle(&s, &ok.id, now).unwrap();
        toggle(&s, &undated.id, now).unwrap();
        let h = history(&s).unwrap();
        assert_eq!(h.len(), 3);
        let by = |title: &str| h.iter().find(|c| c.data["title"] == title).unwrap().data.clone();
        assert_eq!(by("late")["on_time"], false);
        assert_eq!(by("on time")["on_time"], true);
        assert!(!by("undated").contains_key("on_time"));
        // Undo (replace with the version before) removes the history entry.
        replace(&s, &late.id, &Value::Object(before)).unwrap();
        assert_eq!(history(&s).unwrap().len(), 2);
        // Un-completing removes it too; undoing that brings it back.
        let done_ok = get(&s, &ok.id).unwrap().unwrap().data;
        toggle(&s, &ok.id, now).unwrap();
        assert_eq!(history(&s).unwrap().len(), 1);
        replace(&s, &ok.id, &Value::Object(done_ok)).unwrap();
        assert_eq!(history(&s).unwrap().len(), 2);
        toggle(&s, &ok.id, now).unwrap();
        // Repeating tasks log every completion although they stay active.
        let r = create(&s, NewTask { start: Some("2026-11-15T08:00".into()), repeat: Some("daily".into()), ..nt("daily") }).unwrap();
        let r = toggle(&s, &r.id, now).unwrap();
        assert!(!r.data.contains_key("done_at"));
        assert_eq!(history(&s).unwrap().len(), 2);
    }
}
