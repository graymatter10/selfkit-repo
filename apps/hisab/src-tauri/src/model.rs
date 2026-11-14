//! Hisab data model: collections, validation and small helpers.
//! Every collection is a selfkit-core record collection; see
//! docs/apps/hisab/README.md § Data model for the field list.

use chrono::NaiveDate;
use selfkit_core::Store;
use selfkit_core::store::Migration;
use serde_json::{Map, Value, json};

pub const MIGRATIONS: &[Migration] = &[];

pub const ACCOUNTS: &str = "accounts";
pub const CATEGORIES: &str = "categories";
pub const TRANSACTIONS: &str = "transactions";
pub const RECURRING: &str = "recurring";
pub const LOANS: &str = "loans";
pub const GOALS: &str = "goals";
pub const SMS: &str = "sms";
pub const RATES: &str = "rates";
/// Fixed id of the single exchange-rate record.
pub const RATES_ID: &str = "hisab-rates";

pub const COLLECTIONS: &[&str] = &[ACCOUNTS, CATEGORIES, TRANSACTIONS, RECURRING, LOANS, GOALS, SMS, RATES];

pub type R<T> = Result<T, String>;

// ---- JSON helpers -----------------------------------------------------------

pub fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
pub fn f(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).filter(|x| x.is_finite()).unwrap_or(0.0)
}
pub fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}
pub fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}
pub fn round2(x: f64) -> f64 {
    // `+ 0.0` turns -0.0 (an empty f64 sum) into 0.0, so the UI never shows "-$0".
    (x * 100.0).round() / 100.0 + 0.0
}
pub fn date(v: &Value, k: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s(v, k), "%Y-%m-%d").ok()
}

/// Record data with its id merged in (`{"id": …, …data}`), what the UI gets.
pub fn flat(id: &str, data: &Value) -> Value {
    let mut m = data.as_object().cloned().unwrap_or_default();
    m.insert("id".into(), Value::String(id.into()));
    Value::Object(m)
}

/// All live records of a collection, flattened.
pub fn all(store: &Store, coll: &str) -> R<Vec<Value>> {
    let recs = store.list(coll, u32::MAX, 0).map_err(|e| e.to_string())?;
    Ok(recs.iter().map(|r| flat(&r.id, &r.data)).collect())
}

pub fn get(store: &Store, id: &str) -> R<Option<Value>> {
    Ok(store.get(id).map_err(|e| e.to_string())?.filter(|r| !r.deleted).map(|r| flat(&r.id, &r.data)))
}

// ---- validation -----------------------------------------------------------

fn text(m: &Map<String, Value>, k: &str) -> String {
    m.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string()
}
fn num(m: &Map<String, Value>, k: &str) -> Option<f64> {
    match m.get(k) {
        Some(Value::Number(n)) => n.as_f64().filter(|x| x.is_finite()),
        Some(Value::String(s)) => s.trim().replace(',', "").parse::<f64>().ok().filter(|x| x.is_finite()),
        _ => None,
    }
}
fn currency(m: &Map<String, Value>, main: &str) -> R<String> {
    let c = text(m, "currency").to_ascii_uppercase();
    if c.is_empty() {
        return Ok(main.to_string());
    }
    if c.len() != 3 || !c.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Err(format!("Unknown currency “{c}”"));
    }
    Ok(c)
}
fn valid_date(sv: &str, what: &str) -> R<()> {
    NaiveDate::parse_from_str(sv, "%Y-%m-%d").map(|_| ()).map_err(|_| format!("{what} is not a valid date"))
}
fn one_of(v: &str, opts: &[&str], default: &str) -> String {
    if opts.contains(&v) { v.to_string() } else { default.to_string() }
}
fn tags(m: &Map<String, Value>) -> Value {
    let mut out: Vec<String> = Vec::new();
    if let Some(a) = m.get("tags").and_then(Value::as_array) {
        for t in a.iter().filter_map(Value::as_str) {
            let t = t.trim().trim_start_matches('#').trim().to_lowercase();
            if !t.is_empty() && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    json!(out)
}
fn str_list(m: &Map<String, Value>, k: &str) -> Value {
    json!(m.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>()).unwrap_or_default())
}
/// Largest amount accepted: 1 trillion minus a cent fits f64 with exact cents;
/// far bigger values turned into `null` (infinity) when stored (QA 2026-11-15).
pub const MAX_AMOUNT: f64 = 1e13;

fn positive(m: &Map<String, Value>, k: &str, what: &str) -> R<f64> {
    match num(m, k) {
        Some(x) if x >= MAX_AMOUNT => Err(format!("{what} is too large")),
        Some(x) if round2(x) > 0.0 => Ok(round2(x)),
        _ => Err(format!("{what} must be more than 0")),
    }
}

/// Validate and normalize a record before it is stored. `main` is the main
/// currency (shared settings), used when no currency is given.
pub fn normalize(coll: &str, data: &Value, main: &str) -> R<Value> {
    let m = data.as_object().ok_or("Expected an object")?;
    let mut out = m.clone();
    out.remove("id");
    // Computed fields from views must never be stored.
    for k in ["balance", "balance_main", "usage", "spent", "repaid", "remaining", "remaining_main", "status", "days_left", "saved", "monthly"] {
        out.remove(k);
    }
    match coll {
        ACCOUNTS => {
            let name = text(m, "name");
            if name.is_empty() {
                return Err("Give the account a name".into());
            }
            out.insert("name".into(), json!(name));
            out.insert("kind".into(), json!(one_of(&text(m, "kind"), &["cash", "bank", "wallet", "card", "other"], "cash")));
            out.insert("currency".into(), json!(currency(m, main)?));
            let opening = num(m, "opening").unwrap_or(0.0);
            if opening.abs() >= MAX_AMOUNT {
                return Err("Opening balance is too large".into());
            }
            out.insert("opening".into(), json!(round2(opening)));
            out.insert("archived".into(), json!(b(data, "archived")));
            out.insert("order".into(), json!(num(m, "order").unwrap_or(0.0)));
        }
        CATEGORIES => {
            let name = text(m, "name");
            if name.is_empty() {
                return Err("Give the category a name".into());
            }
            out.insert("name".into(), json!(name));
            out.insert("kind".into(), json!(one_of(&text(m, "kind"), &["expense", "income"], "expense")));
            let icon = text(m, "icon");
            out.insert("icon".into(), json!(if icon.is_empty() { "dot".to_string() } else { icon }));
            let color = text(m, "color");
            out.insert("color".into(), json!(if color.is_empty() { "#8a8a8e".to_string() } else { color }));
            let budget = num(m, "budget").unwrap_or(0.0);
            if budget < 0.0 {
                return Err("Budget can't be negative".into());
            }
            if budget >= MAX_AMOUNT {
                return Err("Budget is too large".into());
            }
            out.insert("budget".into(), json!(round2(budget)));
            out.insert("archived".into(), json!(b(data, "archived")));
            out.insert("order".into(), json!(num(m, "order").unwrap_or(0.0)));
        }
        TRANSACTIONS => tx(m, &mut out, main)?,
        RECURRING => {
            let freq = one_of(&text(m, "freq"), &["daily", "weekly", "monthly", "yearly"], "monthly");
            out.insert("freq".into(), json!(freq));
            out.insert("interval".into(), json!(num(m, "interval").unwrap_or(1.0).max(1.0).round()));
            let start = text(m, "start");
            valid_date(&start, "Start date")?;
            let next = text(m, "next");
            let next = if next.is_empty() { start.clone() } else { next };
            valid_date(&next, "Next date")?;
            out.insert("start".into(), json!(start));
            out.insert("next".into(), json!(next));
            let end = text(m, "end");
            if !end.is_empty() {
                valid_date(&end, "End date")?;
            }
            out.insert("end".into(), json!(end));
            out.insert("active".into(), json!(m.get("active").and_then(Value::as_bool).unwrap_or(true)));
            out.insert("subscription".into(), json!(b(data, "subscription")));
            let tpl = m.get("template").ok_or("Missing the transaction")?;
            let mut t = tpl.as_object().cloned().ok_or("Missing the transaction")?;
            t.insert("date".into(), json!(next));
            let tn = normalize(TRANSACTIONS, &Value::Object(t), main)?;
            let mut tn = tn.as_object().cloned().unwrap_or_default();
            for k in ["date", "time", "source", "recurring", "photos", "created_at"] {
                tn.remove(k);
            }
            let name = text(m, "name");
            let name = if name.is_empty() { s(&Value::Object(tn.clone()), "note").to_string() } else { name };
            if name.is_empty() {
                return Err("Give it a name".into());
            }
            out.insert("name".into(), json!(name));
            out.insert("template".into(), Value::Object(tn));
        }
        LOANS => {
            out.insert("direction".into(), json!(one_of(&text(m, "direction"), &["lent", "borrowed"], "lent")));
            let person = text(m, "person");
            if person.is_empty() {
                return Err("Who is the loan with?".into());
            }
            out.insert("person".into(), json!(person));
            out.insert("title".into(), json!(text(m, "title")));
            out.insert("description".into(), json!(text(m, "description")));
            let amount = positive(m, "amount", "Amount")?;
            out.insert("amount".into(), json!(amount));
            out.insert("currency".into(), json!(currency(m, main)?));
            let given = text(m, "given");
            valid_date(&given, "Date given")?;
            out.insert("given".into(), json!(given));
            let due = text(m, "due");
            if !due.is_empty() {
                valid_date(&due, "Due date")?;
                if due < given {
                    return Err("Due date is before the date given".into());
                }
            }
            out.insert("due".into(), json!(due));
            out.insert("account".into(), json!(text(m, "account")));
            out.insert("photos".into(), str_list(m, "photos"));
            let mut reps = Vec::new();
            let mut total = 0.0;
            for r in m.get("repayments").and_then(Value::as_array).cloned().unwrap_or_default() {
                let rm = r.as_object().cloned().unwrap_or_default();
                let a = positive(&rm, "amount", "Repayment")?;
                let d = text(&rm, "date");
                valid_date(&d, "Repayment date")?;
                total += a;
                let id = text(&rm, "id");
                reps.push(json!({"id": if id.is_empty() { selfkit_core::store::new_id() } else { id }, "date": d, "amount": a, "note": text(&rm, "note")}));
            }
            if total > amount + 0.005 {
                return Err("Repayments are more than the loan".into());
            }
            out.insert("repayments".into(), json!(reps));
            out.insert("remind".into(), json!(m.get("remind").and_then(Value::as_bool).unwrap_or(true)));
            out.insert("remind_days".into(), json!(num(m, "remind_days").unwrap_or(1.0).clamp(0.0, 60.0).round()));
            let rt = text(m, "remind_time");
            let rt = if chrono::NaiveTime::parse_from_str(&rt, "%H:%M").is_ok() { rt } else { "09:00".into() };
            out.insert("remind_time".into(), json!(rt));
        }
        GOALS => {
            let name = text(m, "name");
            if name.is_empty() {
                return Err("Give the goal a name".into());
            }
            out.insert("name".into(), json!(name));
            out.insert("target".into(), json!(positive(m, "target", "Target")?));
            out.insert("currency".into(), json!(currency(m, main)?));
            let dl = text(m, "deadline");
            if !dl.is_empty() {
                valid_date(&dl, "Deadline")?;
            }
            out.insert("deadline".into(), json!(dl));
            let mut cs = Vec::new();
            for c in m.get("contributions").and_then(Value::as_array).cloned().unwrap_or_default() {
                let cm = c.as_object().cloned().unwrap_or_default();
                let a = num(&cm, "amount").unwrap_or(0.0);
                if a == 0.0 {
                    continue;
                }
                let d = text(&cm, "date");
                valid_date(&d, "Date")?;
                let id = text(&cm, "id");
                cs.push(json!({"id": if id.is_empty() { selfkit_core::store::new_id() } else { id }, "date": d, "amount": round2(a), "note": text(&cm, "note")}));
            }
            out.insert("contributions".into(), json!(cs));
            out.insert("archived".into(), json!(b(data, "archived")));
        }
        SMS => {
            out.insert("status".into(), json!(one_of(&text(m, "status"), &["pending", "saved", "dismissed"], "pending")));
        }
        RATES => {}
        _ => return Err(format!("unknown collection {coll}")),
    }
    Ok(Value::Object(out))
}

fn tx(m: &Map<String, Value>, out: &mut Map<String, Value>, main: &str) -> R<()> {
    let kind = one_of(&text(m, "kind"), &["expense", "income", "transfer"], "expense");
    let amount = positive(m, "amount", "Amount")?;
    out.insert("kind".into(), json!(kind));
    out.insert("amount".into(), json!(amount));
    out.insert("currency".into(), json!(currency(m, main)?));
    let d = text(m, "date");
    valid_date(&d, "Date")?;
    out.insert("date".into(), json!(d));
    let t = text(m, "time");
    out.insert("time".into(), json!(if chrono::NaiveTime::parse_from_str(&t, "%H:%M").is_ok() { t } else { String::new() }));
    let account = text(m, "account");
    if account.is_empty() {
        return Err("Choose an account".into());
    }
    out.insert("account".into(), json!(account));
    out.insert("tags".into(), tags(m));
    out.insert("photos".into(), str_list(m, "photos"));
    out.insert("note".into(), json!(text(m, "note")));
    out.insert("source".into(), json!(one_of(&text(m, "source"), &["manual", "sms", "recurring", "import"], "manual")));
    if kind == "transfer" {
        let to = text(m, "to_account");
        if to.is_empty() {
            return Err("Choose where the money goes".into());
        }
        if to == account {
            return Err("Pick two different accounts".into());
        }
        out.insert("to_account".into(), json!(to));
        out.insert("to_amount".into(), json!(num(m, "to_amount").filter(|x| *x > 0.0).map(round2).unwrap_or(amount)));
        out.insert("category".into(), json!(""));
        out.insert("splits".into(), json!([]));
        return Ok(());
    }
    out.remove("to_account");
    out.remove("to_amount");
    let mut splits = Vec::new();
    let mut sum = 0.0;
    for sp in m.get("splits").and_then(Value::as_array).cloned().unwrap_or_default() {
        let sm = sp.as_object().cloned().unwrap_or_default();
        let c = text(&sm, "category");
        let a = num(&sm, "amount").unwrap_or(0.0);
        if c.is_empty() || a <= 0.0 {
            continue;
        }
        sum += round2(a);
        splits.push(json!({"category": c, "amount": round2(a)}));
    }
    if splits.len() == 1 {
        out.insert("category".into(), splits[0]["category"].clone());
        splits.clear();
    } else if !splits.is_empty() {
        if (sum - amount).abs() > 0.005 {
            return Err(format!("Split parts add up to {:.2}, not {:.2}", sum, amount));
        }
        out.insert("category".into(), splits[0]["category"].clone());
    }
    if splits.is_empty() && text(out, "category").is_empty() {
        return Err("Choose a category".into());
    }
    out.insert("splits".into(), json!(splits));
    Ok(())
}

/// Category parts of a transaction: `(category id, amount in tx currency)`.
pub fn parts(t: &Value) -> Vec<(String, f64)> {
    let sp = arr(t, "splits");
    if sp.len() > 1 {
        sp.iter().map(|p| (s(p, "category").to_string(), f(p, "amount"))).collect()
    } else {
        vec![(s(t, "category").to_string(), f(t, "amount"))]
    }
}

/// Default categories and one cash account for a fresh install.
pub fn seed_defaults(store: &Store, main: &str) -> R<()> {
    if store.count(CATEGORIES).map_err(|e| e.to_string())? > 0 || store.count(ACCOUNTS).map_err(|e| e.to_string())? > 0 {
        return Ok(());
    }
    let put = |c: &str, v: Value| store.put(c, None, &v).map(|_| ()).map_err(|e| e.to_string());
    put(ACCOUNTS, json!({"name": "Cash", "kind": "cash", "currency": main, "opening": 0, "order": 0, "archived": false}))?;
    let exp = [
        ("Food", "food", "#d9822b"),
        ("Transport", "car", "#3b82c4"),
        ("Groceries", "cart", "#5a9e5f"),
        ("Bills", "bolt", "#c4a43b"),
        ("Shopping", "bag", "#a05ac4"),
        ("Health", "heart", "#d1495b"),
        ("Fun", "star", "#e0719c"),
        ("Other", "dot", "#8a8a8e"),
    ];
    for (i, (n, ic, col)) in exp.iter().enumerate() {
        put(CATEGORIES, json!({"name": n, "kind": "expense", "icon": ic, "color": col, "budget": 0, "order": i, "archived": false}))?;
    }
    let inc = [("Salary", "briefcase", "#4f8a55"), ("Gift", "gift", "#e0719c"), ("Other", "dot", "#8a8a8e")];
    for (i, (n, ic, col)) in inc.iter().enumerate() {
        put(CATEGORIES, json!({"name": n, "kind": "income", "icon": ic, "color": col, "budget": 0, "order": i, "archived": false}))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_validation_and_splits() {
        let ok = normalize(TRANSACTIONS, &json!({"kind":"expense","amount":"1,250.5","date":"2026-11-02","account":"a","category":"c","tags":["#Trip","trip"," Food "]}), "USD").unwrap();
        assert_eq!(f(&ok, "amount"), 1250.5);
        assert_eq!(s(&ok, "currency"), "USD");
        assert_eq!(ok["tags"], json!(["trip", "food"]));
        assert!(normalize(TRANSACTIONS, &json!({"kind":"expense","amount":0,"date":"2026-11-02","account":"a","category":"c"}), "USD").is_err());
        assert!(normalize(TRANSACTIONS, &json!({"kind":"expense","amount":5,"date":"2026-13-02","account":"a","category":"c"}), "USD").is_err());
        assert!(normalize(TRANSACTIONS, &json!({"kind":"expense","amount":5,"date":"2026-11-02","account":"a"}), "USD").is_err());
        let bad = normalize(TRANSACTIONS, &json!({"kind":"expense","amount":10,"date":"2026-11-02","account":"a","splits":[{"category":"x","amount":4},{"category":"y","amount":5}]}), "USD");
        assert!(bad.unwrap_err().contains("add up"));
        let sp = normalize(TRANSACTIONS, &json!({"kind":"expense","amount":10,"date":"2026-11-02","account":"a","splits":[{"category":"x","amount":4},{"category":"y","amount":6}]}), "USD").unwrap();
        assert_eq!(s(&sp, "category"), "x");
        assert_eq!(parts(&sp), vec![("x".into(), 4.0), ("y".into(), 6.0)]);
        let tr = normalize(TRANSACTIONS, &json!({"kind":"transfer","amount":10,"date":"2026-11-02","account":"a","to_account":"a"}), "USD");
        assert!(tr.is_err());
        let tr = normalize(TRANSACTIONS, &json!({"kind":"transfer","amount":10,"date":"2026-11-02","account":"a","to_account":"b"}), "USD").unwrap();
        assert_eq!(f(&tr, "to_amount"), 10.0);
        // Extremes (QA): 1e12 with cents is kept exactly; absurd amounts are refused, not stored as null.
        let big = normalize(TRANSACTIONS, &json!({"kind":"expense","amount":999999999999.99,"date":"2026-11-02","account":"a","category":"c"}), "USD").unwrap();
        assert_eq!(f(&big, "amount"), 999999999999.99);
        assert!(normalize(TRANSACTIONS, &json!({"kind":"expense","amount":"0.004","date":"2026-11-02","account":"a","category":"c"}), "USD").is_err(), "0.004 rounds to 0.00");
        assert!(normalize(TRANSACTIONS, &json!({"kind":"expense","amount":1e300,"date":"2026-11-02","account":"a","category":"c"}), "USD").unwrap_err().contains("too large"));
        assert!(normalize(TRANSACTIONS, &json!({"kind":"expense","amount":-5,"date":"2026-11-02","account":"a","category":"c"}), "USD").is_err());
    }

    #[test]
    fn loan_validation() {
        let base = json!({"direction":"lent","person":"Sam","amount":100,"given":"2026-10-01","due":"2026-12-01","repayments":[{"date":"2026-10-10","amount":30}]});
        let l = normalize(LOANS, &base, "USD").unwrap();
        assert_eq!(arr(&l, "repayments").len(), 1);
        assert!(!s(&arr(&l, "repayments")[0], "id").is_empty());
        let mut over = base.clone();
        over["repayments"] = json!([{"date":"2026-10-10","amount":130}]);
        assert!(normalize(LOANS, &over, "USD").is_err());
        let mut early = base.clone();
        early["due"] = json!("2026-09-01");
        assert!(normalize(LOANS, &early, "USD").is_err());
    }

    #[test]
    fn defaults_seed_once() {
        let st = Store::open_in_memory("hisab", "d", MIGRATIONS).unwrap();
        seed_defaults(&st, "USD").unwrap();
        seed_defaults(&st, "USD").unwrap();
        assert_eq!(st.count(ACCOUNTS).unwrap(), 1);
        assert_eq!(st.count(CATEGORIES).unwrap(), 11);
    }
}
