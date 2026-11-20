//! `.ics` (iCalendar, RFC 5545) export and import of tasks as VTODO.
//! Docs: docs/apps/agenda/ics.md (field mapping).
//!
//! Mapping (Agenda → VTODO):
//! - id → UID; title → SUMMARY; notes → DESCRIPTION; tags → CATEGORIES.
//! - start + stop → DTSTART + DUE; start only → DUE (what other task apps show as
//!   "due"), except repeating tasks, which need DTSTART for their RRULE.
//!   All-day values use VALUE=DATE; times are floating local time.
//! - repeat → RRULE (FREQ=DAILY | WEEKLY;BYDAY=MO..FR | WEEKLY | MONTHLY | YEARLY).
//! - severity 1–5 → PRIORITY 9/7/5/3/1 (+ X-AGENDA-SEVERITY for an exact round trip).
//! - done → STATUS:COMPLETED + COMPLETED + PERCENT-COMPLETE:100; stage doing → IN-PROCESS.
//! - reminder → VALARM (DISPLAY) with a relative TRIGGER (all-day tasks remind at 09:00).
//! - list → X-AGENDA-LIST (name); steps → X-AGENDA-STEPS (JSON). Photos are not exported.

use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use selfkit_core::{Result, Store, time};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::tasks::{self, ListInfo, TaskView, parse_when};

// ---------------------------------------------------------------- writing

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace("\r\n", "\\n").replace('\n', "\\n")
}

/// Fold a content line at 75 octets without splitting UTF-8 characters.
fn fold(line: &str, out: &mut String) {
    let mut n = 0;
    for ch in line.chars() {
        let w = ch.len_utf8();
        if n + w > 75 {
            out.push_str("\r\n ");
            n = 1;
        }
        out.push(ch);
        n += w;
    }
    out.push_str("\r\n");
}

fn utc_stamp(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_else(Utc::now).format("%Y%m%dT%H%M%SZ").to_string()
}

fn when_prop(name: &str, v: &str) -> Option<String> {
    let (dt, all_day) = parse_when(v)?;
    Some(if all_day { format!("{name};VALUE=DATE:{}", dt.format("%Y%m%d")) } else { format!("{name}:{}", dt.format("%Y%m%dT%H%M%S")) })
}

fn fmt_dur(min: i64) -> String {
    let (sign, m) = if min < 0 { ("-", -min) } else { ("", min) };
    if m == 0 {
        return "PT0M".into();
    }
    if m % 1440 == 0 {
        return format!("{sign}P{}D", m / 1440);
    }
    let (d, h, mm) = (m / 1440, (m % 1440) / 60, m % 60);
    let mut s = format!("{sign}P");
    if d > 0 {
        s += &format!("{d}D");
    }
    s.push('T');
    if h > 0 {
        s += &format!("{h}H");
    }
    if mm > 0 {
        s += &format!("{mm}M");
    }
    s
}

fn str_of<'a>(d: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    d.get(k).and_then(Value::as_str)
}

/// Base time the reminder counts back from: the start, or 09:00 for all-day.
fn reminder_base(start: &str) -> Option<NaiveDateTime> {
    let (dt, all_day) = parse_when(start)?;
    if all_day { dt.date().and_hms_opt(9, 0, 0) } else { Some(dt) }
}

fn vtodo(t: &TaskView, lists: &[ListInfo], out: &mut String) {
    let d = &t.data;
    let mut lines: Vec<String> = vec!["BEGIN:VTODO".into(), format!("UID:{}", t.id), format!("DTSTAMP:{}", utc_stamp(time::now_ms()))];
    if let Some(c) = d.get("created_at").and_then(Value::as_i64) {
        lines.push(format!("CREATED:{}", utc_stamp(c)));
    }
    lines.push(format!("LAST-MODIFIED:{}", utc_stamp(t.updated_at)));
    lines.push(format!("SUMMARY:{}", esc(str_of(d, "title").unwrap_or(""))));
    if let Some(n) = str_of(d, "notes") {
        lines.push(format!("DESCRIPTION:{}", esc(n)));
    }
    let start = str_of(d, "start");
    let stop = str_of(d, "stop");
    let repeat = str_of(d, "repeat");
    let has_dtstart = start.is_some() && (stop.is_some() || repeat.is_some());
    // The property the alarm is anchored to.
    let mut anchor: Option<(&str, &str)> = None;
    if let Some(s) = start {
        if has_dtstart {
            lines.extend(when_prop("DTSTART", s));
            anchor = Some(("START", s));
            if let Some(e) = stop {
                lines.extend(when_prop("DUE", e));
            }
        } else {
            lines.extend(when_prop("DUE", s));
            anchor = Some(("END", s));
        }
    }
    if let Some(r) = repeat {
        let rule = match r {
            "daily" => "FREQ=DAILY",
            "weekdays" => "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
            "weekly" => "FREQ=WEEKLY",
            "monthly" => "FREQ=MONTHLY",
            "yearly" => "FREQ=YEARLY",
            _ => "",
        };
        if !rule.is_empty() {
            lines.push(format!("RRULE:{rule}"));
        }
    }
    let sev = d.get("severity").and_then(Value::as_i64).unwrap_or(3).clamp(1, 5);
    lines.push(format!("PRIORITY:{}", 11 - 2 * sev));
    lines.push(format!("X-AGENDA-SEVERITY:{sev}"));
    if let Some(done) = d.get("done_at").and_then(Value::as_i64) {
        lines.push("STATUS:COMPLETED".into());
        lines.push(format!("COMPLETED:{}", utc_stamp(done)));
        lines.push("PERCENT-COMPLETE:100".into());
    } else if str_of(d, "stage") == Some("doing") {
        lines.push("STATUS:IN-PROCESS".into());
    } else {
        lines.push("STATUS:NEEDS-ACTION".into());
    }
    if let Some(tags) = d.get("tags").and_then(Value::as_array).filter(|a| !a.is_empty()) {
        let v: Vec<String> = tags.iter().filter_map(Value::as_str).map(esc).collect();
        lines.push(format!("CATEGORIES:{}", v.join(",")));
    }
    if let Some(l) = str_of(d, "list").filter(|l| *l != tasks::INBOX)
        && let Some(info) = lists.iter().find(|i| i.id == l)
    {
        lines.push(format!("X-AGENDA-LIST:{}", esc(&info.name)));
    }
    if let Some(steps) = d.get("steps").and_then(Value::as_array).filter(|a| !a.is_empty()) {
        lines.push(format!("X-AGENDA-STEPS:{}", esc(&Value::Array(steps.clone()).to_string())));
    }
    if let (Some(m), Some((rel, av))) = (d.get("reminder_min").and_then(Value::as_i64), anchor)
        && let (Some(base), Some((anchor_dt, _))) = (start.and_then(reminder_base), parse_when(av))
    {
        let fire = base - chrono::Duration::minutes(m);
        let offset = (fire - anchor_dt).num_minutes();
        lines.push("BEGIN:VALARM".into());
        lines.push("ACTION:DISPLAY".into());
        lines.push(format!("DESCRIPTION:{}", esc(str_of(d, "title").unwrap_or("Reminder"))));
        lines.push(if rel == "END" { format!("TRIGGER;RELATED=END:{}", fmt_dur(offset)) } else { format!("TRIGGER:{}", fmt_dur(offset)) });
        lines.push("END:VALARM".into());
    }
    lines.push("END:VTODO".into());
    for l in lines {
        fold(&l, out);
    }
}

/// A whole VCALENDAR with one VTODO per task.
/// `version` = the app's package version (tauri.conf.json), so PRODID never goes stale (L11).
pub fn export(list: &[TaskView], lists: &[ListInfo], version: &str) -> String {
    let mut out = String::new();
    let prodid = format!("PRODID:-//Selfkit//Agenda {version}//EN");
    for l in ["BEGIN:VCALENDAR", "VERSION:2.0", prodid.as_str(), "CALSCALE:GREGORIAN", "X-WR-CALNAME:Agenda"] {
        fold(l, &mut out);
    }
    for t in list {
        vtodo(t, lists, &mut out);
    }
    fold("END:VCALENDAR", &mut out);
    out
}

// ---------------------------------------------------------------- reading

#[derive(Debug, Clone)]
pub struct Prop {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub value: String,
}

impl Prop {
    fn param(&self, k: &str) -> Option<&str> {
        self.params.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Component {
    pub name: String,
    pub props: Vec<Prop>,
    pub children: Vec<Component>,
}

impl Component {
    fn get(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|p| p.name == name)
    }
    fn text(&self, name: &str) -> Option<String> {
        self.get(name).map(|p| unesc(&p.value))
    }
}

fn unesc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Split on unescaped commas (CATEGORIES).
fn split_list(s: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut it = s.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                if let Some(n) = it.next() {
                    out.last_mut().unwrap().push(if n == 'n' || n == 'N' { '\n' } else { n });
                }
            }
            ',' => out.push(String::new()),
            _ => out.last_mut().unwrap().push(c),
        }
    }
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn parse_line(line: &str) -> Option<Prop> {
    // name *(";" param) ":" value — a ':' inside a quoted param value doesn't end the params.
    let mut in_q = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_q = !in_q,
            ':' if !in_q => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_uppercase();
    let params = parts
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.trim().to_uppercase(), v.trim_matches('"').to_string()))
        .collect();
    Some(Prop { name, params, value: value.to_string() })
}

/// Parse iCalendar text into top-level components (usually one VCALENDAR).
pub fn parse(text: &str) -> Vec<Component> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&raw[1..]);
        } else if !raw.is_empty() {
            lines.push(raw.to_string());
        }
    }
    let mut stack: Vec<Component> = Vec::new();
    let mut top = Vec::new();
    for l in &lines {
        let Some(p) = parse_line(l) else { continue };
        match p.name.as_str() {
            "BEGIN" => stack.push(Component { name: p.value.trim().to_uppercase(), ..Default::default() }),
            "END" => {
                if let Some(c) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(c),
                        None => top.push(c),
                    }
                }
            }
            _ => {
                if let Some(c) = stack.last_mut() {
                    c.props.push(p);
                }
            }
        }
    }
    top
}

/// Date-time property → (local datetime, all-day). UTC values are converted to
/// local time; TZID values are read as local time (one-person app, one zone).
fn read_when(p: &Prop) -> Option<(NaiveDateTime, bool)> {
    let v = p.value.trim();
    if p.param("VALUE") == Some("DATE") || v.len() == 8 {
        return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(|d| (d.and_hms_opt(0, 0, 0).unwrap(), true));
    }
    if let Some(u) = v.strip_suffix('Z') {
        let dt = NaiveDateTime::parse_from_str(u, "%Y%m%dT%H%M%S").ok()?;
        return Some((Utc.from_utc_datetime(&dt).with_timezone(&Local).naive_local(), false));
    }
    NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok().map(|d| (d, false))
}

fn read_utc_ms(p: &Prop) -> Option<i64> {
    let (dt, _) = read_when(p)?;
    Local.from_local_datetime(&dt).earliest().map(|d| d.timestamp_millis())
}

fn fmt_local(dt: NaiveDateTime, all_day: bool) -> String {
    if all_day { dt.format("%Y-%m-%d").to_string() } else { dt.format("%Y-%m-%dT%H:%M").to_string() }
}

/// "-PT1H30M" → -90 (minutes).
pub fn parse_dur(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, s) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let s = s.strip_prefix('P')?;
    let (mut total, mut num, mut in_time) = (0i64, String::new(), false);
    for c in s.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            _ => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += match (c, in_time) {
                    ('W', _) => n * 7 * 1440,
                    ('D', _) => n * 1440,
                    ('H', true) => n * 60,
                    ('M', true) => n,
                    ('S', true) => n / 60,
                    _ => return None,
                };
            }
        }
    }
    Some(if neg { -total } else { total })
}

/// Task id for a UID: the UID itself when it is a safe id, else a stable hash.
pub fn id_for_uid(uid: &str) -> String {
    let ok = !uid.is_empty() && uid.len() <= 128 && uid.chars().all(|c| c.is_ascii_alphanumeric() || "-_.@".contains(c));
    if ok {
        return uid.to_string();
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for b in uid.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    format!("ics-{h:016x}")
}

/// Fields a VTODO describes (replaced on import; others such as photos are kept).
const MANAGED: [&str; 12] = ["title", "notes", "start", "stop", "repeat", "severity", "done_at", "stage", "tags", "list", "steps", "reminder_min"];

/// One VTODO → (task id, task fields). `list_name` is X-AGENDA-LIST.
pub fn task_from_vtodo(c: &Component) -> (String, Map<String, Value>, Option<String>) {
    let mut d = Map::new();
    let uid = c.text("UID").unwrap_or_default();
    let id = if uid.is_empty() { selfkit_core::store::new_id() } else { id_for_uid(&uid) };
    let title = c.text("SUMMARY").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Untitled".into());
    d.insert("title".into(), json!(title));
    if let Some(n) = c.text("DESCRIPTION").filter(|s| !s.trim().is_empty()) {
        d.insert("notes".into(), json!(n));
    }
    let dtstart = c.get("DTSTART").and_then(read_when);
    let due = c.get("DUE").and_then(read_when);
    let (start, stop) = match (dtstart, due) {
        (Some(s), Some(e)) => (Some(s), if e.0 > s.0 && !s.1 { Some(e) } else { None }),
        (Some(s), None) => (Some(s), None),
        (None, Some(e)) => (Some(e), None),
        (None, None) => (None, None),
    };
    if let Some((s, ad)) = start {
        d.insert("start".into(), json!(fmt_local(s, ad)));
    }
    if let Some((e, _)) = stop {
        d.insert("stop".into(), json!(fmt_local(e, false)));
    }
    if let Some(r) = c.get("RRULE") {
        let up = r.value.to_uppercase();
        let freq = up.split(';').find_map(|kv| kv.strip_prefix("FREQ=")).unwrap_or("");
        let byday = up.split(';').find_map(|kv| kv.strip_prefix("BYDAY=")).unwrap_or("");
        let rep = match freq {
            "DAILY" => "daily",
            "WEEKLY" if byday == "MO,TU,WE,TH,FR" => "weekdays",
            "WEEKLY" => "weekly",
            "MONTHLY" => "monthly",
            "YEARLY" => "yearly",
            _ => "",
        };
        if !rep.is_empty() {
            d.insert("repeat".into(), json!(rep));
        }
    }
    let sev = c
        .text("X-AGENDA-SEVERITY")
        .and_then(|s| s.trim().parse::<i64>().ok())
        .or_else(|| {
            c.text("PRIORITY").and_then(|p| p.trim().parse::<i64>().ok()).map(|p| match p {
                1 => 5,
                2..=3 => 4,
                4..=6 => 3,
                7..=8 => 2,
                9 => 1,
                _ => 3,
            })
        })
        .unwrap_or(3)
        .clamp(1, 5);
    d.insert("severity".into(), json!(sev));
    let status = c.text("STATUS").unwrap_or_default().to_uppercase();
    let completed = c.get("COMPLETED").and_then(read_utc_ms);
    if status == "COMPLETED" || completed.is_some() {
        let at = completed.or_else(|| c.get("LAST-MODIFIED").and_then(read_utc_ms)).unwrap_or_else(time::now_ms);
        d.insert("done_at".into(), json!(at));
    } else if status == "IN-PROCESS" {
        d.insert("stage".into(), json!("doing"));
    }
    let tags: Vec<Value> = c.props.iter().filter(|p| p.name == "CATEGORIES").flat_map(|p| split_list(&p.value)).map(Value::from).collect();
    if !tags.is_empty() {
        d.insert("tags".into(), Value::Array(tags));
    }
    if let Some(steps) = c.text("X-AGENDA-STEPS").and_then(|s| serde_json::from_str::<Value>(&s).ok()).filter(Value::is_array) {
        d.insert("steps".into(), steps);
    }
    if let Some(cr) = c.get("CREATED").and_then(read_utc_ms) {
        d.insert("created_at".into(), json!(cr));
    }
    // Reminder: first VALARM with a relative trigger.
    if let Some(s) = str_of(&d, "start").map(str::to_owned)
        && let Some(base) = reminder_base(&s)
    {
        for a in c.children.iter().filter(|a| a.name == "VALARM") {
            let Some(tr) = a.get("TRIGGER") else { continue };
            let Some(off) = parse_dur(&tr.value) else { continue };
            let anchor = if tr.param("RELATED") == Some("END") { due.or(dtstart) } else { dtstart.or(due) };
            let Some((anchor_dt, _)) = anchor else { continue };
            let fire = anchor_dt + chrono::Duration::minutes(off);
            d.insert("reminder_min".into(), json!((base - fire).num_minutes().max(0)));
            break;
        }
    }
    (id, d, c.text("X-AGENDA-LIST"))
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct ImportResult {
    pub added: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub skipped: u32,
    /// Tasks changed here after the file was written; the newer local version is kept.
    pub kept: u32,
}

/// Import every VTODO. A UID that matches an existing task updates it in place,
/// so importing the same file twice never makes duplicates.
pub fn import(store: &Store, text: &str) -> Result<ImportResult> {
    let mut res = ImportResult::default();
    let mut lists = tasks::lists(store)?;
    let todos: Vec<Component> = parse(text)
        .into_iter()
        .flat_map(|cal| if cal.name == "VTODO" { vec![cal] } else { cal.children })
        .filter(|c| c.name == "VTODO")
        .collect();
    if todos.is_empty() {
        return Err(selfkit_core::Error::Format("No tasks (VTODO) found in this file.".into()));
    }
    for c in &todos {
        let (id, mut d, list_name) = task_from_vtodo(c);
        let list_id = match list_name {
            Some(n) if !n.trim().is_empty() => match lists.iter().find(|l| l.name.eq_ignore_ascii_case(n.trim())) {
                Some(l) => l.id.clone(),
                None => {
                    let nid = tasks::add_list(store, &n)?;
                    lists = tasks::lists(store)?;
                    nid
                }
            },
            _ => tasks::INBOX.to_string(),
        };
        d.insert("list".into(), json!(list_id));
        let existing = tasks::get(store, &id)?;
        let mut merged = match &existing {
            Some(t) => {
                let mut m = t.data.clone();
                let was_done = m.get("done_at").cloned().filter(|v| !v.is_null());
                for k in MANAGED {
                    m.remove(k);
                }
                let created = m.get("created_at").cloned();
                m.extend(d);
                if let (Some(old), true) = (was_done, m.contains_key("done_at")) {
                    m.insert("done_at".into(), old); // keep the exact original time
                }
                if let Some(c) = created {
                    m.insert("created_at".into(), c);
                }
                m
            }
            None => {
                if !d.contains_key("created_at") {
                    d.insert("created_at".into(), json!(time::now_ms()));
                }
                d
            }
        };
        merged = tasks::normalize(merged);
        // Empty fields ("" notes, [] tags/steps from the form) aren't exported, and numbers
        // compare by value: a re-import of an unchanged export is "unchanged" (L10).
        let bare = |m: &Map<String, Value>| {
            Value::Object(m.iter().filter(|(_, v)| !matches!(v, Value::Null) && v.as_str() != Some("") && v.as_array().is_none_or(|a| !a.is_empty())).map(|(k, v)| (k.clone(), v.clone())).collect())
        };
        if existing.as_ref().is_some_and(|t| selfkit_core::conflict::json_eq(&bare(&t.data), &bare(&merged))) {
            res.unchanged += 1;
            continue;
        }
        let file_time = c.get("LAST-MODIFIED").and_then(read_utc_ms);
        if let (Some(t), Some(ft)) = (&existing, file_time)
            && ft + 999 < t.updated_at
        {
            res.kept += 1; // an older export must not undo newer changes
            continue;
        }
        match tasks::put_raw(store, &id, merged) {
            Ok(()) if existing.is_some() => res.updated += 1,
            Ok(()) => res.added += 1,
            Err(_) => res.skipped += 1,
        }
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{Filter, NewTask, create, list, lists};

    fn store() -> Store {
        Store::open_in_memory("agenda", "dev", tasks::MIGRATIONS).unwrap()
    }

    #[test]
    fn durations() {
        assert_eq!(parse_dur("-PT30M"), Some(-30));
        assert_eq!(parse_dur("-P1D"), Some(-1440));
        assert_eq!(parse_dur("PT8H30M"), Some(510));
        assert_eq!(parse_dur("-P1DT2H"), Some(-1560));
        assert_eq!(parse_dur("P1W"), Some(10080));
        for m in [-1560, -90, -30, 0, 45, 510, 1440] {
            assert_eq!(parse_dur(&fmt_dur(m)), Some(m));
        }
    }

    #[test]
    fn round_trip_keeps_every_field() {
        let a = store();
        let study = tasks::add_list(&a, "Study, hard; really").unwrap();
        create(&a, NewTask {
            title: "Pay rent, now; ok".into(),
            notes: Some("Line 1\nLine **2** with a very long text that has to be folded because it is longer than seventy-five octets ✓✓✓".into()),
            severity: Some(4),
            start: Some("2026-11-16T17:00".into()),
            reminder_min: Some(60),
            tags: Some(vec!["home".into(), "money".into()]),
            list: Some(study),
            steps: Some(vec![json!({"id":"a","title":"Check, balance","done":true})]),
            ..Default::default()
        })
        .unwrap();
        create(&a, NewTask { title: "Standup".into(), start: Some("2026-11-16T09:00".into()), stop: Some("2026-11-16T09:15".into()), repeat: Some("weekdays".into()), reminder_min: Some(5), stage: Some("doing".into()), ..Default::default() }).unwrap();
        create(&a, NewTask { title: "Birthday".into(), start: Some("2026-12-01".into()), reminder_min: Some(1440), repeat: Some("yearly".into()), ..Default::default() }).unwrap();
        create(&a, NewTask { title: "All day due".into(), start: Some("2026-12-02".into()), reminder_min: Some(30), severity: Some(1), ..Default::default() }).unwrap();
        let d = create(&a, NewTask { title: "Done one".into(), ..Default::default() }).unwrap();
        tasks::toggle(&a, &d.id, Local::now().naive_local()).unwrap();
        let text = export(&list(&a, Filter::All).unwrap(), &lists(&a).unwrap(), "0.0.0");
        assert!(text.lines().all(|l| l.len() <= 76), "folded");

        let b = store();
        let r = import(&b, &text).unwrap();
        assert_eq!(r, ImportResult { added: 5, ..Default::default() });
        let strip = |s: &Store| {
            let ls = lists(s).unwrap();
            let mut v: Vec<(String, Map<String, Value>)> = list(s, Filter::All)
                .unwrap()
                .into_iter()
                .map(|t| {
                    let mut d = t.data;
                    for k in ["created_at", "last_completion", "done_at"] {
                        d.remove(k);
                    }
                    if let Some(l) = d.get("list").and_then(Value::as_str) {
                        let name = ls.iter().find(|x| x.id == l).map(|x| x.name.clone()).unwrap();
                        d.insert("list".into(), json!(name));
                    }
                    (t.id, d)
                })
                .collect();
            v.sort_by(|x, y| x.0.cmp(&y.0));
            v
        };
        assert_eq!(strip(&a), strip(&b));
        assert_eq!(list(&b, Filter::Done).unwrap().len(), 1);
        // Importing again changes nothing and makes no duplicates.
        assert_eq!(import(&b, &text).unwrap(), ImportResult { unchanged: 5, ..Default::default() });
        assert_eq!(import(&a, &text).unwrap().unchanged, 5);
        // A task changed after the export keeps its newer version.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let t = list(&a, Filter::Active).unwrap().into_iter().find(|t| t.data["title"] == "Birthday").unwrap();
        let mut p = Map::new();
        p.insert("title".into(), json!("Birthday (edited)"));
        tasks::update(&a, &t.id, &p).unwrap();
        let r = import(&a, &text).unwrap();
        assert_eq!((r.kept, r.unchanged), (1, 4));
        assert_eq!(tasks::get(&a, &t.id).unwrap().unwrap().data["title"], "Birthday (edited)");
    }

    #[test]
    fn reimport_with_empty_form_fields_is_unchanged() {
        // L10: tasks saved by the form carry notes "", tags [], steps []; an export drops them.
        let s = store();
        let mut d = Map::new();
        for (k, v) in [("title", json!("Bazar করো 🛒")), ("notes", json!("")), ("tags", json!([])), ("steps", json!([])), ("severity", json!(3)), ("list", json!(tasks::INBOX)), ("created_at", json!(1_763_000_000_000_i64))] {
            d.insert(k.into(), v);
        }
        tasks::put_raw(&s, "t1", d).unwrap();
        let text = export(&list(&s, Filter::All).unwrap(), &lists(&s).unwrap(), "0.0.0");
        assert_eq!(import(&s, &text).unwrap(), ImportResult { unchanged: 1, ..Default::default() });
    }

    #[test]
    fn foreign_vtodo() {
        let text = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VTODO\r\nUID:weird uid/with:chars\r\nSUMMARY:Buy milk\r\nDUE:20261120T100000Z\r\nPRIORITY:1\r\nCATEGORIES:Shop,Food\r\nCATEGORIES:urgent\r\nDESCRIPTION:a\\nb\\, c\r\n  continued\r\nBEGIN:VALARM\r\nTRIGGER;RELATED=END:-PT15M\r\nACTION:DISPLAY\r\nEND:VALARM\r\nEND:VTODO\r\nBEGIN:VEVENT\r\nUID:e\r\nSUMMARY:not a task\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let s = store();
        assert_eq!(import(&s, text).unwrap().added, 1);
        let t = &list(&s, Filter::All).unwrap()[0];
        assert!(t.id.starts_with("ics-"));
        assert_eq!(t.data["title"], "Buy milk");
        assert_eq!(t.data["severity"], 5);
        assert_eq!(t.data["tags"], json!(["shop", "food", "urgent"]));
        assert_eq!(t.data["notes"], "a\nb, c continued");
        assert_eq!(t.data["reminder_min"], 15);
        assert!(t.data["start"].as_str().unwrap().starts_with("2026-11-2"));
        assert!(import(&s, "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n").is_err());
    }
}
