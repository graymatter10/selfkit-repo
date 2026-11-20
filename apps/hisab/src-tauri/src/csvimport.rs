//! CSV import (next to CSV export). Hisab's own export is read first-class
//! (type, currency, account, category, tags, transfers); any other CSV (bank
//! statements) goes through a column mapping that is guessed from the header
//! row and can be changed in the UI. Delimiter, quotes, decimal commas and
//! the date format are detected. Rows already in Hisab (same date + amount +
//! note + kind + account, each existing entry matching one row) are marked as
//! duplicates and skipped; repeats inside one file are kept (C22). Every row is validated with
//! the same `model::normalize` as a hand-made entry, so an import can't store
//! anything the add screen couldn't. New entries default to the main currency
//! and carry `source: "import"` and one `import_batch` id; the UI undoes the
//! whole import with `csv_import_undo` (all ids at once).
//!
//! Docs: docs/apps/hisab/csv-import.md.

use std::collections::HashMap;

use chrono::{NaiveDate, NaiveTime};
use selfkit_core::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{self, ACCOUNTS, CATEGORIES, R, TRANSACTIONS, f, round2, s};

/// Header of Hisab's own CSV export (export.rs). Matched as a prefix.
pub const HISAB_HEADER: &[&str] = &["date", "time", "type", "amount", "currency", "amount_main", "main_currency", "account", "to_account", "to_amount", "category", "tags", "note"];

#[derive(Debug, Clone, Serialize)]
pub struct Table {
    pub delimiter: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// The most frequent of `, ; TAB |` in the first non-empty line (outside quotes).
pub fn detect_delimiter(text: &str) -> char {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut best = (',', 0);
    for d in [',', ';', '\t', '|'] {
        let mut n = 0;
        let mut q = false;
        for c in line.chars() {
            if c == '"' {
                q = !q;
            } else if c == d && !q {
                n += 1;
            }
        }
        if n > best.1 {
            best = (d, n);
        }
    }
    best.0
}

/// RFC 4180 records: quoted fields, `""` escapes, CR/LF, line breaks inside quotes.
pub fn records(text: &str, delim: char) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut rec: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut q = false;
    let mut it = text.chars().peekable();
    let push_rec = |rec: &mut Vec<String>, field: &mut String, out: &mut Vec<Vec<String>>| {
        rec.push(std::mem::take(field));
        if rec.iter().any(|x| !x.trim().is_empty()) {
            out.push(std::mem::take(rec));
        } else {
            rec.clear();
        }
    };
    while let Some(c) = it.next() {
        if q {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    field.push('"');
                    it.next();
                } else {
                    q = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.trim().is_empty() {
            field.clear();
            q = true;
        } else if c == delim {
            rec.push(std::mem::take(&mut field));
        } else if c == '\r' || c == '\n' {
            if c == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            push_rec(&mut rec, &mut field, &mut out);
        } else {
            field.push(c);
        }
    }
    push_rec(&mut rec, &mut field, &mut out);
    out
}

/// Largest CSV accepted for import (a year of bank statements is well under 1 MB).
pub const MAX_CSV_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CSV_ROWS: usize = 100_000;

/// Undo export's formula guard (`export::neutralize_formula`): `'=…` → `=…`.
fn unescape_cell(x: String) -> String {
    match x.strip_prefix('\'') {
        Some(rest) if rest.starts_with(['=', '+', '-', '@', '\t', '\r']) => rest.to_string(),
        _ => x,
    }
}

pub fn parse(text: &str) -> R<Table> {
    if text.len() > MAX_CSV_BYTES {
        return Err("The file is too large to import (16 MB at most)".into());
    }
    let text = text.trim_start_matches('\u{feff}');
    let d = detect_delimiter(text);
    let mut recs = records(text, d);
    if recs.is_empty() {
        return Err("The file is empty".into());
    }
    if recs.len() > MAX_CSV_ROWS + 1 {
        return Err(format!("The file has too many rows to import ({MAX_CSV_ROWS} at most)"));
    }
    let headers: Vec<String> = recs.remove(0).into_iter().map(|h| h.trim().to_string()).collect();
    let n = headers.len();
    let rows = recs
        .into_iter()
        .map(|mut r| {
            r.resize(n.max(r.len()), String::new());
            r.into_iter().map(|x| unescape_cell(x.trim().to_string())).collect()
        })
        .collect();
    Ok(Table { delimiter: if d == '\t' { "tab".into() } else { d.to_string() }, headers, rows })
}

/// Which column holds what (column indexes). Guessed by `guess`, editable in the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Mapping {
    pub date: Option<usize>,
    pub time: Option<usize>,
    pub amount: Option<usize>,
    /// Bank statements with separate money-out / money-in columns.
    pub debit: Option<usize>,
    pub credit: Option<usize>,
    pub description: Option<usize>,
    pub category: Option<usize>,
    pub account: Option<usize>,
    pub currency: Option<usize>,
    /// expense / income / transfer (Hisab's export "type").
    pub kind: Option<usize>,
    pub tags: Option<usize>,
    pub to_account: Option<usize>,
    pub to_amount: Option<usize>,
    /// chrono format, e.g. "%d/%m/%Y".
    #[serde(default)]
    pub date_format: String,
    /// The file is Hisab's own export.
    #[serde(default)]
    pub hisab: bool,
    /// A single amount column with only positive numbers usually lists
    /// spending: then positive = expense (negative = refund / income).
    #[serde(default)]
    pub positive_is_expense: bool,
}

pub const DATE_FORMATS: &[&str] = &[
    "%Y-%m-%d", "%Y/%m/%d", "%d/%m/%Y", "%m/%d/%Y", "%d-%m-%Y", "%m-%d-%Y", "%d.%m.%Y", "%d/%m/%y", "%m/%d/%y", "%d-%m-%y", "%d %b %Y", "%d-%b-%Y", "%d-%b-%y", "%d %B %Y", "%b %d, %Y", "%B %d, %Y", "%Y%m%d",
];

/// Date (and a time after it, if any) in `fmt`; "2026-11-15 14:30" and "…T14:30:00" work too.
pub fn parse_date(v: &str, fmt: &str) -> Option<(NaiveDate, String)> {
    let v = v.trim();
    let (d, rest) = NaiveDate::parse_and_remainder(v, fmt).ok()?;
    let rest = rest.trim_start_matches(['T', ' ']).trim();
    if rest.is_empty() {
        return Some((d, String::new()));
    }
    let t = NaiveTime::parse_from_str(rest, "%H:%M:%S").or_else(|_| NaiveTime::parse_from_str(rest, "%H:%M")).ok()?;
    Some((d, t.format("%H:%M").to_string()))
}

/// The format that reads the most (non-empty) samples (one bad date doesn't
/// spoil the rest); on a tie the earlier one: day-first before month-first, so
/// 03/04/2026 is 3 April unless a value proves otherwise.
pub fn detect_date_format<'a>(values: impl Iterator<Item = &'a str>) -> String {
    let vals: Vec<&str> = values.filter(|v| !v.trim().is_empty()).take(300).collect();
    let mut best = (DATE_FORMATS[0], 0);
    for fm in DATE_FORMATS {
        let n = vals.iter().filter(|v| parse_date(v, fm).is_some()).count();
        if n > best.1 {
            best = (fm, n);
        }
    }
    best.0.to_string()
}

/// "1,234.50", "1.234,50", "(12.00)", "-12", "12.00-", "৳ 1,200", "12.00 DR".
pub fn parse_amount(v: &str) -> Option<f64> {
    let t = v.trim();
    if t.is_empty() {
        return None;
    }
    let low = t.to_ascii_lowercase();
    // A minus before the first digit ("-12", "৳ -850", "$-3"), brackets, a trailing minus or "DR".
    let before = t.split(|c: char| c.is_ascii_digit()).next().unwrap_or("");
    // "DR"/"CR" only as their own word at the end ("12.00 DR", "12.00DR"), not the end of
    // a currency code such as IDR (C25).
    let mark = low.split(|c: char| !c.is_ascii_alphabetic()).rfind(|w| !w.is_empty()).filter(|_| low.trim_end().ends_with(|c: char| c.is_ascii_alphabetic()));
    let mut neg = before.contains(['-', '−']) || (t.starts_with('(') && t.ends_with(')')) || t.ends_with('-') || mark == Some("dr");
    if mark == Some("cr") {
        neg = false;
    }
    let num: String = t.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    if !num.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let (dot, comma) = (num.rfind('.'), num.rfind(','));
    let normal = match (dot, comma) {
        (Some(d), Some(c)) if c > d => num.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => num.replace(',', ""),
        // Only commas: "12,50" is a decimal comma, "1,200" a thousands separator.
        (None, Some(c)) if num.len() - c - 1 <= 2 && num.matches(',').count() == 1 => num.replace(',', "."),
        (None, Some(_)) => num.replace(',', ""),
        _ => num,
    };
    let x: f64 = normal.parse().ok()?;
    x.is_finite().then_some(if neg { -x } else { x })
}

fn find(headers: &[String], names: &[&str]) -> Option<usize> {
    let low: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();
    names.iter().find_map(|n| low.iter().position(|h| h == n)).or_else(|| names.iter().find_map(|n| low.iter().position(|h| n.len() > 3 && h.contains(n))))
}

/// Guess the mapping from the header row (and the data, for the date format and sign).
pub fn guess(t: &Table) -> Mapping {
    let h = &t.headers;
    let low: Vec<String> = h.iter().map(|x| x.to_lowercase()).collect();
    let hisab = low.len() >= HISAB_HEADER.len() && HISAB_HEADER.iter().zip(&low).all(|(a, b)| a == b);
    let col = |n: &str| low.iter().position(|x| x == n);
    let mut m = if hisab {
        Mapping {
            date: col("date"),
            time: col("time"),
            amount: col("amount"),
            description: col("note"),
            category: col("category"),
            account: col("account"),
            currency: col("currency"),
            kind: col("type"),
            tags: col("tags"),
            to_account: col("to_account"),
            to_amount: col("to_amount"),
            hisab: true,
            ..Default::default()
        }
    } else {
        let debit = find(h, &["debit", "withdrawal", "withdrawals", "money out", "paid out", "debit amount"]);
        let credit = find(h, &["credit", "deposit", "deposits", "money in", "paid in", "credit amount"]);
        Mapping {
            date: find(h, &["date", "transaction date", "posting date", "posted", "value date", "booking date", "day"]),
            time: find(h, &["time"]),
            amount: find(h, &["amount", "value", "sum", "total", "amt"]).filter(|i| Some(*i) != debit && Some(*i) != credit),
            debit,
            credit,
            description: find(h, &["note", "description", "details", "narration", "memo", "payee", "merchant", "particulars", "remarks", "name", "reference"]),
            category: find(h, &["category"]),
            account: find(h, &["account"]),
            currency: find(h, &["currency", "ccy"]),
            kind: find(h, &["type", "kind"]).filter(|&i| t.rows.iter().all(|r| matches!(r.get(i).map(|x| x.to_lowercase()).as_deref(), Some("expense" | "income" | "transfer" | "")))),
            tags: find(h, &["tags"]),
            ..Default::default()
        }
    };
    if let Some(i) = m.date {
        m.date_format = detect_date_format(t.rows.iter().filter_map(|r| r.get(i).map(String::as_str)));
    }
    if !m.hisab && m.kind.is_none() && m.debit.is_none() && m.credit.is_none() {
        if let Some(i) = m.amount {
            m.positive_is_expense = !t.rows.iter().any(|r| r.get(i).and_then(|v| parse_amount(v)).is_some_and(|x| x < 0.0));
        }
    }
    m
}

/// One CSV row as it would be imported.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Row {
    pub line: usize,
    pub date: String,
    pub time: String,
    pub kind: String,
    pub amount: f64,
    pub currency: String,
    pub note: String,
    pub category: String,
    /// Category name in the file (shown when it didn't match one of yours).
    pub category_name: String,
    pub account: String,
    pub to_account: String,
    pub to_amount: f64,
    pub tags: Vec<String>,
    /// Split parts `[(category id, amount)]` read back from Hisab's own export, where a
    /// split is written as "Food 60.00 + Bills 40.00" in the category column (L2-15).
    #[serde(default)]
    pub splits: Vec<(String, f64)>,
    pub duplicate: bool,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub rows: Vec<Row>,
    pub ok: usize,
    pub duplicates: usize,
    pub errors: usize,
}

/// Duplicate key: date + amount in cents + note (trimmed, lower-case) + kind + account.
pub type DupKey = (String, i64, String, String, String);
pub fn dup_key(date: &str, amount: f64, note: &str, kind: &str, account: &str) -> DupKey {
    (date.to_string(), (round2(amount.abs()) * 100.0).round() as i64, note.trim().to_lowercase(), kind.to_string(), account.to_string())
}

fn data_of(r: &Row) -> Value {
    let mut v = json!({
        "kind": r.kind, "amount": r.amount, "currency": r.currency, "date": r.date, "time": r.time,
        "account": r.account, "category": r.category, "note": r.note, "tags": r.tags, "source": "import",
    });
    if r.kind == "transfer" {
        v["to_account"] = json!(r.to_account);
        v["to_amount"] = json!(r.to_amount);
    }
    if r.splits.len() > 1 {
        v["splits"] = json!(r.splits.iter().map(|(c, a)| json!({"category": c, "amount": a})).collect::<Vec<_>>());
    }
    v
}

/// Turn the table into rows to import into `account` (used when the file has
/// no account column or names an unknown account). Nothing is written.
pub fn preview(store: &Store, t: &Table, m: &Mapping, account: &str, main: &str) -> R<Preview> {
    let accounts = model::all(store, ACCOUNTS)?;
    let cats = model::all(store, CATEGORIES)?;
    let acct_by_name: HashMap<String, String> = accounts.iter().map(|a| (s(a, "name").to_lowercase(), s(a, "id").to_string())).collect();
    let cat_by_name: HashMap<(String, String), String> = cats.iter().filter(|c| !model::b(c, "archived")).map(|c| ((s(c, "kind").to_string(), s(c, "name").to_lowercase()), s(c, "id").to_string())).collect();
    // Unknown category → your "Other" (or the first one) of that kind.
    let fallback = |kind: &str| -> (String, String) {
        let mut list: Vec<&Value> = cats.iter().filter(|c| s(c, "kind") == kind && !model::b(c, "archived")).collect();
        list.sort_by(|a, b| f(a, "order").total_cmp(&f(b, "order")));
        list.iter().find(|c| s(c, "name").eq_ignore_ascii_case("other")).or(list.first()).map(|c| (s(c, "id").to_string(), s(c, "name").to_string())).unwrap_or_default()
    };
    // Only entries already in Hisab count as duplicates, each one matching one row: two
    // equal ATM withdrawals on one day in the same file are both real (C22), and
    // importing that file again marks both as duplicates.
    let mut seen: HashMap<DupKey, usize> = HashMap::new();
    for x in model::all(store, TRANSACTIONS)? {
        *seen.entry(dup_key(s(&x, "date"), f(&x, "amount"), s(&x, "note"), s(&x, "kind"), s(&x, "account"))).or_default() += 1;
    }
    let cell = |r: &Vec<String>, i: Option<usize>| i.and_then(|i| r.get(i)).map(|x| x.trim().to_string()).unwrap_or_default();
    let mut out = Preview { rows: vec![], ok: 0, duplicates: 0, errors: 0 };
    for (n, r) in t.rows.iter().enumerate() {
        let mut row = Row { line: n + 2, ..Default::default() };
        let err = |row: &mut Row, e: String| {
            if row.error.is_empty() {
                row.error = e;
            }
        };
        // Date (and time).
        let dv = cell(r, m.date);
        match parse_date(&dv, &m.date_format) {
            Some((d, tm)) => {
                row.date = d.format("%Y-%m-%d").to_string();
                row.time = tm;
            }
            None => err(&mut row, if dv.is_empty() { "No date".into() } else { format!("Date “{dv}” not understood") }),
        }
        let tv = cell(r, m.time);
        if !tv.is_empty() {
            if let Ok(x) = NaiveTime::parse_from_str(&tv, "%H:%M").or_else(|_| NaiveTime::parse_from_str(&tv, "%H:%M:%S")) {
                row.time = x.format("%H:%M").to_string();
            }
        }
        // Amount and kind.
        let kind_cell = cell(r, m.kind).to_lowercase();
        let (debit, credit) = (parse_amount(&cell(r, m.debit)).unwrap_or(0.0).abs(), parse_amount(&cell(r, m.credit)).unwrap_or(0.0).abs());
        let signed = if m.debit.is_some() || m.credit.is_some() {
            if debit > 0.0 { Some(-debit) } else if credit > 0.0 { Some(credit) } else { parse_amount(&cell(r, m.amount)) }
        } else {
            parse_amount(&cell(r, m.amount))
        };
        match signed {
            Some(x) if round2(x.abs()) > 0.0 => {
                row.amount = round2(x.abs());
                row.kind = if matches!(kind_cell.as_str(), "expense" | "income" | "transfer") {
                    kind_cell.clone()
                } else if m.positive_is_expense && m.debit.is_none() && m.credit.is_none() {
                    (if x > 0.0 { "expense" } else { "income" }).to_string()
                } else {
                    (if x < 0.0 { "expense" } else { "income" }).to_string()
                };
            }
            Some(_) => err(&mut row, "Amount is 0".into()),
            None => {
                let raw = if m.amount.is_some() { cell(r, m.amount) } else { String::new() };
                err(&mut row, if raw.is_empty() { "No amount".into() } else { format!("Amount “{raw}” not understood") })
            }
        }
        row.note = cell(r, m.description);
        // Currency: the file's, else the main currency.
        let cv = cell(r, m.currency).to_ascii_uppercase();
        row.currency = if cv.len() == 3 && cv.chars().all(|c| c.is_ascii_alphabetic()) { cv } else { main.to_string() };
        // Account by name, else the chosen one.
        row.account = acct_by_name.get(&cell(r, m.account).to_lowercase()).cloned().unwrap_or_else(|| account.to_string());
        if row.kind == "transfer" {
            row.to_account = acct_by_name.get(&cell(r, m.to_account).to_lowercase()).cloned().unwrap_or_default();
            row.to_amount = parse_amount(&cell(r, m.to_amount)).map(f64::abs).unwrap_or(row.amount);
            if row.to_account.is_empty() {
                err(&mut row, format!("Transfer to “{}”: no such account", cell(r, m.to_account)));
            }
        } else if !row.kind.is_empty() {
            let name = cell(r, m.category);
            row.category_name = name.clone();
            // "Food 60.00 + Bills 40.00" (Hisab export of a split): read the parts back.
            if row.kind == "expense" && name.contains(" + ") {
                let parts: Option<Vec<(String, f64)>> = name
                    .split(" + ")
                    .map(|p| {
                        let (n, a) = p.trim().rsplit_once(' ')?;
                        Some((cat_by_name.get(&(row.kind.clone(), n.trim().to_lowercase()))?.clone(), a.trim().parse::<f64>().ok()?))
                    })
                    .collect();
                if let Some(parts) = parts.filter(|p| p.len() > 1 && (p.iter().map(|x| x.1).sum::<f64>() - row.amount).abs() < 0.01) {
                    row.category = parts[0].0.clone();
                    row.splits = parts;
                }
            }
            if row.splits.is_empty() { match cat_by_name.get(&(row.kind.clone(), name.to_lowercase())) {
                Some(id) => row.category = id.clone(),
                None => {
                    let (id, nm) = fallback(&row.kind);
                    row.category = id;
                    if name.is_empty() {
                        row.category_name = nm;
                    }
                }
            } }
        }
        row.tags = cell(r, m.tags).split([' ', ',']).map(|x| x.trim_start_matches('#').to_lowercase()).filter(|x| !x.is_empty()).collect();
        // Same validation as a hand-made entry.
        if row.error.is_empty() {
            if let Err(e) = model::normalize(TRANSACTIONS, &data_of(&row), main) {
                row.error = e;
            }
        }
        if row.error.is_empty() {
            if let Some(n) = seen.get_mut(&dup_key(&row.date, row.amount, &row.note, &row.kind, &row.account))
                && *n > 0
            {
                *n -= 1;
                row.duplicate = true;
            }
        }
        if !row.error.is_empty() {
            out.errors += 1;
        } else if row.duplicate {
            out.duplicates += 1;
        } else {
            out.ok += 1;
        }
        out.rows.push(row);
    }
    Ok(out)
}

/// Store the rows that are neither errors nor duplicates; returns their ids
/// (the UI's Undo removes exactly these).
pub fn import(store: &Store, rows: &[Row], main: &str, batch: &str) -> R<Vec<String>> {
    let now = selfkit_core::time::now_ms();
    let mut ids = Vec::new();
    for (i, r) in rows.iter().filter(|r| r.error.is_empty() && !r.duplicate).enumerate() {
        let mut d = data_of(r);
        d["import_batch"] = json!(batch);
        d["created_at"] = json!(now + i as i64);
        let Ok(v) = model::normalize(TRANSACTIONS, &d, main) else { continue };
        ids.push(store.put(TRANSACTIONS, None, &v).map_err(|e| e.to_string())?.id);
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::seed_defaults;

    #[test]
    fn formula_guard_roundtrip_and_size_cap() {
        let t = parse("date,note\n2026-01-01,'=SUM(A1)\n2026-01-02,'tis fine\n").unwrap();
        assert_eq!(t.rows[0][1], "=SUM(A1)");
        assert_eq!(t.rows[1][1], "'tis fine");
        assert!(parse(&"x".repeat(MAX_CSV_BYTES + 1)).is_err());
    }

    fn store() -> Store {
        let st = Store::open_in_memory("hisab", "d", model::MIGRATIONS).unwrap();
        seed_defaults(&st, "BDT").unwrap();
        st
    }
    fn cash(st: &Store) -> String {
        model::all(st, ACCOUNTS).unwrap().into_iter().find(|a| s(a, "name") == "Cash").map(|a| s(&a, "id").to_string()).unwrap()
    }

    #[test]
    fn records_quotes_and_delimiters() {
        let t = "a;b;c\r\n1;\"x;y\";\"say \"\"hi\"\"\"\n\n2;\"two\nlines\";3";
        assert_eq!(detect_delimiter(t), ';');
        let r = records(t, ';');
        assert_eq!(r.len(), 3, "blank line skipped");
        assert_eq!(r[1], vec!["1", "x;y", "say \"hi\""]);
        assert_eq!(r[2][1], "two\nlines");
        assert_eq!(detect_delimiter("a\tb\tc"), '\t');
        assert_eq!(parse("\u{feff}date,amount\n2026-11-01,5").unwrap().headers[0], "date");
    }

    #[test]
    fn amounts_and_dates() {
        assert_eq!(parse_amount("1,234.50"), Some(1234.5));
        assert_eq!(parse_amount("1.234,50"), Some(1234.5));
        assert_eq!(parse_amount("12,5"), Some(12.5));
        assert_eq!(parse_amount("1,200"), Some(1200.0));
        assert_eq!(parse_amount("(12.00)"), Some(-12.0));
        assert_eq!(parse_amount("৳ -850"), Some(-850.0));
        assert_eq!(parse_amount("30.00 DR"), Some(-30.0));
        assert_eq!(parse_amount("30.00DR"), Some(-30.0));
        assert_eq!(parse_amount("150000 IDR"), Some(150000.0));
        assert_eq!(parse_amount("-75 IDR"), Some(-75.0));
        assert_eq!(parse_amount("abc"), None);
        assert_eq!(detect_date_format(["03/04/2026", "15/04/2026"].into_iter()), "%d/%m/%Y");
        assert_eq!(detect_date_format(["04/15/2026", "04/03/2026"].into_iter()), "%m/%d/%Y");
        assert_eq!(detect_date_format(["15 Nov 2026"].into_iter()), "%d %b %Y");
        assert_eq!(parse_date("2026-11-15T14:30:00", "%Y-%m-%d").unwrap().1, "14:30");
    }

    /// Hisab's own export round-trips: kinds, categories, accounts, transfers, tags.
    #[test]
    fn hisab_export_round_trip() {
        let st = store();
        let csv = "date,time,type,amount,currency,amount_main,main_currency,account,to_account,to_amount,category,tags,note,source\n\
            2026-11-01,09:10,expense,120.00,BDT,120.00,BDT,Cash,,,Food,lunch work,\"Rice, dal\",manual\n\
            2026-11-02,,income,5000.00,BDT,5000.00,BDT,Cash,,,Salary,,Pay,manual\n\
            2026-11-03,,expense,3.00,USD,360.00,BDT,Cash,,,Mystery,,Coffee,manual\n";
        let t = parse(csv).unwrap();
        let m = guess(&t);
        assert!(m.hisab);
        let p = preview(&st, &t, &m, &cash(&st), "BDT").unwrap();
        assert_eq!((p.ok, p.duplicates, p.errors), (3, 0, 0), "{:?}", p.rows);
        let r = &p.rows[0];
        assert_eq!((r.kind.as_str(), r.amount, r.time.as_str(), r.note.as_str()), ("expense", 120.0, "09:10", "Rice, dal"));
        assert_eq!(r.tags, vec!["lunch", "work"]);
        assert_eq!(p.rows[1].kind, "income");
        assert_eq!(p.rows[2].currency, "USD", "the file's currency is kept");
        assert_eq!(p.rows[2].category_name, "Mystery", "unknown category → Other");
        let ids = import(&st, &p.rows, "BDT", "b1").unwrap();
        assert_eq!(ids.len(), 3);
        let saved = model::get(&st, &ids[0]).unwrap().unwrap();
        assert_eq!((s(&saved, "source"), s(&saved, "import_batch")), ("import", "b1"));
        // Importing the same file again: everything is a duplicate.
        let p2 = preview(&st, &t, &m, &cash(&st), "BDT").unwrap();
        assert_eq!((p2.ok, p2.duplicates), (0, 3));
        // Undo = delete those ids.
        for id in &ids {
            st.delete(id).unwrap();
        }
        assert_eq!(preview(&st, &t, &m, &cash(&st), "BDT").unwrap().ok, 3);
    }

    #[test]
    fn split_entry_survives_export_and_import() {
        // L2-15: a split is exported as "Food 60.00 + Bills 40.00" in the category column.
        let st = store();
        let csv = "date,time,type,amount,currency,amount_main,main_currency,account,to_account,to_amount,category,tags,note,source\n\
            2026-11-04,,expense,100.00,BDT,100.00,BDT,Cash,,,Food 60.00 + Bills 40.00,,Groceries and power,manual\n";
        let t = parse(csv).unwrap();
        let m = guess(&t);
        let p = preview(&st, &t, &m, &cash(&st), "BDT").unwrap();
        assert_eq!(p.ok, 1, "{:?}", p.rows);
        assert_eq!(p.rows[0].splits.len(), 2);
        let ids = import(&st, &p.rows, "BDT", "b9").unwrap();
        let saved = model::get(&st, &ids[0]).unwrap().unwrap();
        assert_eq!(saved["splits"].as_array().map(Vec::len), Some(2));
    }

    /// A bank statement: ';' separated, day-first dates, debit/credit columns,
    /// no currency (→ main), duplicates inside the file and errors reported.
    #[test]
    fn bank_csv_mapping_and_dedupe() {
        let st = store();
        let csv = "Posting Date;Narration;Debit;Credit;Balance\n\
            14/11/2026;AGORA GULSHAN;1.200,50;;10.000\n\
            14/11/2026;AGORA GULSHAN;1.200,50;;8.799\n\
            15/11/2026;Salary;;50.000,00;58.799\n\
            31/02/2026;Bad date;10;;\n\
            16/11/2026;Nothing;;;\n";
        let t = parse(csv).unwrap();
        let m = guess(&t);
        assert_eq!((m.date, m.description, m.debit, m.credit), (Some(0), Some(1), Some(2), Some(3)));
        assert_eq!(m.date_format, "%d/%m/%Y");
        let p = preview(&st, &t, &m, &cash(&st), "BDT").unwrap();
        // Two equal rows in one file are both real (C22).
        assert_eq!((p.ok, p.duplicates, p.errors), (3, 0, 2), "{:?}", p.rows);
        assert_eq!((p.rows[0].kind.as_str(), p.rows[0].amount, p.rows[0].currency.as_str()), ("expense", 1200.5, "BDT"));
        assert!(!p.rows[1].duplicate);
        assert_eq!((p.rows[2].kind.as_str(), p.rows[2].amount), ("income", 50000.0));
        assert!(p.rows[3].error.contains("not understood"));
        assert_eq!(p.rows[4].error, "No amount");
        assert_eq!(import(&st, &p.rows, "BDT", "b2").unwrap().len(), 3);
        // The same file again: every row is already there, once each.
        let p = preview(&st, &t, &m, &cash(&st), "BDT").unwrap();
        assert_eq!((p.ok, p.duplicates, p.errors), (0, 3, 2), "{:?}", p.rows);
    }

    /// One amount column with only positive numbers = a spending list.
    #[test]
    fn positive_only_amounts_are_expenses() {
        let st = store();
        let t = parse("Date,Description,Amount\n2026-11-01,Bus,40\n2026-11-02,Tea,15").unwrap();
        let m = guess(&t);
        assert!(m.positive_is_expense);
        let p = preview(&st, &t, &m, &cash(&st), "BDT").unwrap();
        assert!(p.rows.iter().all(|r| r.kind == "expense" && r.error.is_empty()));
        let t = parse("Date,Description,Amount\n2026-11-01,Bus,-40\n2026-11-02,Refund,15").unwrap();
        let m = guess(&t);
        assert!(!m.positive_is_expense);
        let p = preview(&st, &t, &m, &cash(&st), "BDT").unwrap();
        assert_eq!((p.rows[0].kind.as_str(), p.rows[1].kind.as_str()), ("expense", "income"));
    }
}
