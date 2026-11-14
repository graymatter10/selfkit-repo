//! Where activity and connection data come from:
//! - `chrono.db` (desktop, read-only): `app_sessions` + `web_sessions`.
//! - `dusk.db` `device_use`: phone screen-on intervals written by Dusk on Android.
//! - Android UsageStatsManager events (via the Kotlin bridge) → `device_use`.
//! - `agenda.db` (read-only): tasks done per day.
//! - `hisab.db` (read-only, optional): money spent per day.
//!   Docs: docs/apps/dusk/README.md (Data model, Reads).

use std::collections::BTreeMap;
use std::path::Path;

use chrono::NaiveDate;
use selfkit_core::Store;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::model::{self, MIN, Ms, Span};

/// Read-only handle on another app's DB, or None if it doesn't exist.
pub fn open_other(app: &str) -> Option<Store> {
    let p = selfkit_core::paths::app_db(app);
    open_other_at(&p, app)
}

pub fn open_other_at(p: &Path, app: &str) -> Option<Store> {
    if !p.exists() {
        return None;
    }
    Store::open_read_only(p, app).ok()
}

fn num(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)))
}

/// Spans from records whose data has `start`/`end` (UTC ms) and that overlap
/// [from, to). Uses `updated_at >= from` as an index prefilter: a session is
/// written after it ends, so it can't overlap the range if written before it.
pub fn session_spans(store: &Store, collections: &[&str], from: Ms, to: Ms) -> Vec<Span> {
    let mut out = Vec::new();
    for c in collections {
        let Ok(mut st) = store.conn().prepare(
            "SELECT data, tz_offset FROM records WHERE collection=?1 AND deleted=0 AND updated_at>=?2
             AND CAST(json_extract(data,'$.end') AS INTEGER) > ?2 AND CAST(json_extract(data,'$.start') AS INTEGER) < ?3",
        ) else {
            continue;
        };
        let rows = st.query_map((*c, from, to), |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)));
        let Ok(rows) = rows else { continue };
        for (data, rec_tz) in rows.flatten() {
            let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
            let (Some(s), Some(e)) = (num(&v, "start"), num(&v, "end")) else { continue };
            let tz = num(&v, "tz_offset").unwrap_or(rec_tz) as i32;
            out.push(Span { start: s, end: e, tz });
        }
    }
    out
}

/// Desktop activity from Chrono (all devices that sync into chrono.db).
pub fn chrono_spans(from: Ms, to: Ms) -> (Vec<Span>, bool) {
    match open_other("chrono") {
        Some(s) => (session_spans(&s, &["app_sessions", "web_sessions"], from, to), true),
        None => (vec![], false),
    }
}

#[derive(Deserialize)]
struct DeviceUse {
    #[serde(default)]
    intervals: Vec<(Ms, Ms)>,
    #[serde(default)]
    tz_offset: i32,
}

/// Phone intervals stored in dusk.db (from this phone or synced from it).
pub fn device_use_spans(store: &Store, from: Ms, to: Ms) -> Vec<Span> {
    let mut out = Vec::new();
    let Ok(mut st) = store.conn().prepare("SELECT data FROM records WHERE collection='device_use' AND deleted=0") else { return out };
    let Ok(rows) = st.query_map([], |r| r.get::<_, String>(0)) else { return out };
    for data in rows.flatten() {
        let Ok(u) = serde_json::from_str::<DeviceUse>(&data) else { continue };
        for (s, e) in u.intervals {
            if e > from && s < to {
                out.push(Span { start: s, end: e, tz: u.tz_offset });
            }
        }
    }
    out
}

// ---- Android usage events ----------------------------------------------

/// UsageEvents.Event types the Kotlin bridge sends.
pub const ACTIVITY_RESUMED: i32 = 1;
pub const ACTIVITY_PAUSED: i32 = 2;
pub const SCREEN_INTERACTIVE: i32 = 15;
pub const SCREEN_NON_INTERACTIVE: i32 = 16;
pub const KEYGUARD_HIDDEN: i32 = 18;

/// A screen-on period shorter than this, without an unlock or an app
/// opening, is a notification lighting the screen, not use.
const MIN_GLANCE: Ms = 45_000;

/// Turn raw events `[time, type]` (sorted or not) into use intervals.
/// Screen interactive → non-interactive periods; if the device sends no
/// screen events, activity resumed → paused periods instead.
pub fn intervals_from_events(mut ev: Vec<(Ms, i32)>, from: Ms, to: Ms) -> Vec<(Ms, Ms)> {
    ev.sort();
    let has_screen = ev.iter().any(|e| e.1 == SCREEN_INTERACTIVE || e.1 == SCREEN_NON_INTERACTIVE);
    let mut out = Vec::new();
    if has_screen {
        let mut on: Option<Ms> = None;
        let mut used = false;
        // Screen already on at `from` if the first screen event is "off".
        if ev.iter().find(|e| e.1 == SCREEN_INTERACTIVE || e.1 == SCREEN_NON_INTERACTIVE).is_some_and(|e| e.1 == SCREEN_NON_INTERACTIVE) {
            on = Some(from);
            used = true;
        }
        for (t, k) in ev {
            match k {
                SCREEN_INTERACTIVE if on.is_none() => {
                    on = Some(t);
                    used = false;
                }
                KEYGUARD_HIDDEN | ACTIVITY_RESUMED if on.is_some() => used = true,
                SCREEN_NON_INTERACTIVE => {
                    if let Some(s) = on.take()
                        && (used || t - s >= MIN_GLANCE)
                    {
                        out.push((s, t));
                    }
                }
                _ => {}
            }
        }
        if let Some(s) = on
            && to > s
        {
            out.push((s, to));
        }
    } else {
        let mut open: Option<Ms> = None;
        for (t, k) in ev {
            match k {
                ACTIVITY_RESUMED if open.is_none() => open = Some(t),
                ACTIVITY_PAUSED => {
                    if let Some(s) = open.take() {
                        out.push((s, t));
                    }
                }
                _ => {}
            }
        }
    }
    out.retain(|(s, e)| e > s);
    out
}

/// Union two interval lists.
pub fn union(a: Vec<(Ms, Ms)>, b: Vec<(Ms, Ms)>) -> Vec<(Ms, Ms)> {
    let spans = a.into_iter().chain(b).map(|(s, e)| Span { start: s, end: e, tz: 0 }).collect();
    model::merge_spans(spans).into_iter().map(|s| (s.start, s.end)).collect()
}

/// Store phone intervals per night in `device_use` (merged with what is
/// already there, because Android only keeps a few days of events).
/// Returns the nights touched.
pub fn save_device_use(store: &Store, device: &str, tz: i32, intervals: &[(Ms, Ms)]) -> selfkit_core::Result<Vec<String>> {
    let mut by: BTreeMap<NaiveDate, Vec<(Ms, Ms)>> = BTreeMap::new();
    for &(s, e) in intervals {
        // Split at night-window borders.
        let mut a = s;
        while a < e {
            let n = model::night_of(a, tz);
            let (_, we) = model::window(n, tz);
            let b = e.min(we);
            by.entry(n).or_default().push((a, b));
            a = b;
        }
    }
    let mut touched = Vec::new();
    for (n, v) in by {
        let key = model::date_str(n);
        let id = format!("u-{device}-{key}");
        let old: Vec<(Ms, Ms)> = store
            .get(&id)?
            .filter(|r| !r.deleted)
            .and_then(|r| serde_json::from_value::<DeviceUse>(r.data).ok())
            .map(|u| u.intervals)
            .unwrap_or_default();
        let merged = union(old.clone(), v);
        if merged != old {
            store.put(
                "device_use",
                Some(&id),
                &json!({"night": key, "device": device, "platform": "android", "tz_offset": tz, "intervals": merged, "source": "usage_stats"}),
            )?;
        }
        touched.push(key);
    }
    Ok(touched)
}

// ---- connections data ----------------------------------------------------

/// Logical day of a record (UTC ms + its tz offset).
fn day_of(ms: Ms, tz: i32, day_start: u16) -> NaiveDate {
    model::logical_day(ms, tz, day_start)
}

/// Tasks done per logical day, from agenda.db. Covers every day from the
/// first task ever created to today (days without tasks done = 0).
pub fn tasks_done_per_day(agenda: &Store, day_start: u16, today: NaiveDate) -> BTreeMap<NaiveDate, f64> {
    let mut out: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let Ok(mut st) = agenda.conn().prepare("SELECT data, tz_offset, updated_at FROM records WHERE collection='tasks' AND deleted=0") else { return out };
    let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))) else { return out };
    let mut first: Option<NaiveDate> = None;
    for (data, tz, upd) in rows.flatten() {
        let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
        let created = num(&v, "created_at").unwrap_or(upd);
        let cd = day_of(created, tz as i32, day_start);
        first = Some(first.map_or(cd, |f: NaiveDate| f.min(cd)));
        if let Some(done) = num(&v, "done_at").filter(|d| *d > 0) {
            *out.entry(day_of(done, tz as i32, day_start)).or_default() += 1.0;
        }
    }
    fill_days(out, first, today)
}

fn fill_days(mut m: BTreeMap<NaiveDate, f64>, first: Option<NaiveDate>, today: NaiveDate) -> BTreeMap<NaiveDate, f64> {
    if let Some(mut d) = first {
        while d <= today {
            m.entry(d).or_insert(0.0);
            d += chrono::Duration::days(1);
        }
    }
    m
}

/// Money spent per logical day from hisab.db `transactions` (falls back to
/// `expenses`). Hisab's shape (apps/hisab/src-tauri/src/model.rs):
/// `{kind: "expense"|"income"|"transfer", amount, currency, amount_main, date: "YYYY-MM-DD", time: "HH:MM"|""}`.
/// Uses `amount_main` (main currency) when present, else `amount` converted
/// from its `currency` into `main` with Hisab's `rates` record (Hisab never
/// stores `amount_main`; QA 2026-11-15 found BDT amounts summed as if they were
/// USD). An amount in a currency without a rate is left out. A time
/// before the day-start hour counts for the day before. Unknown shapes are
/// skipped, so a changed Hisab schema degrades to "no data".
pub fn spent_per_day(hisab: &Store, day_start: u16, today: NaiveDate, main: &str) -> BTreeMap<NaiveDate, f64> {
    let mut out: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let mut first: Option<NaiveDate> = None;
    let rates = hisab_rates(hisab);
    let main = main.to_ascii_uppercase();
    for coll in ["transactions", "expenses"] {
        let Ok(mut st) = hisab.conn().prepare("SELECT data, tz_offset FROM records WHERE collection=?1 AND deleted=0") else { continue };
        let Ok(rows) = st.query_map([coll], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) else { continue };
        for (data, rec_tz) in rows.flatten() {
            let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
            let Some((day, amount)) = hisab_expense(&v, rec_tz as i32, day_start, coll == "expenses") else { continue };
            first = Some(first.map_or(day, |f: NaiveDate| f.min(day)));
            let has_main = v.get("amount_main").is_some_and(|x| !x.is_null());
            let cur = v.get("currency").and_then(Value::as_str).unwrap_or("").to_ascii_uppercase();
            let amount = if has_main || cur.is_empty() || cur == main {
                amount
            } else {
                match (rates.get(&cur), rates.get(&main)) {
                    (Some(a), Some(b)) => amount / a * b,
                    _ => continue,
                }
            };
            if amount > 0.0 {
                *out.entry(day).or_default() += amount;
            }
        }
        if first.is_some() {
            break;
        }
    }
    fill_days(out, first, today)
}

/// Hisab's exchange rates (record `hisab-rates`: units per 1 `base`; `manual`
/// overrides `rates`), as currency → rate. Empty when there is none.
pub fn hisab_rates(hisab: &Store) -> std::collections::HashMap<String, f64> {
    let mut m = std::collections::HashMap::new();
    let data: Option<String> = hisab.conn().query_row("SELECT data FROM records WHERE id='hisab-rates' AND deleted=0", [], |r| r.get(0)).ok();
    let Some(v) = data.and_then(|d| serde_json::from_str::<Value>(&d).ok()) else { return m };
    for k in ["rates", "manual"] {
        if let Some(o) = v.get(k).and_then(Value::as_object) {
            for (c, r) in o {
                if let Some(x) = r.as_f64().filter(|x| *x > 0.0 && x.is_finite()) {
                    m.insert(c.to_ascii_uppercase(), x);
                }
            }
        }
    }
    let base = v.get("base").and_then(Value::as_str).unwrap_or("USD").to_ascii_uppercase();
    m.insert(base, 1.0);
    m
}

/// (day, spent amount; 0 for income/transfers) of one Hisab record.
pub fn hisab_expense(v: &Value, rec_tz: i32, day_start: u16, all_expenses: bool) -> Option<(NaiveDate, f64)> {
    let tz = num(v, "tz_offset").map(|x| x as i32).unwrap_or(rec_tz);
    let day = match v.get("date").and_then(Value::as_str).and_then(|s| NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()) {
        Some(d) => {
            let t = v.get("time").and_then(Value::as_str).and_then(|t| chrono::NaiveTime::parse_from_str(t, "%H:%M").ok());
            match t {
                Some(t) if (chrono::Timelike::hour(&t) * 60 + chrono::Timelike::minute(&t)) < day_start as u32 => d - chrono::Duration::days(1),
                _ => d,
            }
        }
        None => ["at", "occurred_at", "ts"].iter().find_map(|k| Some(day_of(v.get(*k)?.as_i64()?, tz, day_start)))?,
    };
    let f = |k: &str| v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str()?.replace(',', "").parse().ok()));
    let amount = f("amount_main")
        .or_else(|| f("amount"))
        .or_else(|| f("amount_minor").map(|m| m / 100.0))
        .or_else(|| f("amount_cents").map(|m| m / 100.0))?;
    let kind = ["kind", "type"].iter().find_map(|k| v.get(*k).and_then(Value::as_str)).unwrap_or("");
    let spent = match kind {
        "expense" | "spend" | "out" => amount.abs(),
        "income" | "transfer" | "in" => 0.0,
        _ if all_expenses => amount.abs(),
        _ if amount < 0.0 => -amount,
        _ => 0.0,
    };
    Some((day, spent))
}

/// Minutes, used by the UI: whole minutes of a span list.
pub fn total_min(v: &[(Ms, Ms)]) -> i64 {
    v.iter().map(|(a, b)| b - a).sum::<Ms>() / MIN
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{local_to_utc, parse_date, parse_hhmm};

    #[test]
    fn events_to_intervals() {
        let ev = vec![
            (1_000, SCREEN_INTERACTIVE),
            (5_000, KEYGUARD_HIDDEN),
            (60_000, SCREEN_NON_INTERACTIVE),
            (100_000, SCREEN_INTERACTIVE), // notification glance, 10 s
            (110_000, SCREEN_NON_INTERACTIVE),
            (200_000, SCREEN_INTERACTIVE), // long look without unlocking (90 s)
            (290_000, SCREEN_NON_INTERACTIVE),
            (400_000, SCREEN_INTERACTIVE), // still on at the end
        ];
        assert_eq!(intervals_from_events(ev, 0, 500_000), vec![(1_000, 60_000), (200_000, 290_000), (400_000, 500_000)]);
        // starts with screen on
        assert_eq!(intervals_from_events(vec![(50_000, SCREEN_NON_INTERACTIVE)], 0, 90_000), vec![(0, 50_000)]);
        // no screen events: fall back to activities
        let ev = vec![(10, ACTIVITY_RESUMED), (500, ACTIVITY_PAUSED), (900, ACTIVITY_RESUMED), (950, ACTIVITY_PAUSED)];
        assert_eq!(intervals_from_events(ev, 0, 1000), vec![(10, 500), (900, 950)]);
    }

    #[test]
    fn device_use_splits_and_merges() {
        let s = Store::open_in_memory("dusk", "dev1", crate::MIGRATIONS).unwrap();
        let tz = 60;
        let d = |x: &str| parse_date(x).unwrap();
        let at = |x: &str, hm: &str| local_to_utc(d(x), parse_hhmm(hm).unwrap(), tz);
        // 17:30–18:30 crosses the window border
        let n = save_device_use(&s, "dev1", tz, &[(at("2026-03-10", "17:30"), at("2026-03-10", "18:30"))]).unwrap();
        assert_eq!(n, vec!["2026-03-10", "2026-03-11"]);
        save_device_use(&s, "dev1", tz, &[(at("2026-03-10", "18:20"), at("2026-03-10", "19:00"))]).unwrap();
        let spans = device_use_spans(&s, 0, i64::MAX);
        let mut v: Vec<(Ms, Ms)> = spans.iter().map(|s| (s.start, s.end)).collect();
        v.sort();
        assert_eq!(v, vec![(at("2026-03-10", "17:30"), at("2026-03-10", "18:00")), (at("2026-03-10", "18:00"), at("2026-03-10", "19:00"))]);
    }

    #[test]
    fn reads_chrono_and_agenda_and_hisab() {
        let s = Store::open_in_memory("chrono", "d", &[]).unwrap();
        s.put("app_sessions", None, &json!({"app":"code","platform":"linux","start":1000,"end":5000,"tz_offset":60})).unwrap();
        s.put("web_sessions", None, &json!({"domain":"x.org","start":6000,"end":9000,"tz_offset":60})).unwrap();
        s.put("app_sessions", None, &json!({"app":"old","start":10,"end":20,"tz_offset":60})).unwrap();
        let mut sp = session_spans(&s, &["app_sessions", "web_sessions"], 900, 100_000);
        sp.sort_by_key(|s| s.start);
        // updated_at prefilter: records written "now" are after 900, both overlap
        assert_eq!(sp.iter().map(|s| (s.start, s.end, s.tz)).collect::<Vec<_>>(), vec![(1000, 5000, 60), (6000, 9000, 60)]);

        let a = Store::open_in_memory("agenda", "d", &[]).unwrap();
        let today = chrono::Utc::now().date_naive();
        let now = chrono::Utc::now().timestamp_millis();
        a.put("tasks", None, &json!({"title":"a","done_at": now, "created_at": now - 3 * 86_400_000})).unwrap();
        a.put("tasks", None, &json!({"title":"b","done_at": null, "created_at": now})).unwrap();
        let t = tasks_done_per_day(&a, 0, today + chrono::Duration::days(1));
        assert_eq!(t.values().sum::<f64>(), 1.0);
        assert!(t.len() >= 4, "days are filled with zeros: {t:?}");

        let h = Store::open_in_memory("hisab", "d", &[]).unwrap();
        h.put("transactions", None, &json!({"kind":"expense","amount":12.5,"at": now})).unwrap();
        h.put("transactions", None, &json!({"kind":"expense","amount":1000,"amount_main":9.0,"currency":"BDT","date":"2026-01-05","time":"02:00"})).unwrap();
        h.put("transactions", None, &json!({"kind":"income","amount":100,"at": now})).unwrap();
        h.put("transactions", None, &json!({"amount_minor":-250,"date":"2026-01-02"})).unwrap();
        let m = spent_per_day(&h, 0, today + chrono::Duration::days(1), "USD");
        assert_eq!(m.get(&parse_date("2026-01-02").unwrap()), Some(&2.5));
        assert!(m.values().any(|v| (*v - 12.5).abs() < 1e-9));
        assert_eq!(m.get(&parse_date("2026-01-05").unwrap()), Some(&9.0));
        let m = spent_per_day(&h, 240, today + chrono::Duration::days(1), "USD");
        assert_eq!(m.get(&parse_date("2026-01-04").unwrap()), Some(&9.0), "02:00 is still the day before with a 04:00 day start");
    }

    #[test]
    fn hisab_amounts_are_converted_to_the_main_currency() {
        let h = Store::open_in_memory("hisab", "d", &[]).unwrap();
        h.put("rates", Some("hisab-rates"), &json!({"base":"USD","rates":{"BDT":120.0,"EUR":0.9},"manual":{"EUR":0.8}})).unwrap();
        h.put("transactions", None, &json!({"kind":"expense","amount":1200,"currency":"BDT","date":"2026-01-02","time":"12:00"})).unwrap();
        h.put("transactions", None, &json!({"kind":"expense","amount":8,"currency":"EUR","date":"2026-01-02","time":"13:00"})).unwrap();
        h.put("transactions", None, &json!({"kind":"expense","amount":5,"currency":"USD","date":"2026-01-02","time":"14:00"})).unwrap();
        h.put("transactions", None, &json!({"kind":"expense","amount":99,"currency":"XYZ","date":"2026-01-02","time":"15:00"})).unwrap();
        let today = parse_date("2026-01-03").unwrap();
        let m = spent_per_day(&h, 0, today, "USD");
        // 1200 BDT = 10 USD, 8 EUR (manual 0.8) = 10 USD, 5 USD; XYZ has no rate.
        assert!((m[&parse_date("2026-01-02").unwrap()] - 25.0).abs() < 1e-9, "{m:?}");
        let m = spent_per_day(&h, 0, today, "BDT");
        assert!((m[&parse_date("2026-01-02").unwrap()] - 3000.0).abs() < 1e-6, "{m:?}");
    }
}
