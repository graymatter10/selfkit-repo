//! Recurring transactions and subscriptions: each `recurring` record holds a
//! transaction `template`, a frequency (`daily|weekly|monthly|yearly` ×
//! `interval`), `start`, `next` and optional `end`. `generate` creates every
//! transaction whose date is ≤ today (source `recurring`) and moves `next` on.
//! Monthly/yearly items keep the start day, clamped to short months. Generated
//! transactions get the id `rec-<recurring id>-<date>` (same on every device, C18).

use chrono::{Datelike, Duration, NaiveDate};
use serde_json::{Value, json};

use crate::model::{self, R, RECURRING, TRANSACTIONS, date, f, s};
use selfkit_core::Store;

fn add_months(start: NaiveDate, months: i32) -> NaiveDate {
    let total = start.year() * 12 + start.month0() as i32 + months;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let mut d = start.day();
    loop {
        if let Some(x) = NaiveDate::from_ymd_opt(y, m, d) {
            return x;
        }
        d -= 1;
    }
}

/// The `n`-th occurrence (0 = start).
pub fn occurrence(freq: &str, interval: i64, start: NaiveDate, n: i64) -> NaiveDate {
    let k = interval.max(1) * n;
    match freq {
        "daily" => start + Duration::days(k),
        "weekly" => start + Duration::weeks(k),
        "yearly" => add_months(start, (k * 12) as i32),
        _ => add_months(start, k as i32),
    }
}

/// Next occurrence strictly after `after`.
pub fn next_after(r: &Value, after: NaiveDate) -> Option<NaiveDate> {
    let start = date(r, "start")?;
    let iv = f(r, "interval").max(1.0) as i64;
    let freq = s(r, "freq");
    let mut n = 0;
    loop {
        let d = occurrence(freq, iv, start, n);
        if d > after {
            return Some(d);
        }
        n += 1;
        if n > 100_000 {
            return None;
        }
    }
}

/// Create due transactions for every active item. Returns how many were made.
pub fn generate(store: &Store, main: &str, today: NaiveDate) -> R<usize> {
    let mut made = 0;
    for r in model::all(store, RECURRING)? {
        if r.get("active").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let Some(mut next) = date(&r, "next") else { continue };
        let end = date(&r, "end");
        let mut guard = 0;
        while next <= today && end.is_none_or(|e| next <= e) && guard < 400 {
            let mut t = r["template"].clone();
            t["date"] = json!(next.format("%Y-%m-%d").to_string());
            t["source"] = json!("recurring");
            t["recurring"] = json!(s(&r, "id"));
            if s(&t, "note").is_empty() {
                t["note"] = json!(s(&r, "name"));
            }
            let t = model::normalize(TRANSACTIONS, &t, main)?;
            // Same id on every device (`rec-<item>-<date>`), so two devices generating the
            // same due item before syncing make one transaction, not two (C18). An existing
            // one (also a deleted one) is left alone.
            let id = format!("rec-{}-{}", s(&r, "id"), next.format("%Y-%m-%d"));
            if store.get(&id).map_err(|e| e.to_string())?.is_none() {
                store.put(TRANSACTIONS, Some(&id), &t).map_err(|e| e.to_string())?;
                made += 1;
            }
            guard += 1;
            next = match next_after(&r, next) {
                Some(n) => n,
                None => break,
            };
        }
        if Some(next) != date(&r, "next") {
            let mut upd = r.clone();
            upd["next"] = json!(next.format("%Y-%m-%d").to_string());
            if end.is_some_and(|e| next > e) {
                upd["active"] = json!(false);
            }
            let id = s(&r, "id").to_string();
            let upd = model::normalize(RECURRING, &upd, main)?;
            store.put(RECURRING, Some(&id), &upd).map_err(|e| e.to_string())?;
        }
    }
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{Data, Filter, filter_tx};

    #[test]
    fn month_end_clamps() {
        let j31 = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        assert_eq!(occurrence("monthly", 1, j31, 1), NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(occurrence("monthly", 1, j31, 2), NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
        assert_eq!(occurrence("yearly", 1, NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(), 1), NaiveDate::from_ymd_opt(2025, 2, 28).unwrap());
        assert_eq!(occurrence("weekly", 2, j31, 1), NaiveDate::from_ymd_opt(2026, 2, 14).unwrap());
    }

    #[test]
    fn generates_due_items_once() {
        let st = Store::open_in_memory("hisab", "d", &[]).unwrap();
        let r = model::normalize(
            RECURRING,
            &json!({"name":"Netflix","freq":"monthly","start":"2026-09-15","subscription":true,
                    "template":{"kind":"expense","amount":9.99,"account":"cash","category":"fun"}}),
            "USD",
        )
        .unwrap();
        st.put(RECURRING, Some("r1"), &r).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        assert_eq!(generate(&st, "USD", today).unwrap(), 3); // Sep 15, Oct 15, Nov 15
        assert_eq!(generate(&st, "USD", today).unwrap(), 0);
        let after = model::get(&st, "r1").unwrap().unwrap();
        assert_eq!(s(&after, "next"), "2026-12-15");
        let d = Data::load(&st, "USD").unwrap();
        let txs = filter_tx(&d, &Filter::default());
        assert_eq!(txs.len(), 3);
        assert_eq!(s(&txs[0], "note"), "Netflix");
        assert_eq!(s(&txs[0], "source"), "recurring");
        // A second device generating the same items before syncing writes the same ids (C18).
        let st2 = Store::open_in_memory("hisab", "e", &[]).unwrap();
        st2.put(RECURRING, Some("r1"), &r).unwrap();
        assert_eq!(generate(&st2, "USD", today).unwrap(), 3);
        let ids = |st: &Store| {
            let mut v: Vec<String> = st.list(TRANSACTIONS, 100, 0).unwrap().into_iter().map(|x| x.id).collect();
            v.sort();
            v
        };
        assert_eq!(ids(&st), ids(&st2));
    }

    #[test]
    fn end_date_stops() {
        let st = Store::open_in_memory("hisab", "d", &[]).unwrap();
        let r = model::normalize(
            RECURRING,
            &json!({"name":"Gym","freq":"weekly","start":"2026-11-01","end":"2026-11-10",
                    "template":{"kind":"expense","amount":5,"account":"cash","category":"fun"}}),
            "USD",
        )
        .unwrap();
        st.put(RECURRING, Some("r1"), &r).unwrap();
        assert_eq!(generate(&st, "USD", NaiveDate::from_ymd_opt(2026, 11, 30).unwrap()).unwrap(), 2);
        assert_eq!(model::get(&st, "r1").unwrap().unwrap()["active"], json!(false));
    }
}
