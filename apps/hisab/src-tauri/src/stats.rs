//! Computed views: balances, category usage and budgets, loan status, goals,
//! overview for a month, trends (daily, by category, by year, lifetime,
//! heatmap, net worth) and the transaction search/filter.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Duration, NaiveDate};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::fx::Fx;
use crate::model::{self, ACCOUNTS, CATEGORIES, GOALS, LOANS, R, RECURRING, TRANSACTIONS, arr, b, date, f, parts, round2, s};
use selfkit_core::Store;

/// Everything loaded once per call.
pub struct Data {
    pub main: String,
    pub fx: Fx,
    pub accounts: Vec<Value>,
    pub categories: Vec<Value>,
    pub txs: Vec<Value>,
    pub loans: Vec<Value>,
    pub goals: Vec<Value>,
    pub recurring: Vec<Value>,
}

impl Data {
    pub fn load(store: &Store, main: &str) -> R<Data> {
        Ok(Data {
            main: main.to_string(),
            fx: Fx::load(store)?,
            accounts: model::all(store, ACCOUNTS)?,
            categories: model::all(store, CATEGORIES)?,
            txs: model::all(store, TRANSACTIONS)?,
            loans: model::all(store, LOANS)?,
            goals: model::all(store, GOALS)?,
            recurring: model::all(store, RECURRING)?,
        })
    }

    /// Amount of a transaction-like value in the main currency.
    pub fn main_of(&self, amount: f64, currency: &str) -> f64 {
        self.fx.conv(amount, currency, &self.main)
    }

    fn acct_currency(&self, id: &str) -> String {
        self.accounts.iter().find(|a| s(a, "id") == id).map(|a| s(a, "currency").to_string()).unwrap_or_else(|| self.main.clone())
    }

    pub fn missing(&self) -> Vec<String> {
        self.fx.missing.borrow().iter().cloned().collect()
    }
}

pub fn month_of(d: NaiveDate) -> String {
    d.format("%Y-%m").to_string()
}

pub fn month_bounds(month: &str) -> Option<(NaiveDate, NaiveDate)> {
    let first = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").ok()?;
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)?
    };
    Some((first, next - Duration::days(1)))
}

fn prev_month(month: &str) -> String {
    let (first, _) = month_bounds(month).unwrap_or((NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(), NaiveDate::MIN));
    month_of(first - Duration::days(1))
}

// ---- balances -------------------------------------------------------------

/// Per-account balance deltas (account currency) of everything dated ≤ `until`.
fn balances(d: &Data, until: Option<NaiveDate>) -> HashMap<String, f64> {
    let mut bal: HashMap<String, f64> = HashMap::new();
    for a in &d.accounts {
        bal.insert(s(a, "id").to_string(), f(a, "opening"));
    }
    let within = |v: &Value, k: &str| until.is_none_or(|u| date(v, k).is_some_and(|x| x <= u));
    for t in &d.txs {
        if !within(t, "date") {
            continue;
        }
        let cur = s(t, "currency");
        let acct = s(t, "account");
        let into = |id: &str, amt: f64, from_cur: &str, bal: &mut HashMap<String, f64>| {
            let c = d.acct_currency(id);
            *bal.entry(id.to_string()).or_default() += d.fx.conv(amt, from_cur, &c);
        };
        match s(t, "kind") {
            "income" => into(acct, f(t, "amount"), cur, &mut bal),
            "expense" => into(acct, -f(t, "amount"), cur, &mut bal),
            "transfer" => {
                into(acct, -f(t, "amount"), cur, &mut bal);
                let to = s(t, "to_account");
                let tc = d.acct_currency(to);
                *bal.entry(to.to_string()).or_default() += f(t, "to_amount");
                let _ = tc;
            }
            _ => {}
        }
    }
    for l in &d.loans {
        let acct = s(l, "account");
        if acct.is_empty() || !within(l, "given") {
            continue;
        }
        let sign = if s(l, "direction") == "lent" { -1.0 } else { 1.0 };
        let c = d.acct_currency(acct);
        let mut delta = sign * f(l, "amount");
        for r in arr(l, "repayments") {
            if within(r, "date") {
                delta -= sign * f(r, "amount");
            }
        }
        *bal.entry(acct.to_string()).or_default() += d.fx.conv(delta, s(l, "currency"), &c);
    }
    bal
}

pub fn accounts_view(d: &Data) -> Vec<Value> {
    let bal = balances(d, None);
    let mut out: Vec<Value> = d
        .accounts
        .iter()
        .map(|a| {
            let mut v = a.clone();
            let x = round2(*bal.get(s(a, "id")).unwrap_or(&0.0));
            v["balance"] = json!(x);
            v["balance_main"] = json!(round2(d.main_of(x, s(a, "currency"))));
            v
        })
        .collect();
    out.sort_by(|a, b| f(a, "order").total_cmp(&f(b, "order")).then_with(|| s(a, "name").cmp(s(b, "name"))));
    out
}

/// Loan remaining amount as of `until` (None = now).
fn loan_remaining(l: &Value, until: Option<NaiveDate>) -> f64 {
    let repaid: f64 = arr(l, "repayments")
        .iter()
        .filter(|r| until.is_none_or(|u| date(r, "date").is_some_and(|x| x <= u)))
        .map(|r| f(r, "amount"))
        .sum();
    (f(l, "amount") - repaid).max(0.0)
}

/// Net worth in the main currency: accounts + money lent − money borrowed.
fn net_worth_at(d: &Data, until: Option<NaiveDate>) -> f64 {
    let bal = balances(d, until);
    let mut total = 0.0;
    for a in &d.accounts {
        total += d.main_of(*bal.get(s(a, "id")).unwrap_or(&0.0), s(a, "currency"));
    }
    for l in &d.loans {
        if until.is_some_and(|u| date(l, "given").is_none_or(|g| g > u)) {
            continue;
        }
        let rem = d.main_of(loan_remaining(l, until), s(l, "currency"));
        total += if s(l, "direction") == "lent" { rem } else { -rem };
    }
    total
}

// ---- categories -----------------------------------------------------------

pub fn categories_view(d: &Data, month: &str) -> Vec<Value> {
    let mut usage: HashMap<String, i64> = HashMap::new();
    let mut spent: HashMap<String, f64> = HashMap::new();
    for t in &d.txs {
        if s(t, "kind") == "transfer" {
            continue;
        }
        let in_month = s(t, "date").starts_with(month);
        for (c, a) in parts(t) {
            *usage.entry(c.clone()).or_default() += 1;
            if in_month {
                *spent.entry(c).or_default() += d.main_of(a, s(t, "currency"));
            }
        }
    }
    let mut out: Vec<Value> = d
        .categories
        .iter()
        .map(|c| {
            let id = s(c, "id");
            let mut v = c.clone();
            v["usage"] = json!(usage.get(id).copied().unwrap_or(0));
            v["spent"] = json!(round2(spent.get(id).copied().unwrap_or(0.0)));
            v
        })
        .collect();
    out.sort_by(|a, b| f(a, "order").total_cmp(&f(b, "order")).then_with(|| s(a, "name").cmp(s(b, "name"))));
    out
}

// ---- loans, goals, recurring ------------------------------------------------

pub fn loan_status(l: &Value, today: NaiveDate) -> (&'static str, Option<i64>) {
    if loan_remaining(l, None) <= 0.005 {
        return ("finished", None);
    }
    match date(l, "due") {
        Some(due) => {
            let left = (due - today).num_days();
            if left < 0 {
                ("overdue", Some(left))
            } else if left <= 7 {
                ("due_soon", Some(left))
            } else {
                ("ongoing", Some(left))
            }
        }
        None => ("ongoing", None),
    }
}

pub fn loans_view(d: &Data, today: NaiveDate) -> Vec<Value> {
    let mut out: Vec<Value> = d
        .loans
        .iter()
        .map(|l| {
            let mut v = l.clone();
            let rem = loan_remaining(l, None);
            let (st, left) = loan_status(l, today);
            v["repaid"] = json!(round2(f(l, "amount") - rem));
            v["remaining"] = json!(round2(rem));
            v["remaining_main"] = json!(round2(d.main_of(rem, s(l, "currency"))));
            v["status"] = json!(st);
            v["days_left"] = json!(left);
            v
        })
        .collect();
    let rank = |v: &Value| match s(v, "status") {
        "overdue" => 0,
        "due_soon" => 1,
        "ongoing" => 2,
        _ => 3,
    };
    out.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| {
        let da = if s(a, "due").is_empty() { "9999" } else { s(a, "due") };
        let db = if s(b, "due").is_empty() { "9999" } else { s(b, "due") };
        da.cmp(db)
    }));
    out
}

pub fn goals_view(d: &Data) -> Vec<Value> {
    d.goals
        .iter()
        .map(|g| {
            let mut v = g.clone();
            let saved: f64 = arr(g, "contributions").iter().map(|c| f(c, "amount")).sum();
            v["saved"] = json!(round2(saved));
            v
        })
        .collect()
}

/// Rough monthly cost of a recurring item, in the main currency.
pub fn monthly_equiv(d: &Data, r: &Value) -> f64 {
    let t = &r["template"];
    let a = d.main_of(f(t, "amount"), s(t, "currency"));
    let n = f(r, "interval").max(1.0);
    let per_month = match s(r, "freq") {
        "daily" => 30.44 / n,
        "weekly" => 4.345 / n,
        "yearly" => 1.0 / (12.0 * n),
        _ => 1.0 / n,
    };
    a * per_month
}

pub fn recurring_view(d: &Data) -> Vec<Value> {
    let mut out: Vec<Value> = d
        .recurring
        .iter()
        .map(|r| {
            let mut v = r.clone();
            v["monthly"] = json!(round2(monthly_equiv(d, r)));
            v
        })
        .collect();
    out.sort_by(|a, b| s(a, "next").cmp(s(b, "next")));
    out
}

// ---- overview + trends ------------------------------------------------------

#[derive(Default)]
struct Sums {
    spent: f64,
    income: f64,
}

fn sums_where(d: &Data, pred: impl Fn(&str) -> bool) -> Sums {
    let mut x = Sums::default();
    for t in &d.txs {
        if !pred(s(t, "date")) {
            continue;
        }
        let a = d.main_of(f(t, "amount"), s(t, "currency"));
        match s(t, "kind") {
            "expense" => x.spent += a,
            "income" => x.income += a,
            _ => {}
        }
    }
    x
}

fn by_category(d: &Data, kind: &str, pred: impl Fn(&str) -> bool) -> Vec<Value> {
    let mut m: HashMap<String, f64> = HashMap::new();
    for t in d.txs.iter().filter(|t| s(t, "kind") == kind && pred(s(t, "date"))) {
        for (c, a) in parts(t) {
            *m.entry(c).or_default() += d.main_of(a, s(t, "currency"));
        }
    }
    let mut out: Vec<Value> = m
        .into_iter()
        .map(|(id, amt)| {
            let c = d.categories.iter().find(|c| s(c, "id") == id);
            json!({
                "id": id,
                "name": c.map(|c| s(c, "name")).unwrap_or("Unknown"),
                "color": c.map(|c| s(c, "color")).unwrap_or("#8a8a8e"),
                "icon": c.map(|c| s(c, "icon")).unwrap_or("dot"),
                "archived": c.map(|c| b(c, "archived")).unwrap_or(false),
                "amount": round2(amt),
            })
        })
        .collect();
    out.sort_by(|a, b| f(b, "amount").total_cmp(&f(a, "amount")));
    out
}

fn daily(d: &Data, month: &str) -> Vec<f64> {
    let Some((first, last)) = month_bounds(month) else { return vec![] };
    let mut v = vec![0.0; last.day() as usize];
    for t in d.txs.iter().filter(|t| s(t, "kind") == "expense") {
        if let Some(dt) = date(t, "date").filter(|x| *x >= first && *x <= last) {
            v[dt.day0() as usize] += d.main_of(f(t, "amount"), s(t, "currency"));
        }
    }
    v.into_iter().map(round2).collect()
}

pub fn overview(d: &Data, month: &str, today: NaiveDate) -> Value {
    let cur = sums_where(d, |x| x.starts_with(month));
    let pm = prev_month(month);
    let prev = sums_where(d, |x| x.starts_with(&pm));
    let budget: f64 = d
        .categories
        .iter()
        .filter(|c| s(c, "kind") == "expense" && !b(c, "archived"))
        .map(|c| f(c, "budget"))
        .sum();
    let (_, last) = month_bounds(month).unwrap_or((today, today));
    let elapsed = if month_of(today) == month { today.day() } else if month < month_of(today).as_str() { last.day() } else { 0 };
    json!({
        "month": month,
        "main": d.main,
        "spent": round2(cur.spent),
        "income": round2(cur.income),
        "balance": round2(cur.income - cur.spent),
        "budget": round2(budget),
        "remaining": if budget > 0.0 { json!(round2(budget - cur.spent)) } else { Value::Null },
        "prev_spent": round2(prev.spent),
        "days_in_month": last.day(),
        "days_elapsed": elapsed,
        "net_worth": round2(net_worth_at(d, None)),
        "by_category": by_category(d, "expense", |x| x.starts_with(month)),
        "missing_rates": d.missing(),
    })
}

pub fn trends(d: &Data, month: &str, today: NaiveDate) -> Value {
    let year = &month[..4.min(month.len())];
    let mut months = Vec::new();
    for m in 1..=12 {
        let key = format!("{year}-{m:02}");
        let x = sums_where(d, |dt| dt.starts_with(&key));
        months.push(json!({"month": key, "spent": round2(x.spent), "income": round2(x.income)}));
    }
    let mut years: BTreeMap<String, Sums> = BTreeMap::new();
    let mut first: Option<String> = None;
    let mut heat: BTreeMap<String, f64> = BTreeMap::new();
    for t in &d.txs {
        let dt = s(t, "date");
        if dt.len() < 10 {
            continue;
        }
        if first.as_deref().is_none_or(|f0| dt < f0) {
            first = Some(dt.to_string());
        }
        let a = d.main_of(f(t, "amount"), s(t, "currency"));
        let y = years.entry(dt[..4].to_string()).or_default();
        match s(t, "kind") {
            "expense" => {
                y.spent += a;
                if dt.starts_with(year) {
                    *heat.entry(dt.to_string()).or_default() += a;
                }
            }
            "income" => y.income += a,
            _ => {}
        }
    }
    for l in &d.loans {
        let g = s(l, "given");
        if g.len() == 10 && first.as_deref().is_none_or(|f0| g < f0) {
            first = Some(g.to_string());
        }
    }
    let lifetime = years.values().fold((0.0, 0.0), |acc, y| (acc.0 + y.spent, acc.1 + y.income));
    // Net worth at each month end from the first record to this month (max 10 years).
    let mut nw = Vec::new();
    if let Some(f0) = first.as_deref().and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()) {
        let mut m = month_of(f0);
        let end = month_of(today);
        let mut guard = 0;
        while m <= end && guard < 120 {
            let (_, last) = month_bounds(&m).unwrap();
            let at = if last > today { today } else { last };
            nw.push(json!({"month": m, "value": round2(net_worth_at(d, Some(at)))}));
            m = month_of(last + Duration::days(1));
            guard += 1;
        }
    }
    json!({
        "month": month,
        "main": d.main,
        "daily": daily(d, month),
        "daily_prev": daily(d, &prev_month(month)),
        "by_category_month": by_category(d, "expense", |x| x.starts_with(month)),
        "income_by_category_month": by_category(d, "income", |x| x.starts_with(month)),
        "by_category_year": by_category(d, "expense", |x| x.starts_with(year)),
        "months": months,
        "years": years.iter().map(|(y, x)| json!({"year": y, "spent": round2(x.spent), "income": round2(x.income)})).collect::<Vec<_>>(),
        "heatmap": heat.iter().map(|(k, v)| json!({"date": k, "amount": round2(*v)})).collect::<Vec<_>>(),
        "networth": nw,
        "lifetime": {"spent": round2(lifetime.0), "income": round2(lifetime.1), "count": d.txs.len(), "first": first},
        "missing_rates": d.missing(),
    })
}

// ---- search / filter --------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub q: String,
    pub kind: String,
    pub category: String,
    pub account: String,
    pub tag: String,
    pub from: String,
    pub to: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub limit: Option<usize>,
}

pub fn filter_tx(d: &Data, flt: &Filter) -> Vec<Value> {
    let q = flt.q.trim().to_lowercase();
    let name_of = |list: &[Value], id: &str| list.iter().find(|x| s(x, "id") == id).map(|x| s(x, "name").to_lowercase()).unwrap_or_default();
    let mut out: Vec<Value> = d
        .txs
        .iter()
        .filter(|t| {
            let dt = s(t, "date");
            if !flt.kind.is_empty() && s(t, "kind") != flt.kind {
                return false;
            }
            if !flt.account.is_empty() && s(t, "account") != flt.account && s(t, "to_account") != flt.account {
                return false;
            }
            if !flt.category.is_empty() && !parts(t).iter().any(|(c, _)| *c == flt.category) {
                return false;
            }
            if !flt.tag.is_empty() && !arr(t, "tags").iter().any(|x| x.as_str() == Some(flt.tag.as_str())) {
                return false;
            }
            if !flt.from.is_empty() && dt < flt.from.as_str() {
                return false;
            }
            if !flt.to.is_empty() && dt > flt.to.as_str() {
                return false;
            }
            let amt = f(t, "amount");
            if flt.min.is_some_and(|m| amt < m) || flt.max.is_some_and(|m| amt > m) {
                return false;
            }
            if q.is_empty() {
                return true;
            }
            let mut hay = format!("{} {} {} ", s(t, "note").to_lowercase(), dt, amt);
            for x in arr(t, "tags") {
                hay.push_str(x.as_str().unwrap_or(""));
                hay.push(' ');
            }
            for (c, _) in parts(t) {
                hay.push_str(&name_of(&d.categories, &c));
                hay.push(' ');
            }
            hay.push_str(&name_of(&d.accounts, s(t, "account")));
            hay.push(' ');
            hay.push_str(&name_of(&d.accounts, s(t, "to_account")));
            q.split_whitespace().all(|w| hay.contains(w.trim_start_matches('#')))
        })
        .map(|t| {
            let mut v = t.clone();
            v["amount_main"] = json!(round2(d.main_of(f(t, "amount"), s(t, "currency"))));
            v
        })
        .collect();
    out.sort_by(|a, b| s(b, "date").cmp(s(a, "date")).then_with(|| s(b, "time").cmp(s(a, "time"))).then_with(|| s(b, "id").cmp(s(a, "id"))));
    if let Some(l) = flt.limit {
        out.truncate(l);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TRANSACTIONS, normalize};

    fn data() -> (Store, NaiveDate) {
        let st = Store::open_in_memory("hisab", "d", &[]).unwrap();
        let put = |c: &str, id: &str, v: Value| {
            let v = normalize(c, &v, "USD").unwrap();
            st.put(c, Some(id), &v).unwrap();
        };
        crate::fx::save_fetched(&st, "USD", &json!({"BDT": 100.0})).unwrap();
        put(ACCOUNTS, "cash", json!({"name":"Cash","currency":"USD","opening":100}));
        put(ACCOUNTS, "bk", json!({"name":"bKash","kind":"wallet","currency":"BDT","opening":0}));
        put(CATEGORIES, "food", json!({"name":"Food","budget":200}));
        put(CATEGORIES, "bus", json!({"name":"Transport","budget":50}));
        put(CATEGORIES, "sal", json!({"name":"Salary","kind":"income"}));
        put(TRANSACTIONS, "t1", json!({"kind":"expense","amount":20,"date":"2026-11-03","account":"cash","category":"food","note":"Lunch","tags":["work"]}));
        put(TRANSACTIONS, "t2", json!({"kind":"expense","amount":1000,"currency":"BDT","date":"2026-11-04","account":"bk","category":"bus"}));
        put(TRANSACTIONS, "t3", json!({"kind":"income","amount":500,"date":"2026-11-01","account":"cash","category":"sal"}));
        put(TRANSACTIONS, "t4", json!({"kind":"transfer","amount":50,"date":"2026-11-05","account":"cash","to_account":"bk","to_amount":5000}));
        put(TRANSACTIONS, "t5", json!({"kind":"expense","amount":30,"date":"2026-11-06","account":"cash","splits":[{"category":"food","amount":10},{"category":"bus","amount":20}]}));
        put(TRANSACTIONS, "t6", json!({"kind":"expense","amount":40,"date":"2026-10-20","account":"cash","category":"food"}));
        put(LOANS, "l1", json!({"direction":"lent","person":"Sam","amount":80,"given":"2026-11-02","due":"2026-11-20","account":"cash","repayments":[{"date":"2026-11-10","amount":30}]}));
        (st, NaiveDate::from_ymd_opt(2026, 11, 15).unwrap())
    }

    #[test]
    fn balances_and_overview() {
        let (st, today) = data();
        let d = Data::load(&st, "USD").unwrap();
        let acc = accounts_view(&d);
        let cash = acc.iter().find(|a| s(a, "id") == "cash").unwrap();
        // 100 + 500 − 20 − 50 − 30 − 40 − 80 + 30
        assert_eq!(f(cash, "balance"), 410.0);
        let bk = acc.iter().find(|a| s(a, "id") == "bk").unwrap();
        assert_eq!(f(bk, "balance"), 4000.0);
        assert_eq!(f(bk, "balance_main"), 40.0);
        let o = overview(&d, "2026-11", today);
        assert_eq!(f(&o, "spent"), 60.0); // 20 + 10 (1000 BDT) + 30
        assert_eq!(f(&o, "income"), 500.0);
        assert_eq!(f(&o, "budget"), 250.0);
        assert_eq!(f(&o, "remaining"), 190.0);
        assert_eq!(f(&o, "prev_spent"), 40.0);
        // accounts 410 + 40 + lent remaining 50
        assert_eq!(f(&o, "net_worth"), 500.0);
        let cats = arr(&o, "by_category");
        assert_eq!(s(&cats[0], "id"), "food");
        assert_eq!(f(&cats[0], "amount"), 30.0);
        assert_eq!(f(&cats[1], "amount"), 30.0);
    }

    #[test]
    fn trends_and_networth() {
        let (st, today) = data();
        let d = Data::load(&st, "USD").unwrap();
        let t = trends(&d, "2026-11", today);
        assert_eq!(arr(&t, "daily").len(), 30);
        assert_eq!(arr(&t, "daily")[2].as_f64(), Some(20.0));
        assert_eq!(arr(&t, "daily_prev").len(), 31);
        let nw = arr(&t, "networth");
        assert_eq!(nw.len(), 2);
        assert_eq!(f(&nw[0], "value"), 60.0); // Oct: 100 − 40
        assert_eq!(f(&nw[1], "value"), 500.0);
        assert_eq!(f(&t["lifetime"], "spent"), 100.0);
        assert_eq!(arr(&t, "months").len(), 12);
    }

    #[test]
    fn categories_usage_and_loans() {
        let (st, today) = data();
        let d = Data::load(&st, "USD").unwrap();
        let c = categories_view(&d, "2026-11");
        let food = c.iter().find(|x| s(x, "id") == "food").unwrap();
        assert_eq!(food["usage"], json!(3));
        assert_eq!(f(food, "spent"), 30.0);
        let l = loans_view(&d, today);
        assert_eq!(s(&l[0], "status"), "due_soon");
        assert_eq!(f(&l[0], "remaining"), 50.0);
        let mut lf = l[0].clone();
        lf["repayments"] = json!([{"date":"2026-11-10","amount":80}]);
        assert_eq!(loan_status(&lf, today).0, "finished");
        lf["repayments"] = json!([]);
        assert_eq!(loan_status(&lf, NaiveDate::from_ymd_opt(2026, 11, 21).unwrap()).0, "overdue");
    }

    #[test]
    fn search_filters() {
        let (st, _) = data();
        let d = Data::load(&st, "USD").unwrap();
        let all = filter_tx(&d, &Filter::default());
        assert_eq!(all.len(), 6);
        assert_eq!(s(&all[0], "id"), "t5", "newest date first");
        let q = |f0: Filter| filter_tx(&d, &f0).iter().map(|v| s(v, "id").to_string()).collect::<Vec<_>>();
        assert_eq!(q(Filter { q: "lunch".into(), ..Default::default() }), vec!["t1"]);
        assert_eq!(q(Filter { q: "#work".into(), ..Default::default() }), vec!["t1"]);
        assert_eq!(q(Filter { category: "bus".into(), ..Default::default() }), vec!["t5", "t2"]);
        assert_eq!(q(Filter { account: "bk".into(), ..Default::default() }), vec!["t4", "t2"]);
        assert_eq!(q(Filter { kind: "income".into(), ..Default::default() }), vec!["t3"]);
        assert_eq!(q(Filter { from: "2026-11-04".into(), to: "2026-11-05".into(), ..Default::default() }), vec!["t4", "t2"]);
        assert_eq!(q(Filter { min: Some(100.0), ..Default::default() }), vec!["t2", "t3"]);
        assert_eq!(q(Filter { q: "transport".into(), ..Default::default() }), vec!["t5", "t2"]);
    }
}
