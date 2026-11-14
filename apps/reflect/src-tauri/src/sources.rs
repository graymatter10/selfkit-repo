//! Read-only readers for the other apps' databases. Reflect never writes them.
//! Shapes: docs/apps/{chrono,hisab,agenda,dusk}/README.md (Data model).
//! - chrono.db: `app_sessions`, `web_sessions`, `chrono_settings` → screen
//!   time per logical day (union of all devices), phone time, late-night phone
//!   use (00:00–05:00 local), time per app. Cached per day and updated
//!   incrementally from `updated_at` (big table).
//! - hisab.db: `transactions` (expenses), `categories` (names, monthly budgets),
//!   `rates` → spending per day in the main currency.
//! - agenda.db: `completions` (falls back to `tasks.done_at`), open `tasks`.
//! - dusk.db: the `dusk_nights` result cache table.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use selfkit_core::Store;
use serde_json::Value;

use crate::model::{DAY, MIN, Ms};

/// Read-only handle on another app's DB, or None when the app isn't here.
pub fn open_other(app: &str) -> Option<Store> {
    open_at(&selfkit_core::paths::app_db(app), app)
}

pub fn open_at(p: &Path, app: &str) -> Option<Store> {
    if !p.exists() {
        return None;
    }
    Store::open_read_only(p, app).ok()
}

/// Change stamp of a DB file (+ its WAL): cheap "did it change?" check.
pub fn stamp(app: &str) -> Option<(u128, u64)> {
    let p = selfkit_core::paths::app_db(app);
    let m = |p: &PathBuf| std::fs::metadata(p).ok().map(|m| (m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0), m.len()));
    let a = m(&p)?;
    let mut w = p.clone().into_os_string();
    w.push("-wal");
    let b = m(&PathBuf::from(w)).unwrap_or((0, 0));
    Some((a.0.max(b.0), a.1 + b.1))
}

pub(crate) fn num(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)))
}
fn fnum(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str()?.replace(',', "").parse().ok()))
}
fn st<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Logical day index (days since 1970-01-01) of a UTC ms time.
pub fn day_idx(ms: Ms, tz: i32, day_start: u16) -> i32 {
    (ms + tz as i64 * MIN - day_start as i64 * MIN).div_euclid(DAY) as i32
}
/// UTC ms where logical day `i` starts.
pub fn day_begin(i: i32, tz: i32, day_start: u16) -> Ms {
    i as i64 * DAY + day_start as i64 * MIN - tz as i64 * MIN
}

// ---------------------------------------------------------------- Chrono --

#[derive(Debug, Clone, Default)]
pub struct ChronoFilter {
    pub exclude_apps: Vec<String>,
    pub exclude_domains: Vec<String>,
    pub device_names: HashMap<String, String>,
}

impl ChronoFilter {
    pub fn load(store: &Store) -> ChronoFilter {
        let mut f = ChronoFilter::default();
        let Ok(Some(r)) = store.get("settings") else { return f };
        if r.deleted {
            return f;
        }
        let list = |k: &str| -> Vec<String> {
            r.data.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_lowercase())).collect()).unwrap_or_default()
        };
        f.exclude_apps = list("exclude_apps");
        f.exclude_domains = list("exclude_domains").into_iter().map(|d| d.trim_start_matches("www.").to_string()).collect();
        if let Some(o) = r.data.get("device_names").and_then(Value::as_object) {
            for (k, v) in o {
                if let Some(s) = v.as_str() {
                    f.device_names.insert(k.clone(), s.to_string());
                }
            }
        }
        f
    }
    fn app_excluded(&self, app: &str, package: &str) -> bool {
        let (a, p) = (app.to_lowercase(), package.to_lowercase());
        self.exclude_apps.iter().any(|x| *x == a || (!p.is_empty() && *x == p))
    }
    fn domain_excluded(&self, d: &str) -> bool {
        let d = d.to_lowercase();
        let d = d.trim_start_matches("www.");
        self.exclude_domains.iter().any(|x| d == x || d.ends_with(&format!(".{x}")))
    }
    /// A key for the settings that change results (excludes).
    pub fn key(&self) -> String {
        format!("{:?}{:?}", self.exclude_apps, self.exclude_domains)
    }
}

/// One session piece from chrono.db.
#[derive(Debug, Clone)]
pub struct Sess {
    pub start: Ms,
    pub end: Ms,
    pub tz: i32,
    /// App label ("" for websites).
    pub app: String,
    pub phone: bool,
    /// Device label: "Phone", a Chrono device name, or "Linux PC"/"Windows PC".
    pub device: String,
}

/// Parse one app/web session record (None = excluded or malformed).
pub fn parse_session(coll: &str, v: &Value, rec_tz: i32, rec_dev: &str, f: &ChronoFilter) -> Option<Sess> {
    let (s, e) = (num(v, "start")?, num(v, "end")?);
    if e <= s {
        return None;
    }
    let tz = num(v, "tz_offset").map(|x| x as i32).unwrap_or(rec_tz);
    let platform = st(v, "platform");
    if coll == "web_sessions" {
        if f.domain_excluded(st(v, "domain")) {
            return None;
        }
        return Some(Sess { start: s, end: e, tz, app: String::new(), phone: false, device: String::new() });
    }
    let app = st(v, "app");
    if f.app_excluded(app, st(v, "package")) {
        return None;
    }
    let phone = platform == "android";
    let dev_id = if phone { st(v, "device").to_string() } else { rec_dev.to_string() };
    let device = f.device_names.get(&dev_id).cloned().unwrap_or_else(|| {
        match platform {
            "android" => "Phone".into(),
            "windows" => "Windows PC".into(),
            _ => "Linux PC".into(),
        }
    });
    Some(Sess { start: s, end: e, tz, app: app.to_string(), phone, device })
}

/// Screen data of one logical day.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CDay {
    pub screen_ms: Ms,
    pub phone_ms: Ms,
    /// (app, device, ms), longest first, at most 12.
    pub apps: Vec<(String, String, Ms)>,
}

fn union_ms(mut v: Vec<(Ms, Ms)>) -> Ms {
    v.sort_unstable();
    let (mut tot, mut cur): (Ms, Option<(Ms, Ms)>) = (0, None);
    for (a, b) in v {
        match cur {
            Some((s, e)) if a <= e => cur = Some((s, e.max(b))),
            Some((s, e)) => {
                tot += e - s;
                cur = Some((a, b));
            }
            None => cur = Some((a, b)),
        }
    }
    if let Some((s, e)) = cur {
        tot += e - s;
    }
    tot
}

/// Merge overlapping intervals (for the day ribbon).
pub fn merge(mut v: Vec<(Ms, Ms)>) -> Vec<(Ms, Ms)> {
    v.sort_unstable();
    let mut out: Vec<(Ms, Ms)> = Vec::new();
    for (a, b) in v {
        if let Some(l) = out.last_mut()
            && a <= l.1 + 60_000
        {
            l.1 = l.1.max(b);
            continue;
        }
        out.push((a, b));
    }
    out
}

/// Aggregate sessions into logical days in [lo, hi] and late-night phone
/// minutes per calendar date in [lo, hi + 1].
pub fn aggregate(sess: &[Sess], day_start: u16, lo: i32, hi: i32) -> (BTreeMap<i32, CDay>, BTreeMap<i32, Ms>) {
    let mut all: BTreeMap<i32, Vec<(Ms, Ms)>> = BTreeMap::new();
    let mut phone: BTreeMap<i32, Vec<(Ms, Ms)>> = BTreeMap::new();
    let mut apps: BTreeMap<i32, HashMap<(String, String), Ms>> = BTreeMap::new();
    let mut late: BTreeMap<i32, Vec<(Ms, Ms)>> = BTreeMap::new();
    for s in sess {
        let mut a = s.start;
        while a < s.end {
            let d = day_idx(a, s.tz, day_start);
            let b = s.end.min(day_begin(d + 1, s.tz, day_start));
            if d >= lo && d <= hi {
                all.entry(d).or_default().push((a, b));
                if s.phone {
                    phone.entry(d).or_default().push((a, b));
                }
                if !s.app.is_empty() {
                    *apps.entry(d).or_default().entry((s.app.clone(), s.device.clone())).or_default() += b - a;
                }
            }
            a = b;
        }
        if s.phone {
            // Late night = 00:00–05:00 local on the calendar date.
            let first = day_idx(s.start, s.tz, 0);
            let last = day_idx(s.end - 1, s.tz, 0);
            for c in first..=last {
                let (w0, w1) = (day_begin(c, s.tz, 0), day_begin(c, s.tz, 0) + 5 * 60 * MIN);
                let (x, y) = (s.start.max(w0), s.end.min(w1));
                if y > x && c >= lo && c <= hi + 1 {
                    late.entry(c).or_default().push((x, y));
                }
            }
        }
    }
    let mut days = BTreeMap::new();
    for d in lo..=hi {
        let Some(v) = all.remove(&d) else { continue };
        let mut ap: Vec<(String, String, Ms)> = apps.remove(&d).unwrap_or_default().into_iter().map(|((a, dv), ms)| (a, dv, ms)).collect();
        ap.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        ap.truncate(12);
        days.insert(d, CDay { screen_ms: union_ms(v), phone_ms: phone.remove(&d).map(union_ms).unwrap_or(0), apps: ap });
    }
    let late = late.into_iter().map(|(c, v)| (c, union_ms(v))).collect();
    (days, late)
}

/// Chrono cache: per-day aggregates, updated from records changed since the
/// last look (`updated_at`), so a year of sessions is read once.
#[derive(Debug, Default)]
pub struct ChronoCache {
    pub key: String,
    pub last_upd: Ms,
    pub days: BTreeMap<i32, CDay>,
    pub late: BTreeMap<i32, Ms>,
    pub has_phone: bool,
    pub first: Option<i32>,
}

const SESS: &str = "collection IN ('app_sessions','web_sessions') AND deleted=0";

fn load_sessions(store: &Store, f: &ChronoFilter, window: Option<(Ms, Ms)>) -> Vec<Sess> {
    let mut out = Vec::new();
    let sql = match window {
        Some(_) => format!(
            "SELECT collection, data, tz_offset, device_id FROM records WHERE {SESS}
             AND json_extract(data,'$.start') < ?2 AND json_extract(data,'$.end') > ?1"
        ),
        None => format!("SELECT collection, data, tz_offset, device_id FROM records WHERE {SESS}"),
    };
    let Ok(mut q) = store.conn().prepare(&sql) else { return out };
    let map = |r: &rusqlite::Row| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?));
    let rows: Vec<_> = match window {
        Some((a, b)) => q.query_map((a, b), map).map(|r| r.flatten().collect()).unwrap_or_default(),
        None => q.query_map([], map).map(|r| r.flatten().collect()).unwrap_or_default(),
    };
    for (coll, data, tz, dev) in rows {
        let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
        if let Some(s) = parse_session(&coll, &v, tz as i32, &dev, f) {
            out.push(s);
        }
    }
    out
}

impl ChronoCache {
    /// Bring the cache up to date. `key` = settings that change results.
    pub fn update(&mut self, store: &Store, day_start: u16) {
        let f = ChronoFilter::load(store);
        let key = format!("{}|{day_start}", f.key());
        let max_upd: Ms = store
            .conn()
            .query_row("SELECT COALESCE(MAX(updated_at),0) FROM records WHERE collection IN ('app_sessions','web_sessions')", [], |r| r.get(0))
            .unwrap_or(0);
        if key != self.key || self.last_upd == 0 {
            *self = ChronoCache { key, ..Default::default() };
            let sess = load_sessions(store, &f, None);
            self.has_phone = sess.iter().any(|s| s.phone);
            if let (Some(lo), Some(hi)) = (sess.iter().map(|s| day_idx(s.start, s.tz, day_start)).min(), sess.iter().map(|s| day_idx(s.end - 1, s.tz, day_start)).max()) {
                let (d, l) = aggregate(&sess, day_start, lo, hi);
                self.days = d;
                self.late = l;
                self.first = Some(lo);
            }
            self.last_upd = max_upd.max(1);
            return;
        }
        if max_upd <= self.last_upd {
            return;
        }
        // Days touched by records written since the last look (incl. deletions).
        let mut lo = i32::MAX;
        let mut hi = i32::MIN;
        if let Ok(mut q) = store.conn().prepare(
            "SELECT data, tz_offset, deleted FROM records WHERE collection IN ('app_sessions','web_sessions') AND updated_at > ?1",
        ) {
            let rows = q.query_map([self.last_upd], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, bool>(2)?)));
            for (data, tz, deleted) in rows.into_iter().flatten().flatten() {
                let v: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
                match (num(&v, "start"), num(&v, "end")) {
                    (Some(s), Some(e)) if e > s => {
                        let tz = num(&v, "tz_offset").map(|x| x as i32).unwrap_or(tz as i32);
                        lo = lo.min(day_idx(s, tz, day_start) - 1);
                        hi = hi.max(day_idx(e - 1, tz, day_start));
                    }
                    _ if deleted => {
                        // A tombstone without data: start over.
                        self.last_upd = 0;
                        return self.update(store, day_start);
                    }
                    _ => {}
                }
            }
        }
        self.last_upd = max_upd;
        if lo > hi {
            return;
        }
        let tzn = selfkit_core::time::local_offset_min();
        let window = (day_begin(lo, tzn, day_start) - DAY, day_begin(hi + 1, tzn, day_start) + DAY);
        let sess = load_sessions(store, &f, Some(window));
        if sess.iter().any(|s| s.phone) {
            self.has_phone = true;
        }
        let (d, l) = aggregate(&sess, day_start, lo, hi);
        for i in lo..=hi {
            match d.get(&i) {
                Some(x) => {
                    self.days.insert(i, x.clone());
                }
                None => {
                    self.days.remove(&i);
                }
            }
        }
        for c in lo..=hi + 1 {
            match l.get(&c) {
                Some(x) => {
                    self.late.insert(c, *x);
                }
                None => {
                    self.late.remove(&c);
                }
            }
        }
        self.first = self.days.keys().next().copied();
    }
}

/// Today's sessions (for the "Your day" ribbon), merged, as UTC ms.
pub fn sessions_between(store: &Store, from: Ms, to: Ms) -> Vec<Sess> {
    let f = ChronoFilter::load(store);
    load_sessions(store, &f, Some((from, to)))
}

// ----------------------------------------------------------------- Hisab --

#[derive(Debug, Clone, Default)]
pub struct Spend {
    pub day: i32,
    /// Local minute of the day (for the ribbon), None when no time was typed.
    pub minute: Option<u16>,
    pub amount: f64,
    pub category: String,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct HisabData {
    pub spends: Vec<Spend>,
    /// Logical day → total spent (main currency); days since the first entry.
    pub per_day: BTreeMap<i32, f64>,
    pub first: Option<i32>,
    /// Sum of monthly category budgets (main currency); 0 = none set.
    pub monthly_budget: f64,
}

/// Units of `c` per 1 base currency (Hisab's `hisab-rates` record).
fn rates(store: &Store) -> (String, HashMap<String, f64>) {
    let mut m = HashMap::new();
    let mut base = "USD".to_string();
    if let Ok(Some(r)) = store.get("hisab-rates") {
        let b = st(&r.data, "base");
        if !b.is_empty() {
            base = b.to_string();
        }
        for k in ["rates", "manual"] {
            if let Some(o) = r.data.get(k).and_then(Value::as_object) {
                for (c, x) in o {
                    if let Some(x) = x.as_f64().filter(|x| *x > 0.0) {
                        m.insert(c.to_ascii_uppercase(), x);
                    }
                }
            }
        }
    }
    m.insert(base.clone(), 1.0);
    (base, m)
}

pub fn read_hisab(store: &Store, main: &str, day_start: u16) -> HisabData {
    let (_, rt) = rates(store);
    let conv = |a: f64, c: &str| -> f64 {
        let c = if c.is_empty() { main } else { c };
        if c.eq_ignore_ascii_case(main) {
            return a;
        }
        match (rt.get(&c.to_ascii_uppercase()), rt.get(&main.to_ascii_uppercase())) {
            (Some(x), Some(y)) => a / x * y,
            _ => a,
        }
    };
    let mut cats: HashMap<String, String> = HashMap::new();
    let mut budget = 0.0;
    if let Ok(mut q) = store.conn().prepare("SELECT id, data FROM records WHERE collection='categories' AND deleted=0") {
        for (id, data) in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).into_iter().flatten().flatten() {
            let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
            cats.insert(id, st(&v, "name").to_string());
            if st(&v, "kind") != "income" && !v.get("archived").and_then(Value::as_bool).unwrap_or(false) {
                budget += fnum(&v, "budget").unwrap_or(0.0).max(0.0);
            }
        }
    }
    let mut out = HisabData { monthly_budget: budget, ..Default::default() };
    let Ok(mut q) = store.conn().prepare("SELECT data, tz_offset FROM records WHERE collection='transactions' AND deleted=0") else { return out };
    let rows: Vec<(String, i64)> = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|r| r.flatten().collect()).unwrap_or_default();
    for (data, tz) in rows {
        let Ok(v) = serde_json::from_str::<Value>(&data) else { continue };
        let Some(date) = v.get("date").and_then(Value::as_str).and_then(|s| chrono::NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()) else {
            continue;
        };
        let minute = chrono::NaiveTime::parse_from_str(st(&v, "time"), "%H:%M").ok().map(|t| (chrono::Timelike::hour(&t) * 60 + chrono::Timelike::minute(&t)) as u16);
        let mut d = (date - crate::model::epoch()).num_days() as i32;
        if minute.is_some_and(|m| m < day_start) {
            d -= 1;
        }
        let _ = tz;
        out.first = Some(out.first.map_or(d, |f| f.min(d)));
        out.per_day.entry(d).or_insert(0.0);
        if st(&v, "kind") != "expense" {
            continue;
        }
        let amount = conv(fnum(&v, "amount").unwrap_or(0.0).abs(), st(&v, "currency"));
        if amount <= 0.0 {
            continue;
        }
        let cat_id = v.get("splits").and_then(Value::as_array).and_then(|a| {
            a.iter().max_by(|x, y| fnum(x, "amount").unwrap_or(0.0).total_cmp(&fnum(y, "amount").unwrap_or(0.0))).map(|x| st(x, "category").to_string())
        });
        let cat_id = cat_id.unwrap_or_else(|| st(&v, "category").to_string());
        let category = cats.get(&cat_id).cloned().unwrap_or_else(|| if cat_id.is_empty() { "Other".into() } else { cat_id.clone() });
        *out.per_day.entry(d).or_insert(0.0) += amount;
        out.spends.push(Spend { day: d, minute, amount, category, note: st(&v, "note").to_string() });
    }
    out
}

// ---------------------------------------------------------------- Agenda --

#[derive(Debug, Clone, Default)]
pub struct AgendaData {
    /// (logical day, UTC ms done) per completion.
    pub done: Vec<(i32, Ms, i32)>,
    pub per_day: BTreeMap<i32, f64>,
    pub first: Option<i32>,
    /// Open tasks whose start date is the given local date ("YYYY-MM-DD").
    pub open_by_date: HashMap<String, u32>,
}

pub fn read_agenda(store: &Store, day_start: u16) -> AgendaData {
    let mut out = AgendaData::default();
    let rows = |coll: &str| -> Vec<(String, i64, i64)> {
        let Ok(mut q) = store.conn().prepare("SELECT data, tz_offset, updated_at FROM records WHERE collection=?1 AND deleted=0") else { return vec![] };
        q.query_map([coll], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|r| r.flatten().collect()).unwrap_or_default()
    };
    let comps = rows("completions");
    let tasks = rows("tasks");
    let use_tasks = comps.is_empty();
    for (data, tz, _) in &comps {
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        let Some(t) = num(&v, "done_at") else { continue };
        let tz = num(&v, "tz_offset_min").map(|x| x as i32).unwrap_or(*tz as i32);
        out.done.push((day_idx(t, tz, day_start), t, tz));
    }
    for (data, tz, upd) in &tasks {
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        let created = num(&v, "created_at").unwrap_or(*upd);
        let cd = day_idx(created, *tz as i32, day_start);
        out.first = Some(out.first.map_or(cd, |f| f.min(cd)));
        match num(&v, "done_at").filter(|d| *d > 0) {
            Some(t) if use_tasks => out.done.push((day_idx(t, *tz as i32, day_start), t, *tz as i32)),
            Some(_) => {}
            None => {
                let s = st(&v, "start");
                if s.len() >= 10 {
                    *out.open_by_date.entry(s[..10].to_string()).or_default() += 1;
                }
            }
        }
    }
    for (d, _, _) in &out.done {
        *out.per_day.entry(*d).or_default() += 1.0;
        out.first = Some(out.first.map_or(*d, |f| f.min(*d)));
    }
    out
}

// ------------------------------------------------------------------ Dusk --

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Night {
    /// Wake date "YYYY-MM-DD".
    pub night: String,
    pub bed: Option<Ms>,
    pub wake: Option<Ms>,
    pub tz_offset: i32,
    pub asleep_min: i64,
    pub quality: Option<u8>,
    /// estimate | logged | none | allnighter
    pub source: String,
    pub late_use_min: i64,
}

pub fn read_dusk(store: &Store) -> Vec<Night> {
    let Ok(mut q) = store.conn().prepare(
        "SELECT night, bed, wake, tz_offset, asleep_min, nap_min, quality, source, late_use_min FROM dusk_nights ORDER BY night",
    ) else {
        return vec![];
    };
    q.query_map([], |r| {
        Ok(Night {
            night: r.get(0)?,
            bed: r.get(1)?,
            wake: r.get(2)?,
            tz_offset: r.get::<_, Option<i64>>(3)?.unwrap_or(0) as i32,
            asleep_min: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
            quality: r.get::<_, Option<i64>>(6)?.map(|q| q.clamp(1, 5) as u8),
            source: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            late_use_min: r.get::<_, Option<i64>>(8)?.unwrap_or(0),
        })
    })
    .map(|r| r.flatten().collect())
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const H: Ms = 60 * MIN;

    #[test]
    fn union_and_split() {
        // tz +60, day start 04:00. A phone session 23:00–01:30 local on day 0.
        let tz = 60;
        let base = day_begin(10, tz, 0); // local midnight of calendar day 10
        let s = Sess { start: base - H, end: base + 90 * MIN, tz, app: "WhatsApp".into(), phone: true, device: "Phone".into() };
        let pc = Sess { start: base - 30 * MIN, end: base + 30 * MIN, tz, app: "code".into(), phone: false, device: "Linux PC".into() };
        let (d, late) = aggregate(&[s.clone(), pc], 240, 0, 20);
        // Everything before 04:00 belongs to logical day 9.
        let d9 = &d[&9];
        assert_eq!(d9.screen_ms, 150 * MIN, "overlap counts once");
        assert_eq!(d9.phone_ms, 150 * MIN);
        assert_eq!(d9.apps[0].0, "WhatsApp");
        assert_eq!(late[&10], 90 * MIN, "00:00–01:30 is late use on calendar day 10");
        // Day start 00:00 splits at midnight instead.
        let (d, _) = aggregate(&[s], 0, 0, 20);
        assert_eq!(d[&9].screen_ms, H);
        assert_eq!(d[&10].screen_ms, 90 * MIN);
    }

    #[test]
    fn excludes_and_labels() {
        let s = Store::open_in_memory("chrono", "pc1", &[]).unwrap();
        s.put("chrono_settings", Some("settings"), &json!({"exclude_apps":["keepassxc"],"exclude_domains":["bank.com"],"device_names":{"pc1":"Desk PC"}})).unwrap();
        let f = ChronoFilter::load(&s);
        assert!(parse_session("app_sessions", &json!({"app":"KeePassXC","platform":"linux","start":1,"end":5}), 0, "pc1", &f).is_none());
        assert!(parse_session("web_sessions", &json!({"domain":"www.my.bank.com","start":1,"end":5}), 0, "pc1", &f).is_none());
        let x = parse_session("app_sessions", &json!({"app":"code","platform":"linux","start":1,"end":5}), 0, "pc1", &f).unwrap();
        assert_eq!(x.device, "Desk PC");
        let p = parse_session("app_sessions", &json!({"app":"YouTube","platform":"android","device":"ph","start":1,"end":5}), 0, "pc1", &f).unwrap();
        assert!(p.phone);
        assert_eq!(p.device, "Phone");
    }

    #[test]
    fn chrono_cache_updates_incrementally() {
        let s = Store::open_in_memory("chrono", "pc1", &[]).unwrap();
        let tz = selfkit_core::time::local_offset_min();
        let d0 = day_begin(20_000, tz, 0);
        s.put("app_sessions", None, &json!({"app":"code","platform":"linux","start": d0 + 10 * H, "end": d0 + 11 * H, "tz_offset": tz})).unwrap();
        let mut c = ChronoCache::default();
        c.update(&s, 0);
        assert_eq!(c.days[&20_000].screen_ms, H);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let r = s.put("app_sessions", None, &json!({"app":"code","platform":"linux","start": d0 + 12 * H, "end": d0 + 13 * H, "tz_offset": tz})).unwrap();
        c.update(&s, 0);
        assert_eq!(c.days[&20_000].screen_ms, 2 * H, "new record picked up");
        std::thread::sleep(std::time::Duration::from_millis(5));
        s.put("app_sessions", Some(&r.id), &json!({"app":"code","platform":"linux","start": d0 + 12 * H, "end": d0 + 12 * H + 30 * MIN, "tz_offset": tz})).unwrap();
        c.update(&s, 0);
        assert_eq!(c.days[&20_000].screen_ms, H + 30 * MIN, "edited record recomputed");
    }

    #[test]
    fn hisab_converts_and_budgets() {
        let s = Store::open_in_memory("hisab", "d", &[]).unwrap();
        s.put("rates", Some("hisab-rates"), &json!({"base":"USD","rates":{"BDT":120.0},"manual":{}})).unwrap();
        let food = s.put("categories", None, &json!({"name":"Food","kind":"expense","budget":300})).unwrap();
        s.put("categories", None, &json!({"name":"Salary","kind":"income","budget":0})).unwrap();
        s.put("transactions", None, &json!({"kind":"expense","amount":12.5,"currency":"USD","date":"2026-01-05","time":"13:00","category":food.id})).unwrap();
        s.put("transactions", None, &json!({"kind":"expense","amount":1200,"currency":"BDT","date":"2026-01-05","time":"02:00","category":food.id})).unwrap();
        s.put("transactions", None, &json!({"kind":"income","amount":900,"currency":"USD","date":"2026-01-06"})).unwrap();
        let h = read_hisab(&s, "USD", 240);
        let d = |x: &str| (chrono::NaiveDate::parse_from_str(x, "%Y-%m-%d").unwrap() - crate::model::epoch()).num_days() as i32;
        assert_eq!(h.per_day[&d("2026-01-05")], 12.5);
        assert_eq!(h.per_day[&d("2026-01-04")], 10.0, "02:00 counts for the day before; BDT converted");
        assert_eq!(h.per_day[&d("2026-01-06")], 0.0);
        assert_eq!(h.monthly_budget, 300.0);
        assert_eq!(h.spends[0].category, "Food");
    }

    #[test]
    fn agenda_completions_and_open() {
        let s = Store::open_in_memory("agenda", "d", &[]).unwrap();
        s.put("tasks", None, &json!({"title":"a","start":"2026-11-15T10:00","created_at": 1_000})).unwrap();
        s.put("completions", None, &json!({"task":"x","done_at": 2 * DAY + 5 * MIN,"tz_offset_min": 0})).unwrap();
        s.put("completions", None, &json!({"task":"y","done_at": 2 * DAY + 9 * MIN,"tz_offset_min": 0})).unwrap();
        let a = read_agenda(&s, 0);
        assert_eq!(a.per_day[&2], 2.0);
        assert_eq!(a.open_by_date["2026-11-15"], 1);
        assert_eq!(a.first, Some(0));
    }
}
