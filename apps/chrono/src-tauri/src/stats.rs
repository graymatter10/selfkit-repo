//! Aggregation over months of activity data, all in Rust.
//!
//! Sources: `app_sessions` (desktop agent + Android importer), `web_sessions`
//! (browser extension), `android_events` (hourly unlock/pickup/notification
//! counters). Each session is split at local-day boundaries (its own
//! `tz_offset`, shifted by the shared day-start). Per-day aggregates are cached
//! in the local table `chrono_daily` and invalidated by the store's change
//! sequence (`lseq`), so trend screens over years read a few hundred small rows.
//! Docs: docs/apps/chrono/stats.md.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::params;
use selfkit_core::store::Migration;
use selfkit_core::{Result, Store};
use serde::{Deserialize, Serialize};

use crate::categories::{category_pieces, focus_score, is_browser, rule_pieces, rules_for, static_category};
use crate::settings::ChronoSettings;

pub const MIGRATIONS: &[Migration] = &[Migration {
    name: "chrono/001_daily",
    sql: r#"
CREATE TABLE IF NOT EXISTS chrono_daily (day INTEGER PRIMARY KEY, data TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS chrono_rec_start ON records(collection, json_extract(data, '$.start'));
CREATE INDEX IF NOT EXISTS chrono_rec_hour ON records(collection, json_extract(data, '$.hour'));
"#,
}];

pub const DAY: i64 = 86_400_000;
const HOUR: i64 = 3_600_000;
/// Longest session we look back for when loading a window.
const MAX_SESSION: i64 = DAY;
/// Bump when DayAgg's shape or rules change (invalidates every cache).
const CACHE_VERSION: &str = "c4";

// ---- days ---------------------------------------------------------------

pub fn day_to_str(d: i64) -> String {
    (NaiveDate::from_ymd_opt(1970, 1, 1).unwrap() + Duration::days(d)).format("%Y-%m-%d").to_string()
}

pub fn str_to_day(s: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    Some((d - NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days())
}

pub fn date_of(d: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).unwrap() + Duration::days(d)
}

pub fn day_of_date(d: NaiveDate) -> i64 {
    (d - NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days()
}

/// Monday = 0.
pub fn weekday(d: i64) -> usize {
    (d + 3).rem_euclid(7) as usize
}

/// Local day number of instant `t` with offset `tz` (min) and day start `ds` (min).
pub fn local_day(t: i64, tz: i32, ds: i64) -> i64 {
    (t + tz as i64 * 60_000 - ds * 60_000).div_euclid(DAY)
}

/// UTC instant where local day `d` starts.
pub fn day_start_utc(d: i64, tz: i32, ds: i64) -> i64 {
    d * DAY - tz as i64 * 60_000 + ds * 60_000
}

// ---- raw data -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Seg {
    /// 'a' app, 'w' website
    pub kind: char,
    pub name: String,
    pub package: Option<String>,
    /// linux | windows | android | chrome | firefox
    pub platform: String,
    pub device: String,
    pub start: i64,
    pub end: i64,
    pub tz: i32,
}

#[derive(Debug, Clone)]
pub struct HourRec {
    pub hour: i64,
    pub tz: i32,
    pub unlocks: i64,
    pub pickups: i64,
    pub notif: BTreeMap<String, i64>,
}

fn s_of(v: &serde_json::Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(str::to_string)
}

/// Sessions starting in [from - MAX_SESSION, to) that overlap [from, to).
pub fn load_segs(store: &Store, set: &ChronoSettings, from: i64, to: i64) -> Result<Vec<Seg>> {
    let mut st = store.conn().prepare_cached(
        "SELECT collection, data, device_id FROM records
         WHERE collection IN ('app_sessions', 'web_sessions') AND deleted = 0
           AND json_extract(data, '$.start') >= ?1 AND json_extract(data, '$.start') < ?2",
    )?;
    let rows = st.query_map(params![from - MAX_SESSION, to], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (col, data, dev) = row?;
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) else { continue };
        let (Some(start), Some(end)) = (v.get("start").and_then(|x| x.as_i64()), v.get("end").and_then(|x| x.as_i64())) else {
            continue;
        };
        if end <= from || end <= start {
            continue;
        }
        let tz = v.get("tz_offset").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
        let device = s_of(&v, "device").filter(|s| !s.is_empty()).unwrap_or(dev);
        let seg = if col == "app_sessions" {
            let Some(name) = s_of(&v, "app").filter(|s| !s.is_empty()) else { continue };
            let package = s_of(&v, "package");
            if set.app_excluded(&name, package.as_deref()) {
                continue;
            }
            Seg { kind: 'a', name, package, platform: s_of(&v, "platform").unwrap_or_default(), device, start, end, tz }
        } else {
            let Some(name) = s_of(&v, "domain").filter(|s| !s.is_empty()) else { continue };
            if set.domain_excluded(&name) {
                continue;
            }
            Seg { kind: 'w', name, package: None, platform: s_of(&v, "browser").unwrap_or_default(), device, start, end, tz }
        };
        out.push(seg);
    }
    out.sort_by_key(|s| s.start);
    Ok(out)
}

pub fn load_hours(store: &Store, set: &ChronoSettings, from: i64, to: i64) -> Result<Vec<HourRec>> {
    let mut st = store.conn().prepare_cached(
        "SELECT data FROM records WHERE collection = 'android_events' AND deleted = 0
           AND json_extract(data, '$.hour') >= ?1 AND json_extract(data, '$.hour') < ?2",
    )?;
    let rows = st.query_map(params![from, to], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&row?) else { continue };
        let Some(hour) = v.get("hour").and_then(|x| x.as_i64()) else { continue };
        let mut notif = BTreeMap::new();
        if let Some(m) = v.get("notifications").and_then(|x| x.as_object()) {
            for (k, n) in m {
                if !set.app_excluded(k, Some(k)) {
                    notif.insert(k.clone(), n.as_i64().unwrap_or(0));
                }
            }
        }
        out.push(HourRec {
            hour,
            tz: v.get("tz_offset").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            unlocks: v.get("unlocks").and_then(|x| x.as_i64()).unwrap_or(0),
            pickups: v.get("pickups").and_then(|x| x.as_i64()).unwrap_or(0),
            notif,
        });
    }
    Ok(out)
}

/// Split a segment at local-day boundaries: (day, start, end).
pub fn split_days(s: &Seg, ds: i64) -> Vec<(i64, i64, i64)> {
    let mut v = Vec::new();
    let mut t = s.start;
    while t < s.end {
        let d = local_day(t, s.tz, ds);
        let e = s.end.min(day_start_utc(d + 1, s.tz, ds));
        v.push((d, t, e));
        t = e;
    }
    v
}

/// Total length of the union of intervals.
pub fn union_ms(iv: &mut [(i64, i64)]) -> i64 {
    merge(iv).iter().map(|(a, b)| b - a).sum()
}

pub fn merge(iv: &mut [(i64, i64)]) -> Vec<(i64, i64)> {
    iv.sort_unstable();
    let mut out: Vec<(i64, i64)> = Vec::new();
    for &(a, b) in iv.iter() {
        match out.last_mut() {
            Some(l) if a <= l.1 => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

// ---- per-day aggregate ----------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub k: char,
    pub n: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pk: Option<String>,
    pub d: String,
    pub p: String,
    pub ms: i64,
    pub opens: i64,
    /// Part of `ms` inside time-rule windows: rule category → ms (the rest
    /// has the item's static category). Depends on the rules → cache key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rc: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DayAgg {
    pub screen: i64,
    pub items: Vec<Item>,
    /// device id → union ms
    pub dev: BTreeMap<String, i64>,
    /// device id → platforms seen
    pub devp: BTreeMap<String, Vec<String>>,
    /// union ms per local clock hour
    pub hours: Vec<i64>,
    pub unlocks: i64,
    pub pickups: i64,
    pub notif: BTreeMap<String, i64>,
    /// (name, ms, start)
    pub longest: Option<(String, i64, i64)>,
    pub first: Option<i64>,
    pub last: Option<i64>,
    pub sessions: i64,
}

/// Build one day's aggregate from its pieces (already clipped to the day).
/// `set` supplies the time rules whose share is kept per item (`Item::rc`).
pub fn build_day(pieces: &[(&Seg, i64, i64)], hours: &[&HourRec], set: &ChronoSettings) -> DayAgg {
    let mut a = DayAgg { hours: vec![0; 24], ..Default::default() };
    let mut rules: HashMap<(char, &str, Option<&str>), Vec<&crate::settings::CategoryRule>> = HashMap::new();
    let mut all: Vec<(i64, i64, i32)> = Vec::new();
    let mut per_dev: HashMap<&str, Vec<(i64, i64)>> = HashMap::new();
    let mut items: HashMap<(char, &str, &str, &str), Item> = HashMap::new();
    for &(s, st, en) in pieces {
        all.push((st, en, s.tz));
        per_dev.entry(&s.device).or_default().push((st, en));
        let p = a.devp.entry(s.device.clone()).or_default();
        if !p.contains(&s.platform) {
            p.push(s.platform.clone());
        }
        let it = items.entry((s.kind, &s.name, &s.device, &s.platform)).or_insert_with(|| Item {
            k: s.kind,
            n: s.name.clone(),
            pk: s.package.clone(),
            d: s.device.clone(),
            p: s.platform.clone(),
            ms: 0,
            opens: 0,
            rc: BTreeMap::new(),
        });
        it.ms += en - st;
        let rs = rules.entry((s.kind, &s.name, s.package.as_deref())).or_insert_with(|| rules_for(set, s.kind, &s.name, s.package.as_deref()));
        for (x, y, c) in rule_pieces(rs, st, en, s.tz) {
            *it.rc.entry(c).or_default() += y - x;
        }
        if st == s.start {
            it.opens += 1;
            a.sessions += 1;
        }
        if a.longest.as_ref().is_none_or(|l| en - st > l.1) {
            a.longest = Some((s.name.clone(), en - st, st));
        }
        a.first = Some(a.first.map_or(st, |f| f.min(st)));
        a.last = Some(a.last.map_or(en, |l| l.max(en)));
    }
    // union across everything, and per clock hour
    all.sort_unstable_by_key(|x| (x.0, x.1));
    let mut merged: Vec<(i64, i64, i32)> = Vec::new();
    for (s, e, tz) in all {
        match merged.last_mut() {
            Some(l) if s <= l.1 => l.1 = l.1.max(e),
            _ => merged.push((s, e, tz)),
        }
    }
    for &(s, e, tz) in &merged {
        a.screen += e - s;
        let off = tz as i64 * 60_000;
        let mut t = s;
        while t < e {
            let local = t + off;
            let h = (local.rem_euclid(DAY) / HOUR) as usize;
            let next = (local.div_euclid(HOUR) + 1) * HOUR - off;
            let seg_end = e.min(next);
            a.hours[h] += seg_end - t;
            t = seg_end;
        }
    }
    for (d, mut iv) in per_dev {
        a.dev.insert(d.to_string(), union_ms(&mut iv));
    }
    let mut items: Vec<Item> = items.into_values().collect();
    items.sort_by(|x, y| y.ms.cmp(&x.ms).then_with(|| x.n.cmp(&y.n)));
    a.items = items;
    for h in hours {
        a.unlocks += h.unlocks;
        a.pickups += h.pickups;
        for (k, n) in &h.notif {
            *a.notif.entry(k.clone()).or_default() += n;
        }
    }
    a
}

/// Compute aggregates for local days [d0, d1] straight from records.
pub fn compute_days(store: &Store, set: &ChronoSettings, ds: i64, d0: i64, d1: i64) -> Result<BTreeMap<i64, DayAgg>> {
    // Widest UTC window covering these local days in any timezone (-12h..+14h).
    let from = d0 * DAY - 14 * HOUR + ds * 60_000;
    let to = (d1 + 1) * DAY + 12 * HOUR + ds * 60_000;
    let segs = load_segs(store, set, from, to)?;
    let hrs = load_hours(store, set, from, to)?;
    let mut by_day: BTreeMap<i64, Vec<(&Seg, i64, i64)>> = BTreeMap::new();
    for s in &segs {
        for (d, a, b) in split_days(s, ds) {
            if (d0..=d1).contains(&d) {
                by_day.entry(d).or_default().push((s, a, b));
            }
        }
    }
    let mut hby: BTreeMap<i64, Vec<&HourRec>> = BTreeMap::new();
    for h in &hrs {
        let d = local_day(h.hour, h.tz, ds);
        if (d0..=d1).contains(&d) {
            hby.entry(d).or_default().push(h);
        }
    }
    let mut out = BTreeMap::new();
    for d in d0..=d1 {
        let p = by_day.get(&d).map(Vec::as_slice).unwrap_or(&[]);
        let h = hby.get(&d).map(Vec::as_slice).unwrap_or(&[]);
        out.insert(d, build_day(p, h, set));
    }
    Ok(out)
}

// ---- engine: cache + views ------------------------------------------------

pub struct Engine<'a> {
    pub store: &'a Store,
    pub set: ChronoSettings,
    /// day start, minutes after local midnight
    pub ds: i64,
    /// current local offset (minutes) for "today"
    pub tz_now: i32,
    /// Android packages uninstalled on every device that had them → uninstall time
    pub gone: HashMap<String, i64>,
}

/// Packages that are uninstalled everywhere (from `app_installs`) → latest uninstall time.
pub fn uninstalled(store: &Store) -> HashMap<String, i64> {
    let mut inst: HashSet<String> = HashSet::new();
    let mut gone: HashMap<String, i64> = HashMap::new();
    for r in store.list(crate::android::STATES, 100_000, 0).unwrap_or_default() {
        let p = r.data["package"].as_str().unwrap_or_default().to_string();
        if r.data["installed"].as_bool() == Some(true) {
            inst.insert(p);
        } else if let Some(t) = r.data["uninstalled_at"].as_i64() {
            let e = gone.entry(p).or_insert(t);
            *e = (*e).max(t);
        }
    }
    gone.retain(|p, _| !inst.contains(p));
    gone
}

impl<'a> Engine<'a> {
    pub fn new(store: &'a Store, set: ChronoSettings, ds: i64, tz_now: i32) -> Self {
        Self { store, set, ds, tz_now, gone: uninstalled(store) }
    }

    pub fn gone_at(&self, package: Option<&str>) -> Option<i64> {
        package.and_then(|p| self.gone.get(p).copied())
    }

    pub fn today(&self, now: i64) -> i64 {
        local_day(now, self.tz_now, self.ds)
    }

    fn kv_i64(&self, k: &str) -> i64 {
        self.store.kv_get(k).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    /// Drop cached days touched by records changed since the last call, or
    /// everything when the exclude lists / day start changed.
    pub fn refresh_cache(&self) -> Result<()> {
        let c = self.store.conn();
        let key = format!("{CACHE_VERSION}|{}", self.set.cache_key(self.ds as u16));
        let max: i64 = c.query_row("SELECT COALESCE(MAX(lseq), 0) FROM records", [], |r| r.get(0))?;
        if self.store.kv_get("chrono.cache.key")?.as_deref() != Some(key.as_str()) {
            c.execute("DELETE FROM chrono_daily", [])?;
            self.store.kv_set("chrono.cache.key", &key)?;
            self.store.kv_set("chrono.cache.lseq", &max.to_string())?;
            return Ok(());
        }
        let last = self.kv_i64("chrono.cache.lseq");
        if max <= last {
            return Ok(());
        }
        let mut st = c.prepare(
            "SELECT collection, data FROM records WHERE lseq > ?1
               AND collection IN ('app_sessions', 'web_sessions', 'android_events')",
        )?;
        let rows = st.query_map([last], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut days: HashSet<i64> = HashSet::new();
        for row in rows {
            let (col, data) = row?;
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) else { continue };
            let tz = v.get("tz_offset").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
            let (s, e) = if col == "android_events" {
                let h = v.get("hour").and_then(|x| x.as_i64()).unwrap_or(0);
                (h, h + 1)
            } else {
                (v.get("start").and_then(|x| x.as_i64()).unwrap_or(0), v.get("end").and_then(|x| x.as_i64()).unwrap_or(0))
            };
            let (d0, d1) = (local_day(s, tz, self.ds), local_day(e.max(s), tz, self.ds));
            for d in d0..=d1.min(d0 + 2) {
                days.insert(d);
            }
        }
        drop(st);
        let mut del = c.prepare("DELETE FROM chrono_daily WHERE day = ?1")?;
        for d in days {
            del.execute([d])?;
        }
        self.store.kv_set("chrono.cache.lseq", &max.to_string())?;
        Ok(())
    }

    /// Aggregates for local days [d0, d1], from the cache where possible.
    pub fn days(&self, d0: i64, d1: i64, now: i64) -> Result<BTreeMap<i64, DayAgg>> {
        self.refresh_cache()?;
        let today = self.today(now);
        let mut out = BTreeMap::new();
        {
            let mut st = self.store.conn().prepare_cached("SELECT day, data FROM chrono_daily WHERE day BETWEEN ?1 AND ?2")?;
            let rows = st.query_map([d0, d1], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (d, data) = row?;
                if let Ok(a) = serde_json::from_str::<DayAgg>(&data) {
                    out.insert(d, a);
                }
            }
        }
        let missing: Vec<i64> = (d0..=d1).filter(|d| !out.contains_key(d) && *d <= today).collect();
        // compute contiguous runs in chunks of <= 31 days
        let mut i = 0;
        while i < missing.len() {
            let a = missing[i];
            let mut j = i;
            while j + 1 < missing.len() && missing[j + 1] == missing[j] + 1 && missing[j + 1] - a < 31 {
                j += 1;
            }
            let b = missing[j];
            let got = compute_days(self.store, &self.set, self.ds, a, b)?;
            let mut ins = self.store.conn().prepare_cached("INSERT OR REPLACE INTO chrono_daily(day, data) VALUES (?1, ?2)")?;
            for (d, agg) in got {
                ins.execute(params![d, serde_json::to_string(&agg)?])?;
                out.insert(d, agg);
            }
            i = j + 1;
        }
        for d in d0..=d1 {
            out.entry(d).or_insert_with(|| DayAgg { hours: vec![0; 24], ..Default::default() });
        }
        Ok(out)
    }

    /// First local day with any data (None = no data at all).
    pub fn first_day(&self) -> Result<Option<i64>> {
        let c = self.store.conn();
        let mut best: Option<(i64, i32)> = None;
        for (col, path) in [("app_sessions", "$.start"), ("web_sessions", "$.start"), ("android_events", "$.hour")] {
            let sql = format!(
                "SELECT json_extract(data, '{path}') AS t, json_extract(data, '$.tz_offset') FROM records
                 WHERE collection = ?1 AND deleted = 0 AND json_extract(data, '{path}') IS NOT NULL ORDER BY json_extract(data, '{path}') LIMIT 1"
            );
            let r: Option<(i64, Option<i64>)> = c
                .query_row(&sql, [col], |r| Ok((r.get(0)?, r.get(1)?)))
                .ok();
            if let Some((t, tz)) = r
                && best.is_none_or(|b| t < b.0)
            {
                best = Some((t, tz.unwrap_or(0) as i32));
            }
        }
        Ok(best.map(|(t, tz)| local_day(t, tz, self.ds)))
    }

    // ---- derived values -------------------------------------------------

    /// Static category of an item (without time rules).
    pub fn item_cat(&self, it: &Item) -> String {
        static_category(&self.set, it.k, &it.n, it.pk.as_deref())
    }

    /// An item's time per category: rule shares, the rest in its static category.
    pub fn item_cats(&self, it: &Item, out: &mut BTreeMap<String, i64>) {
        let ruled: i64 = it.rc.values().sum();
        if it.ms > ruled {
            *out.entry(self.item_cat(it)).or_default() += it.ms - ruled;
        }
        for (c, v) in &it.rc {
            *out.entry(c.clone()).or_default() += v;
        }
    }

    /// Category totals. Browser app time is replaced by the website time
    /// recorded on the same device; any browser remainder counts as `other`.
    pub fn cats(&self, a: &DayAgg) -> BTreeMap<String, i64> {
        let mut c: BTreeMap<String, i64> = BTreeMap::new();
        let mut browser: HashMap<&str, i64> = HashMap::new();
        let mut web: HashMap<&str, i64> = HashMap::new();
        for it in &a.items {
            if it.k == 'a' && (is_browser(&it.n) || it.pk.as_deref().is_some_and(is_browser)) {
                *browser.entry(&it.d).or_default() += it.ms;
                continue;
            }
            if it.k == 'w' {
                *web.entry(&it.d).or_default() += it.ms;
            }
            self.item_cats(it, &mut c);
        }
        for (d, b) in browser {
            let rest = b - web.get(d).copied().unwrap_or(0);
            if rest > 0 {
                *c.entry("other".into()).or_default() += rest;
            }
        }
        c
    }

    pub fn device_name(&self, id: &str, platforms: &[String]) -> String {
        if let Some(n) = self.set.device_names.get(id) {
            return n.clone();
        }
        let base = if platforms.iter().any(|p| p == "android") {
            "Phone"
        } else if platforms.iter().any(|p| p == "windows") {
            "Windows PC"
        } else if platforms.iter().any(|p| p == "linux") {
            "Linux desktop"
        } else if platforms.iter().any(|p| p == "chrome" || p == "firefox") {
            "Browser"
        } else {
            "Device"
        };
        let short: String = id.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        let _ = short;
        base.to_string()
    }

    /// Used minutes for a goal on one day.
    pub fn goal_used(&self, g: &crate::settings::Goal, a: &DayAgg) -> i64 {
        match g.scope.as_str() {
            "total" => a.screen,
            "category" => self.cats(a).get(&g.target).copied().unwrap_or(0),
            "app" => a
                .items
                .iter()
                .filter(|i| i.k == 'a' && (i.n.eq_ignore_ascii_case(&g.target) || i.pk.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(&g.target))))
                .map(|i| i.ms)
                .sum(),
            "domain" => a.items.iter().filter(|i| i.k == 'w' && crate::settings::domain_matches(&i.n, &g.target)).map(|i| i.ms).sum(),
            _ => 0,
        }
    }
}

// ---- view types -----------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Total {
    pub kind: char,
    pub name: String,
    pub package: Option<String>,
    pub ms: i64,
    pub opens: i64,
    pub cat: String,
    pub platforms: Vec<String>,
    /// previous period, same length (for deltas)
    pub prev: i64,
    /// Android app uninstalled (everywhere) at this time
    pub uninstalled: Option<i64>,
    /// Part of `ms` that time rules put in another category: category → ms.
    pub rules: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceTotal {
    pub id: String,
    pub name: String,
    pub platforms: Vec<String>,
    pub ms: i64,
    /// includes browser (website) time
    pub web: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoalStatus {
    pub goal: crate::settings::Goal,
    pub used: i64,
    /// "ok" | "near" | "over" (limits) / "todo" | "met" (goals)
    pub state: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub screen: i64,
    pub days: i64,
    pub active_days: i64,
    pub avg: i64,
    pub unlocks: i64,
    pub pickups: i64,
    pub notifications: i64,
    pub longest: Option<(String, i64, i64)>,
    pub focus: Option<i64>,
    pub cats: BTreeMap<String, i64>,
    pub sessions: i64,
}

impl Engine<'_> {
    pub fn summarize<'b>(&self, it: impl Iterator<Item = &'b DayAgg>) -> Summary {
        let mut s = Summary::default();
        for a in it {
            s.days += 1;
            if a.screen > 0 {
                s.active_days += 1;
            }
            s.screen += a.screen;
            s.unlocks += a.unlocks;
            s.pickups += a.pickups;
            s.sessions += a.sessions;
            s.notifications += a.notif.values().sum::<i64>();
            if let Some(l) = &a.longest
                && s.longest.as_ref().is_none_or(|x| l.1 > x.1)
            {
                s.longest = Some(l.clone());
            }
            for (k, v) in self.cats(a) {
                *s.cats.entry(k).or_default() += v;
            }
        }
        s.avg = if s.active_days > 0 { s.screen / s.active_days } else { 0 };
        s.focus = focus_score(&s.cats);
        s
    }

    /// Totals per (kind, name) over the days, with previous-period values.
    pub fn totals<'b>(&self, cur: impl Iterator<Item = &'b DayAgg>, prev: impl Iterator<Item = &'b DayAgg>) -> Vec<Total> {
        let mut m: HashMap<(char, String), Total> = HashMap::new();
        for a in cur {
            for it in &a.items {
                let key = (it.k, it.n.clone());
                let t = m.entry(key).or_insert_with(|| Total {
                    kind: it.k,
                    name: it.n.clone(),
                    package: it.pk.clone(),
                    ms: 0,
                    opens: 0,
                    cat: self.item_cat(it),
                    platforms: vec![],
                    prev: 0,
                    uninstalled: self.gone_at(it.pk.as_deref()),
                    rules: BTreeMap::new(),
                });
                t.ms += it.ms;
                for (c, v) in &it.rc {
                    *t.rules.entry(c.clone()).or_default() += v;
                }
                t.opens += it.opens;
                if !t.platforms.contains(&it.p) {
                    t.platforms.push(it.p.clone());
                }
            }
        }
        for a in prev {
            for it in &a.items {
                if let Some(t) = m.get_mut(&(it.k, it.n.clone())) {
                    t.prev += it.ms;
                }
            }
        }
        let mut v: Vec<Total> = m.into_values().collect();
        v.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.name.cmp(&b.name)));
        v
    }

    pub fn devices<'b>(&self, it: impl Iterator<Item = &'b DayAgg>) -> Vec<DeviceTotal> {
        let mut m: BTreeMap<String, (i64, Vec<String>)> = BTreeMap::new();
        for a in it {
            for (d, ms) in &a.dev {
                let e = m.entry(d.clone()).or_default();
                e.0 += ms;
                for p in a.devp.get(d).into_iter().flatten() {
                    if !e.1.contains(p) {
                        e.1.push(p.clone());
                    }
                }
            }
        }
        let mut v: Vec<DeviceTotal> =
            m.into_iter()
                .map(|(id, (ms, platforms))| DeviceTotal {
                    name: self.device_name(&id, &platforms),
                    web: platforms.iter().any(|p| p == "chrome" || p == "firefox"),
                    id,
                    platforms,
                    ms,
                })
                .collect();
        v.sort_by(|a, b| b.ms.cmp(&a.ms));
        v
    }

    pub fn goals(&self, a: &DayAgg, day: i64) -> Vec<GoalStatus> {
        self.set
            .goals
            .iter()
            .filter(|g| g.enabled && g.first_day().is_none_or(|f| day >= f))
            .map(|g| {
                let used = self.goal_used(g, a);
                let lim = g.minutes * 60_000;
                let state = if g.kind == "limit" {
                    if used >= lim { "over" } else if used * 10 >= lim * 8 { "near" } else { "ok" }
                } else if used >= lim {
                    "met"
                } else {
                    "todo"
                };
                GoalStatus { goal: g.clone(), used, state: state.into() }
            })
            .collect()
    }
}

// ---- screens --------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DayView {
    pub day: String,
    pub today: bool,
    pub screen: i64,
    pub unlocks: i64,
    pub pickups: i64,
    pub notifications: i64,
    pub first: Option<i64>,
    pub last: Option<i64>,
    pub longest: Option<(String, i64, i64)>,
    pub hours: Vec<i64>,
    pub cats: BTreeMap<String, i64>,
    pub focus: Option<i64>,
    pub top: Vec<Total>,
    pub devices: Vec<DeviceTotal>,
    pub goals: Vec<GoalStatus>,
    /// average screen time of the previous 7 active days
    pub week_avg: i64,
    /// this week so far vs the same days last week
    pub week_so_far: i64,
    pub last_week_same: i64,
    /// merged combined usage intervals [start, end] for the day strip
    pub strip: Vec<(i64, i64)>,
    pub day_start: i64,
    pub day_end: i64,
    pub notif_top: Vec<(String, i64)>,
    /// name of the phone that reported unlocks/pickups (None = no phone data)
    pub phone: Option<String>,
    pub note: Option<crate::notes::DayNote>,
}

pub fn day_view(e: &Engine, day: i64, now: i64) -> Result<DayView> {
    let ws = day - weekday(day) as i64;
    let days = e.days(ws - 7, day, now)?;
    let a = days.get(&day).cloned().unwrap_or_default();
    let prev7: Vec<&DayAgg> = (day - 7..day).filter_map(|d| days.get(&d)).filter(|a| a.screen > 0).collect();
    let week_avg = if prev7.is_empty() { 0 } else { prev7.iter().map(|a| a.screen).sum::<i64>() / prev7.len() as i64 };
    let week_so_far = (ws..=day).filter_map(|d| days.get(&d)).map(|a| a.screen).sum();
    let last_week_same = (ws - 7..=day - 7).filter_map(|d| days.get(&d)).map(|a| a.screen).sum();
    let cats = e.cats(&a);
    let ds0 = day_start_utc(day, e.tz_now, e.ds);
    let segs = load_segs(e.store, &e.set, ds0, ds0 + DAY)?;
    let mut iv: Vec<(i64, i64)> = segs.iter().map(|s| (s.start.max(ds0), s.end.min(ds0 + DAY))).filter(|(a, b)| b > a).collect();
    let strip = merge(&mut iv);
    let mut notif_top: Vec<(String, i64)> = a.notif.iter().map(|(k, v)| (k.clone(), *v)).collect();
    notif_top.sort_by(|x, y| y.1.cmp(&x.1));
    notif_top.truncate(8);
    for n in &mut notif_top {
        n.0 = package_label(e.store, &n.0);
    }
    let top = e.totals(std::iter::once(&a), std::iter::empty());
    let devices = e.devices(std::iter::once(&a));
    let phone = if a.unlocks + a.pickups + a.notif.values().sum::<i64>() > 0 {
        Some(devices.iter().find(|d| d.platforms.iter().any(|p| p == "android")).map(|d| d.name.clone()).unwrap_or_else(|| "Phone".into()))
    } else {
        None
    };
    let note = crate::notes::get(e.store, &day_to_str(day))?;
    Ok(DayView {
        day: day_to_str(day),
        today: day == e.today(now),
        screen: a.screen,
        unlocks: a.unlocks,
        pickups: a.pickups,
        notifications: a.notif.values().sum(),
        first: a.first,
        last: a.last,
        longest: a.longest.clone(),
        hours: a.hours.clone(),
        focus: focus_score(&cats),
        cats,
        top,
        devices,
        goals: e.goals(&a, day),
        week_avg,
        week_so_far,
        last_week_same,
        strip,
        day_start: ds0,
        day_end: ds0 + DAY,
        notif_top,
        phone,
        note,
    })
}

#[derive(Debug, Serialize)]
pub struct Lane {
    pub device: String,
    pub name: String,
    /// "phone" | "pc" | "browser"
    pub kind: String,
    pub segs: Vec<(i64, i64, String, String)>,
    pub ms: i64,
}

#[derive(Debug, Serialize)]
pub struct TimelineView {
    pub day: String,
    pub from: i64,
    pub to: i64,
    pub lanes: Vec<Lane>,
    /// newest first
    pub sessions: Vec<SessionRow>,
}

#[derive(Debug, Serialize)]
pub struct SessionRow {
    pub kind: char,
    pub name: String,
    /// "Browser" for websites, else the device name
    pub device: String,
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub start: i64,
    pub end: i64,
    pub cat: String,
    pub uninstalled: Option<i64>,
}

pub fn timeline(e: &Engine, day: i64) -> Result<TimelineView> {
    let from = day_start_utc(day, e.tz_now, e.ds);
    let to = from + DAY;
    let segs = load_segs(e.store, &e.set, from, to)?;
    let mut lanes: BTreeMap<(String, String), Lane> = BTreeMap::new();
    let mut sessions = Vec::new();
    let mut plats: HashMap<String, Vec<String>> = HashMap::new();
    for s in &segs {
        let p = plats.entry(s.device.clone()).or_default();
        if !p.contains(&s.platform) {
            p.push(s.platform.clone());
        }
    }
    for s in &segs {
        let (a, b) = (s.start.max(from), s.end.min(to));
        if b <= a {
            continue;
        }
        // Blocks change colour where a time rule starts or ends; the session
        // row takes the category of its longest part.
        let parts = category_pieces(&e.set, s.kind, &s.name, s.package.as_deref(), a, b, s.tz);
        let cat = parts.iter().max_by_key(|p| p.1 - p.0).map(|p| p.2.clone()).unwrap_or_default();
        let kind = if s.kind == 'w' { "browser" } else if s.platform == "android" { "phone" } else { "pc" };
        let lane = lanes.entry((s.device.clone(), kind.to_string())).or_insert_with(|| Lane {
            device: s.device.clone(),
            name: {
                let dn = e.device_name(&s.device, &plats[&s.device]);
                if kind == "browser" { format!("Browser on {dn}") } else { dn }
            },
            kind: kind.into(),
            segs: vec![],
            ms: 0,
        });
        for (x, y, c) in parts {
            lane.segs.push((x, y, s.name.clone(), c));
        }
        lane.ms += b - a;
        let dev_name = e.device_name(&s.device, &plats[&s.device]);
        sessions.push(SessionRow {
            kind: s.kind,
            name: s.name.clone(),
            device: if s.kind == 'w' { "Browser".into() } else { dev_name.clone() },
            device_id: s.device.clone(),
            device_name: dev_name,
            platform: s.platform.clone(),
            start: a,
            end: b,
            cat,
            uninstalled: e.gone_at(s.package.as_deref()),
        });
    }
    sessions.sort_by(|x, y| y.start.cmp(&x.start));
    let order = |k: &str| match k {
        "phone" => 0,
        "pc" => 1,
        _ => 2,
    };
    let mut lanes: Vec<Lane> = lanes.into_values().collect();
    // phones first, then each PC followed by its browser lane
    let rank = |l: &Lane| -> (u8, String, i32) {
        let phone = plats.get(&l.device).is_some_and(|p| p.iter().any(|x| x == "android"));
        (if phone { 0 } else { 1 }, e.device_name(&l.device, &plats[&l.device]), order(&l.kind))
    };
    lanes.sort_by_key(rank);
    Ok(TimelineView { day: day_to_str(day), from, to, lanes, sessions })
}

#[derive(Debug, Serialize)]
pub struct Bucket {
    pub label: String,
    pub from: String,
    pub ms: i64,
    pub focus_ms: i64,
    pub days: i64,
}

#[derive(Debug, Serialize)]
pub struct TrendsView {
    pub period: String,
    pub from: String,
    pub to: String,
    pub title: String,
    pub buckets: Vec<Bucket>,
    pub summary: Summary,
    pub prev: Summary,
    pub apps: Vec<Total>,
    pub sites: Vec<Total>,
    pub devices: Vec<DeviceTotal>,
    /// [weekday Mon=0][hour] average ms per day
    pub heatmap: Vec<Vec<i64>>,
    pub heat_from: String,
    pub heat_to: String,
    pub this_week: i64,
    pub last_week: i64,
    pub has_prev: bool,
    pub prev_anchor: String,
    pub next_anchor: Option<String>,
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WD: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Range [d0, d1] for a period containing `anchor`, plus the previous range.
pub fn period_range(period: &str, anchor: i64, first: i64) -> ((i64, i64), (i64, i64)) {
    let dt = date_of(anchor);
    match period {
        "day" => ((anchor, anchor), (anchor - 1, anchor - 1)),
        "week" => {
            let s = anchor - weekday(anchor) as i64;
            ((s, s + 6), (s - 7, s - 1))
        }
        "month" => {
            let s = day_of_date(dt.with_day(1).unwrap());
            let next = if dt.month() == 12 { NaiveDate::from_ymd_opt(dt.year() + 1, 1, 1) } else { NaiveDate::from_ymd_opt(dt.year(), dt.month() + 1, 1) };
            let e = day_of_date(next.unwrap()) - 1;
            let pm = date_of(s - 1).with_day(1).unwrap();
            ((s, e), (day_of_date(pm), s - 1))
        }
        "year" => {
            let s = day_of_date(NaiveDate::from_ymd_opt(dt.year(), 1, 1).unwrap());
            let e = day_of_date(NaiveDate::from_ymd_opt(dt.year() + 1, 1, 1).unwrap()) - 1;
            let ps = day_of_date(NaiveDate::from_ymd_opt(dt.year() - 1, 1, 1).unwrap());
            ((s, e), (ps, s - 1))
        }
        _ => ((first.min(anchor), anchor), (0, -1)),
    }
}

pub fn trends(e: &Engine, period: &str, anchor: i64, now: i64) -> Result<TrendsView> {
    let today = e.today(now);
    let first = e.first_day()?.unwrap_or(today);
    let ((d0, d1), (p0, p1)) = period_range(period, anchor, first);
    // heatmap needs at least 4 weeks to say anything
    let (h0, h1) = (d0.min(d1.min(today) - 27), d1.min(today));
    let ws = today - weekday(today) as i64;
    let mut lo = d0.min(h0).min(ws - 7);
    if p1 >= p0 {
        lo = lo.min(p0);
    }
    // nothing exists before the first day with data
    let lo = lo.max(first.min(today));
    let days = e.days(lo, today.max(lo), now)?;
    let get = |d: i64| days.get(&d);
    let cur: Vec<&DayAgg> = (d0..=d1.min(today)).filter_map(get).collect();
    let prev: Vec<&DayAgg> = if p1 >= p0 { (p0..=p1).filter_map(get).collect() } else { vec![] };

    let mut buckets = Vec::new();
    let focus_of = |a: &DayAgg| -> i64 { e.cats(a).iter().filter(|(k, _)| crate::categories::FOCUS.contains(&k.as_str())).map(|(_, v)| v).sum() };
    match period {
        "day" => {
            let a = get(d0).cloned().unwrap_or_default();
            for (h, ms) in a.hours.iter().enumerate() {
                buckets.push(Bucket { label: format!("{h}"), from: day_to_str(d0), ms: *ms, focus_ms: 0, days: 1 });
            }
        }
        "week" | "month" => {
            for d in d0..=d1 {
                let a = get(d);
                let label = if period == "week" { WD[weekday(d)].to_string() } else { date_of(d).day().to_string() };
                buckets.push(Bucket {
                    label,
                    from: day_to_str(d),
                    ms: a.map(|a| a.screen).unwrap_or(0),
                    focus_ms: a.map(focus_of).unwrap_or(0),
                    days: 1,
                });
            }
        }
        _ => {
            // months (year, or lifetime ≤ 36 months) or years
            let (fy, fm) = (date_of(d0).year(), date_of(d0).month());
            let (ly, lm) = (date_of(d1).year(), date_of(d1).month());
            let n_months = (ly - fy) * 12 + lm as i32 - fm as i32 + 1;
            if period == "lifetime" && n_months > 36 {
                for y in fy..=ly {
                    let s = day_of_date(NaiveDate::from_ymd_opt(y, 1, 1).unwrap()).max(d0);
                    let en = (day_of_date(NaiveDate::from_ymd_opt(y + 1, 1, 1).unwrap()) - 1).min(d1);
                    let v: Vec<&DayAgg> = (s..=en).filter_map(get).collect();
                    buckets.push(Bucket {
                        label: y.to_string(),
                        from: day_to_str(s),
                        ms: v.iter().map(|a| a.screen).sum(),
                        focus_ms: v.iter().map(|a| focus_of(a)).sum(),
                        days: v.iter().filter(|a| a.screen > 0).count() as i64,
                    });
                }
            } else {
                let (mut y, mut m) = (fy, fm);
                for _ in 0..n_months {
                    let s = day_of_date(NaiveDate::from_ymd_opt(y, m, 1).unwrap()).max(d0);
                    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
                    let en = (day_of_date(NaiveDate::from_ymd_opt(ny, nm, 1).unwrap()) - 1).min(d1);
                    let v: Vec<&DayAgg> = (s..=en).filter_map(get).collect();
                    let label = if period == "lifetime" && (m == 1 || buckets.is_empty()) {
                        format!("{} {}", MONTHS[m as usize - 1], y % 100)
                    } else {
                        MONTHS[m as usize - 1].to_string()
                    };
                    buckets.push(Bucket {
                        label,
                        from: day_to_str(s),
                        ms: v.iter().map(|a| a.screen).sum(),
                        focus_ms: v.iter().map(|a| focus_of(a)).sum(),
                        days: v.iter().filter(|a| a.screen > 0).count() as i64,
                    });
                    (y, m) = (ny, nm);
                }
            }
        }
    }

    // heatmap: average per weekday/hour over [h0, h1]
    let mut heat = vec![vec![0i64; 24]; 7];
    let mut cnt = [0i64; 7];
    for d in h0..=h1 {
        if d < first {
            continue;
        }
        let w = weekday(d);
        cnt[w] += 1;
        if let Some(a) = get(d) {
            for (h, ms) in a.hours.iter().enumerate() {
                heat[w][h] += ms;
            }
        }
    }
    for w in 0..7 {
        if cnt[w] > 0 {
            for h in 0..24 {
                heat[w][h] /= cnt[w];
            }
        }
    }
    let totals = e.totals(cur.iter().copied(), prev.iter().copied());
    let (apps, sites): (Vec<Total>, Vec<Total>) = totals.into_iter().partition(|t| t.kind == 'a');
    let this_week = (ws..=today).filter_map(get).map(|a| a.screen).sum();
    let last_week = (ws - 7..=today - 7).filter_map(get).map(|a| a.screen).sum();
    let title = match period {
        "day" => date_of(d0).format("%A, %-d %B %Y").to_string(),
        "week" => format!("{} – {}", date_of(d0).format("%-d %b"), date_of(d1).format("%-d %b %Y")),
        "month" => date_of(d0).format("%B %Y").to_string(),
        "year" => date_of(d0).format("%Y").to_string(),
        _ => format!("Since {}", date_of(d0).format("%-d %b %Y")),
    };
    let next = match period {
        "day" => d1 + 1,
        "lifetime" => today + 1,
        _ => d1 + 1,
    };
    Ok(TrendsView {
        period: period.into(),
        from: day_to_str(d0),
        to: day_to_str(d1),
        title,
        buckets,
        summary: e.summarize(cur.iter().copied()),
        prev: e.summarize(prev.iter().copied()),
        apps,
        sites,
        devices: e.devices(cur.iter().copied()),
        heatmap: heat,
        heat_from: day_to_str(h0.max(first)),
        heat_to: day_to_str(h1),
        this_week,
        last_week,
        has_prev: p1 >= p0 && p1 >= first,
        prev_anchor: day_to_str(if p1 >= p0 { p1 } else { d0 }),
        next_anchor: (next <= today && period != "lifetime").then(|| day_to_str(next)),
    })
}

#[derive(Debug, Serialize)]
pub struct ReportDay {
    pub day: String,
    pub label: String,
    pub ms: i64,
    pub prev_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct GoalWeek {
    pub goal: crate::settings::Goal,
    pub days_ok: i64,
    pub days: i64,
    pub avg_used: i64,
    /// Mon..Sun: Some(kept) for past/current days, None for future days
    pub kept: Vec<Option<bool>>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub week_start: String,
    pub week_end: String,
    pub title: String,
    pub cur: Summary,
    pub prev: Summary,
    pub days: Vec<ReportDay>,
    pub apps: Vec<Total>,
    pub sites: Vec<Total>,
    pub devices: Vec<DeviceTotal>,
    pub busiest: Option<(String, i64)>,
    pub quietest: Option<(String, i64)>,
    /// clock hour with most use (avg over the week)
    pub peak_hour: Option<usize>,
    pub goals: Vec<GoalWeek>,
    pub prev_week: Option<String>,
    pub next_week: Option<String>,
    pub complete: bool,
    /// longest single session: (name, ms, day)
    pub longest: Option<(String, i64, String)>,
    /// days whose last use was at or after 23:00 local
    pub late_days: i64,
    pub notif_top: Option<(String, i64)>,
    /// days of this week that count for averages (from the first day with data, up to today)
    pub days_counted: i64,
}

pub fn report(e: &Engine, anchor: i64, now: i64) -> Result<Report> {
    let today = e.today(now);
    let ws = anchor - weekday(anchor) as i64;
    let first = e.first_day()?.unwrap_or(today);
    let days = e.days(ws - 7, ws + 6, now)?;
    let get = |d: i64| days.get(&d);
    let upto = (ws + 6).min(today);
    let cur: Vec<&DayAgg> = (ws..=upto).filter_map(get).collect();
    let prev: Vec<&DayAgg> = (ws - 7..ws).filter_map(get).collect();
    let rdays = (0..7)
        .map(|i| ReportDay {
            day: day_to_str(ws + i),
            label: WD[i as usize].into(),
            ms: if ws + i <= today { get(ws + i).map(|a| a.screen).unwrap_or(0) } else { 0 },
            prev_ms: get(ws - 7 + i).map(|a| a.screen).unwrap_or(0),
        })
        .collect::<Vec<_>>();
    let active: Vec<&ReportDay> = rdays.iter().filter(|d| d.ms > 0).collect();
    let busiest = active.iter().max_by_key(|d| d.ms).map(|d| (d.day.clone(), d.ms));
    let quietest = active.iter().min_by_key(|d| d.ms).map(|d| (d.day.clone(), d.ms));
    let mut hours = [0i64; 24];
    for a in &cur {
        for (h, ms) in a.hours.iter().enumerate() {
            hours[h] += ms;
        }
    }
    let peak_hour = hours.iter().enumerate().max_by_key(|(_, v)| **v).filter(|(_, v)| **v > 0).map(|(h, _)| h);
    let goals = e
        .set
        .goals
        .iter()
        .filter(|g| g.enabled)
        .map(|g| {
            let mut ok = 0;
            let mut n = 0;
            let mut used = 0;
            let mut kept = vec![None; 7];
            for i in 0..7 {
                let d = ws + i as i64;
                if d > today || d < first || g.first_day().is_some_and(|f| d < f) {
                    continue;
                }
                let a = get(d).cloned().unwrap_or_default();
                n += 1;
                let u = e.goal_used(g, &a);
                used += u;
                let lim = g.minutes * 60_000;
                let k = (g.kind == "limit" && u <= lim) || (g.kind != "limit" && u >= lim);
                if k {
                    ok += 1;
                }
                kept[i] = Some(k);
            }
            GoalWeek { goal: g.clone(), days_ok: ok, days: n, avg_used: if n > 0 { used / n } else { 0 }, kept }
        })
        .collect();
    let totals = e.totals(cur.iter().copied(), prev.iter().copied());
    let (apps, sites): (Vec<Total>, Vec<Total>) = totals.into_iter().partition(|t| t.kind == 'a');
    let mut longest: Option<(String, i64, String)> = None;
    let mut late_days = 0;
    let mut notif: BTreeMap<String, i64> = BTreeMap::new();
    for d in ws..=upto {
        let Some(a) = get(d) else { continue };
        if let Some(l) = &a.longest
            && longest.as_ref().is_none_or(|x| l.1 > x.1)
        {
            longest = Some((l.0.clone(), l.1, day_to_str(d)));
        }
        if let Some(last) = a.last {
            // clock hour of the last use (minus 1 ms so a session ending at 24:00 counts as 23)
            let h = (last - 1 + e.tz_now as i64 * 60_000).rem_euclid(DAY) / HOUR;
            if h >= 23 || h < e.ds / 60 {
                late_days += 1;
            }
        }
        for (k, v) in &a.notif {
            *notif.entry(k.clone()).or_default() += v;
        }
    }
    let notif_top = notif.into_iter().max_by_key(|x| x.1).map(|(pkg, n)| (package_label(e.store, &pkg), n));
    Ok(Report {
        week_start: day_to_str(ws),
        week_end: day_to_str(ws + 6),
        title: format!("{} – {}", date_of(ws).format("%-d %b"), date_of(ws + 6).format("%-d %b %Y")),
        cur: e.summarize(cur.iter().copied()),
        prev: e.summarize(prev.iter().copied()),
        days: rdays,
        apps: apps.into_iter().take(8).collect(),
        sites: sites.into_iter().take(8).collect(),
        devices: e.devices(cur.iter().copied()),
        busiest,
        quietest,
        peak_hour,
        goals,
        prev_week: (ws - 1 >= first).then(|| day_to_str(ws - 7)),
        next_week: (ws + 7 <= today).then(|| day_to_str(ws + 7)),
        complete: ws + 6 < today,
        longest,
        late_days,
        notif_top,
        days_counted: (upto - ws.max(first) + 1).max(1),
    })
}

#[derive(Debug, Serialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub platforms: Vec<String>,
    /// end of the newest session from this device (UTC ms)
    pub last_seen: i64,
    /// screen time in the last 30 days
    pub ms_30d: i64,
    pub this_device: bool,
    pub browsers: Vec<String>,
    /// phone counters (unlocks/pickups) seen in the last 30 days
    pub phone_counters: bool,
}

/// Devices and data sources seen in the last 30 days, newest first.
pub fn sources(e: &Engine, now: i64) -> Result<Vec<Source>> {
    let from = now - 30 * DAY;
    let segs = load_segs(e.store, &e.set, from, now + DAY)?;
    let mut m: BTreeMap<String, Source> = BTreeMap::new();
    for sg in &segs {
        let src = m.entry(sg.device.clone()).or_insert_with(|| Source {
            id: sg.device.clone(),
            name: String::new(),
            platforms: vec![],
            last_seen: 0,
            ms_30d: 0,
            this_device: sg.device == e.store.device_id(),
            browsers: vec![],
            phone_counters: false,
        });
        if !src.platforms.contains(&sg.platform) {
            src.platforms.push(sg.platform.clone());
        }
        if sg.kind == 'w' && !src.browsers.contains(&sg.platform) {
            src.browsers.push(sg.platform.clone());
        }
        src.last_seen = src.last_seen.max(sg.end);
        if sg.kind == 'a' {
            src.ms_30d += sg.end - sg.start.max(from);
        }
    }
    let mut st = e.store.conn().prepare_cached(
        "SELECT json_extract(data, '$.device'), MAX(json_extract(data, '$.hour')) FROM records
         WHERE collection = 'android_events' AND deleted = 0 AND json_extract(data, '$.hour') >= ?1 GROUP BY 1",
    )?;
    let rows = st.query_map([from], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (dev, h) = row?;
        let Some(dev) = dev else { continue };
        let src = m.entry(dev.clone()).or_insert_with(|| Source {
            id: dev.clone(),
            name: String::new(),
            platforms: vec!["android".into()],
            last_seen: 0,
            ms_30d: 0,
            this_device: dev == e.store.device_id(),
            browsers: vec![],
            phone_counters: true,
        });
        src.phone_counters = true;
        src.last_seen = src.last_seen.max((h + HOUR).min(now));
    }
    let mut v: Vec<Source> = m.into_values().collect();
    for s in &mut v {
        s.name = e.device_name(&s.id, &s.platforms);
    }
    v.sort_by(|a, b| b.this_device.cmp(&a.this_device).then(b.last_seen.cmp(&a.last_seen)));
    Ok(v)
}

#[derive(Debug, Serialize)]
pub struct AppChange {
    pub id: String,
    pub package: String,
    pub label: String,
    /// installed | uninstalled | reinstalled | updated
    pub kind: String,
    pub at: i64,
    pub device: String,
    pub device_name: String,
    pub icon: Option<String>,
    /// all recorded use of this app, on every device
    pub lifetime: i64,
    pub installed_now: bool,
}

/// App install/uninstall/reinstall/update events, newest first.
pub fn app_changes(e: &Engine) -> Result<Vec<AppChange>> {
    let recs = e.store.list(crate::android::CHANGES, 100_000, 0)?;
    let pkgs: HashSet<String> = recs.iter().filter_map(|r| r.data["package"].as_str().map(str::to_string)).collect();
    let mut life: HashMap<String, i64> = HashMap::new();
    if !pkgs.is_empty() {
        let mut st = e.store.conn().prepare(
            "SELECT json_extract(data, '$.package'), SUM(json_extract(data, '$.end') - json_extract(data, '$.start'))
             FROM records WHERE collection = 'app_sessions' AND deleted = 0 AND json_extract(data, '$.package') IS NOT NULL GROUP BY 1",
        )?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (p, ms) = row?;
            if pkgs.contains(&p) {
                life.insert(p, ms);
            }
        }
    }
    let mut plats: HashMap<String, Vec<String>> = HashMap::new();
    let mut out: Vec<AppChange> = recs
        .into_iter()
        .map(|r| {
            let d = &r.data;
            let package = d["package"].as_str().unwrap_or_default().to_string();
            let device = d["device"].as_str().unwrap_or_default().to_string();
            let pl = plats.entry(device.clone()).or_insert_with(|| vec!["android".into()]).clone();
            let app = e.store.get(&format!("p-{package}")).ok().flatten();
            AppChange {
                id: r.id.clone(),
                label: d["label"].as_str().unwrap_or(&package).to_string(),
                kind: d["kind"].as_str().unwrap_or_default().to_string(),
                at: d["at"].as_i64().unwrap_or(0),
                device_name: e.device_name(&device, &pl),
                device,
                icon: app.and_then(|a| a.data.get("icon").and_then(|v| v.as_str()).map(str::to_string)),
                lifetime: life.get(&package).copied().unwrap_or(0),
                installed_now: !e.gone.contains_key(&package),
                package,
            }
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at));
    Ok(out)
}

/// App label for an Android package (from `android_apps`), else the package.
pub fn package_label(store: &Store, pkg: &str) -> String {
    store
        .get(&format!("p-{pkg}"))
        .ok()
        .flatten()
        .and_then(|r| r.data.get("label").and_then(|l| l.as_str()).map(str::to_string))
        .unwrap_or_else(|| pkg.to_string())
}

// ---- websites ---------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SiteRow {
    pub name: String,
    pub ms: i64,
    /// previous period of the same length
    pub prev: i64,
    /// static category (settings, defaults)
    pub cat: String,
    /// time that time rules put in another category: category → ms
    pub rules: BTreeMap<String, i64>,
    /// "chrome" | "firefox"
    pub browsers: Vec<String>,
    /// device display names
    pub devices: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct WebsitesView {
    pub period: String,
    pub from: String,
    pub to: String,
    pub title: String,
    pub total: i64,
    pub prev_total: i64,
    /// days counted (up to today) for "a day" averages
    pub days: i64,
    /// website time per category, time rules applied
    pub cats: BTreeMap<String, i64>,
    pub sites: Vec<SiteRow>,
    /// any website session ever (false = show how to add the extension)
    pub has_any: bool,
    pub prev_anchor: Option<String>,
    pub next_anchor: Option<String>,
}

fn has_web(store: &Store) -> Result<bool> {
    Ok(store
        .conn()
        .query_row("SELECT EXISTS(SELECT 1 FROM records WHERE collection = 'web_sessions' AND deleted = 0)", [], |r| r.get::<_, i64>(0))?
        == 1)
}

fn period_title(period: &str, d0: i64, d1: i64) -> String {
    match period {
        "day" => date_of(d0).format("%A, %-d %B").to_string(),
        "week" => format!("{} – {}", date_of(d0).format("%-d %b"), date_of(d1).format("%-d %b %Y")),
        _ => date_of(d0).format("%B %Y").to_string(),
    }
}

/// Websites with time in a day / week / month (`period`) containing `anchor`.
pub fn websites(e: &Engine, period: &str, anchor: i64, now: i64) -> Result<WebsitesView> {
    let period = if matches!(period, "day" | "week" | "month") { period } else { "week" };
    let today = e.today(now);
    let first = e.first_day()?.unwrap_or(today);
    let ((d0, d1), (p0, p1)) = period_range(period, anchor.min(today), first);
    let lo = p0.max(first.min(d0));
    let days = e.days(lo, d1.min(today).max(lo), now)?;
    let cur: Vec<&DayAgg> = (d0..=d1.min(today)).filter_map(|d| days.get(&d)).collect();
    let prev: Vec<&DayAgg> = (p0..=p1).filter_map(|d| days.get(&d)).collect();
    let mut devp: HashMap<String, Vec<String>> = HashMap::new();
    for a in &cur {
        for (d, ps) in &a.devp {
            let v = devp.entry(d.clone()).or_default();
            for p in ps {
                if !v.contains(p) {
                    v.push(p.clone());
                }
            }
        }
    }
    let mut m: HashMap<String, SiteRow> = HashMap::new();
    let mut cats: BTreeMap<String, i64> = BTreeMap::new();
    for a in &cur {
        for it in a.items.iter().filter(|i| i.k == 'w') {
            e.item_cats(it, &mut cats);
            let r = m.entry(it.n.clone()).or_insert_with(|| SiteRow {
                name: it.n.clone(),
                ms: 0,
                prev: 0,
                cat: e.item_cat(it),
                rules: BTreeMap::new(),
                browsers: vec![],
                devices: vec![],
            });
            r.ms += it.ms;
            for (c, v) in &it.rc {
                *r.rules.entry(c.clone()).or_default() += v;
            }
            if !r.browsers.contains(&it.p) {
                r.browsers.push(it.p.clone());
            }
            let dn = e.device_name(&it.d, devp.get(&it.d).map(Vec::as_slice).unwrap_or(&[]));
            if !r.devices.contains(&dn) {
                r.devices.push(dn);
            }
        }
    }
    let mut prev_total = 0;
    for a in &prev {
        for it in a.items.iter().filter(|i| i.k == 'w') {
            prev_total += it.ms;
            if let Some(r) = m.get_mut(&it.n) {
                r.prev += it.ms;
            }
        }
    }
    let mut sites: Vec<SiteRow> = m.into_values().collect();
    sites.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.name.cmp(&b.name)));
    Ok(WebsitesView {
        period: period.into(),
        from: day_to_str(d0),
        to: day_to_str(d1),
        title: period_title(period, d0, d1),
        total: sites.iter().map(|s| s.ms).sum(),
        prev_total,
        days: (d1.min(today) - d0.max(first) + 1).max(1),
        cats,
        sites,
        has_any: has_web(e.store)?,
        prev_anchor: (p1 >= first).then(|| day_to_str(p1)),
        next_anchor: (d1 < today).then(|| day_to_str(d1 + 1)),
    })
}

#[derive(Debug, Serialize)]
pub struct SiteDay {
    pub day: String,
    pub label: String,
    /// category → ms (time rules applied)
    pub cats: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize)]
pub struct SiteDetail {
    pub name: String,
    pub period: String,
    pub title: String,
    pub ms: i64,
    pub prev: i64,
    pub days_counted: i64,
    /// per day of the period (week/month; one entry for a day)
    pub days: Vec<SiteDay>,
    /// local clock hour → ms
    pub hours: Vec<i64>,
    /// category → ms over the period (time rules applied)
    pub cats: BTreeMap<String, i64>,
    /// static category, and whether the user chose it (else a default)
    pub cat: String,
    pub cat_set: bool,
    /// (browser, ms), (device name, ms), longest first
    pub browsers: Vec<(String, i64)>,
    pub devices: Vec<(String, i64)>,
    /// time rules that apply to this site (also disabled ones), in list order
    pub rules: Vec<crate::settings::CategoryRule>,
    pub goals: Vec<crate::settings::Goal>,
    pub excluded: bool,
}

/// One website (exact domain as recorded) over a day / week / month.
pub fn site_detail(e: &Engine, name: &str, period: &str, anchor: i64, now: i64) -> Result<SiteDetail> {
    let period = if matches!(period, "day" | "week" | "month") { period } else { "week" };
    let today = e.today(now);
    let first = e.first_day()?.unwrap_or(today);
    let ((d0, d1), (p0, p1)) = period_range(period, anchor.min(today), first);
    let lo = p0.max(first.min(d0));
    let days = e.days(lo, d1.min(today).max(lo), now)?;
    let mut devp: HashMap<String, Vec<String>> = HashMap::new();
    for a in days.values() {
        for (d, ps) in &a.devp {
            devp.entry(d.clone()).or_insert_with(|| ps.clone());
        }
    }
    let (mut ms, mut prev) = (0, 0);
    let mut cats = BTreeMap::new();
    let mut br: BTreeMap<String, i64> = BTreeMap::new();
    let mut dv: BTreeMap<String, i64> = BTreeMap::new();
    let mut out_days = Vec::new();
    for d in d0..=d1 {
        let mut dc = BTreeMap::new();
        if d <= today
            && let Some(a) = days.get(&d)
        {
            for it in a.items.iter().filter(|i| i.k == 'w' && i.n == name) {
                ms += it.ms;
                e.item_cats(it, &mut dc);
                *br.entry(it.p.clone()).or_default() += it.ms;
                *dv.entry(e.device_name(&it.d, devp.get(&it.d).map(Vec::as_slice).unwrap_or(&[]))).or_default() += it.ms;
            }
        }
        for (c, v) in &dc {
            *cats.entry(c.clone()).or_default() += v;
        }
        let label = if period == "month" { date_of(d).day().to_string() } else { WD[weekday(d)].to_string() };
        out_days.push(SiteDay { day: day_to_str(d), label, cats: dc });
    }
    for d in p0..=p1 {
        if let Some(a) = days.get(&d) {
            prev += a.items.iter().filter(|i| i.k == 'w' && i.n == name).map(|i| i.ms).sum::<i64>();
        }
    }
    // Clock hours from the sessions themselves (the cache keeps no per-site hours).
    let mut hours = vec![0i64; 24];
    let (f, t) = (day_start_utc(d0, e.tz_now, e.ds), day_start_utc(d1.min(today) + 1, e.tz_now, e.ds));
    for sg in load_segs(e.store, &e.set, f, t)?.iter().filter(|s| s.kind == 'w' && s.name == name) {
        for (dd, a, b) in split_days(sg, e.ds) {
            if dd < d0 || dd > d1 {
                continue;
            }
            let off = sg.tz as i64 * 60_000;
            let mut x = a;
            while x < b {
                let local = x + off;
                let next = ((local.div_euclid(HOUR) + 1) * HOUR - off).min(b);
                hours[(local.rem_euclid(DAY) / HOUR) as usize] += next - x;
                x = next;
            }
        }
    }
    let sort = |m: BTreeMap<String, i64>| {
        let mut v: Vec<(String, i64)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    };
    let n = crate::settings::norm_domain(name);
    Ok(SiteDetail {
        name: name.into(),
        period: period.into(),
        title: period_title(period, d0, d1),
        ms,
        prev,
        days_counted: (d1.min(today) - d0.max(first) + 1).max(1),
        days: out_days,
        hours,
        cats,
        cat: static_category(&e.set, 'w', name, None),
        cat_set: e.set.categories.keys().any(|k| k.contains('.') && crate::settings::domain_matches(&n, k)),
        browsers: sort(br),
        devices: sort(dv),
        rules: e.set.category_rules.iter().filter(|r| r.kind == "domain" && crate::settings::domain_matches(name, &r.target)).cloned().collect(),
        goals: e.set.goals.iter().filter(|g| g.scope == "domain" && crate::settings::domain_matches(name, &g.target)).cloned().collect(),
        excluded: e.set.domain_excluded(name),
    })
}

/// Every app/site seen in the last `days` days with its time, for the
/// category editor and exclusion picker.
pub fn names(e: &Engine, days: i64, now: i64) -> Result<Vec<Total>> {
    let today = e.today(now);
    let d = e.days(today - days + 1, today, now)?;
    Ok(e.totals(d.values(), std::iter::empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const H: i64 = HOUR;
    const M: i64 = 60_000;

    fn st() -> Store {
        Store::open_in_memory("chrono", "dev-pc", MIGRATIONS).unwrap()
    }

    fn app(s: &Store, app: &str, plat: &str, start: i64, end: i64, tz: i64) {
        s.put("app_sessions", None, &json!({"app": app, "platform": plat, "start": start, "end": end, "tz_offset": tz})).unwrap();
    }

    #[test]
    fn day_split_union_and_cache() {
        let s = st();
        let d = 20_000; // a Monday? weekday(20000) computed below
        let t0 = d * DAY; // UTC midnight, tz 0
        app(&s, "code", "linux", t0 + 9 * H, t0 + 10 * H, 0);
        app(&s, "firefox", "linux", t0 + 10 * H, t0 + 11 * H, 0);
        s.put("web_sessions", None, &json!({"domain":"youtube.com","browser":"firefox","device":"dev-pc","start": t0 + 10 * H, "end": t0 + 10 * H + 40 * M, "tz_offset": 0})).unwrap();
        // phone overlapping code for 30 min, other device
        s.put("app_sessions", None, &json!({"app":"WhatsApp","package":"com.whatsapp","platform":"android","device":"phone1","start": t0 + 9 * H + 30 * M, "end": t0 + 10 * H, "tz_offset": 0})).unwrap();
        // crosses midnight: 23:30 → 00:30
        app(&s, "code", "linux", t0 + 23 * H + 30 * M, t0 + DAY + 30 * M, 0);
        s.put("android_events", Some("e1"), &json!({"hour": t0 + 9 * H, "tz_offset": 0, "device": "phone1", "unlocks": 3, "pickups": 5, "notifications": {"com.whatsapp": 4}})).unwrap();

        let e = Engine::new(&s, ChronoSettings::default(), 0, 0);
        let now = t0 + DAY + 12 * H;
        let days = e.days(d, d + 1, now).unwrap();
        let a = &days[&d];
        // union: 9-11 + 23:30-24 = 2.5 h
        assert_eq!(a.screen, 2 * H + 30 * M);
        assert_eq!(a.dev["dev-pc"], 2 * H + 30 * M);
        assert_eq!(a.dev["phone1"], 30 * M);
        assert_eq!(a.hours[9], H);
        assert_eq!(a.hours[23], 30 * M);
        assert_eq!(a.unlocks, 3);
        assert_eq!(a.pickups, 5);
        assert_eq!(a.notif["com.whatsapp"], 4);
        assert_eq!(days[&(d + 1)].screen, 30 * M);
        assert_eq!(a.first, Some(t0 + 9 * H));
        assert_eq!(a.last, Some(t0 + DAY));
        // categories: code 1.5h work, firefox 60m - youtube 40m → other 20m, youtube ent 40m, whatsapp social 30m
        let c = e.cats(a);
        assert_eq!(c["work"], H + 30 * M);
        assert_eq!(c["entertainment"], 40 * M);
        assert_eq!(c["other"], 20 * M);
        assert_eq!(c["social"], 30 * M);
        // cached now
        let n: i64 = s.conn().query_row("SELECT COUNT(*) FROM chrono_daily", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
        // new data invalidates only that day
        app(&s, "anki", "linux", t0 + DAY + 8 * H, t0 + DAY + 9 * H, 0);
        let days = e.days(d, d + 1, now).unwrap();
        assert_eq!(days[&(d + 1)].screen, 90 * M);
        assert_eq!(days[&d].screen, 2 * H + 30 * M);
        // excluding an app rebuilds everything
        let mut set = ChronoSettings::default();
        set.exclude_apps.push("Code".into());
        let e2 = Engine::new(&s, set, 0, 0);
        let days = e2.days(d, d, now).unwrap();
        assert_eq!(days[&d].dev["dev-pc"], H);
        assert!(days[&d].items.iter().all(|i| i.n != "code"));
    }

    #[test]
    fn sources_report_extras_and_labels() {
        let s = st();
        let now = 20_010 * DAY + 12 * H;
        app(&s, "code", "linux", now - 3 * H, now - 2 * H, 0);
        s.put("web_sessions", None, &json!({"domain":"github.com","browser":"firefox","device":"dev-pc","start": now - 3 * H, "end": now - 150 * M, "tz_offset": 0})).unwrap();
        s.put("app_sessions", None, &json!({"app":"WhatsApp","package":"com.whatsapp","platform":"android","device":"ph","start": now - 26 * H, "end": now - 25 * H, "tz_offset": 0})).unwrap();
        s.put("android_events", Some("e"), &json!({"hour": now - 26 * H, "tz_offset": 0, "device": "ph", "unlocks": 2, "pickups": 3, "notifications": {"com.whatsapp": 5}})).unwrap();
        s.put("android_apps", Some("p-com.whatsapp"), &json!({"package":"com.whatsapp","label":"WhatsApp"})).unwrap();
        // a late night: last use 23:30
        app(&s, "spotify", "linux", now - 36 * H - 30 * M, now - 36 * H, 0);
        let e = Engine::new(&s, ChronoSettings::default(), 0, 0);
        let src = sources(&e, now).unwrap();
        assert_eq!(src.len(), 2);
        assert!(src[0].this_device && src[0].browsers == vec!["firefox".to_string()]);
        assert!(src[1].phone_counters && src[1].name == "Phone");
        let r = report(&e, 20_009, now).unwrap();
        assert_eq!(r.notif_top, Some(("WhatsApp".into(), 5)));
        assert_eq!(r.late_days, 1);
        assert!(r.goals.is_empty());
        assert_eq!(package_label(&s, "com.unknown"), "com.unknown");
    }

    #[test]
    fn uninstalled_apps_keep_history() {
        let s = st();
        let now = 20_010 * DAY + 12 * H;
        s.put("app_sessions", None, &json!({"app":"Tester","package":"com.t","platform":"android","device":"ph","start": now - 3 * H, "end": now - 2 * H, "tz_offset": 0})).unwrap();
        s.put(crate::android::STATES, Some("i-ph-x"), &json!({"package":"com.t","label":"Tester","device":"ph","installed":false,"uninstalled_at": now - H})).unwrap();
        s.put(crate::android::CHANGES, Some("c1"), &json!({"package":"com.t","label":"Tester","kind":"uninstalled","at": now - H,"tz_offset":0,"device":"ph"})).unwrap();
        s.put(crate::android::CHANGES, Some("c0"), &json!({"package":"com.t","label":"Tester","kind":"installed","at": now - 4 * H,"tz_offset":0,"device":"ph"})).unwrap();
        let e = Engine::new(&s, ChronoSettings::default(), 0, 0);
        let v = day_view(&e, 20_010, now).unwrap();
        assert_eq!(v.top[0].name, "Tester");
        assert_eq!(v.top[0].uninstalled, Some(now - H));
        let c = app_changes(&e).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].kind, "uninstalled");
        assert_eq!(c[0].lifetime, H);
        assert!(!c[0].installed_now);
        assert_eq!(timeline(&e, 20_010).unwrap().sessions[0].uninstalled, Some(now - H));
    }

    #[test]
    fn timezone_and_day_start() {
        let s = st();
        let d = 20_000;
        // 01:00-02:00 local at UTC+6 = 19:00-20:00 UTC previous day
        let start = d * DAY - 6 * H + H;
        app(&s, "code", "linux", start, start + H, 360);
        let e = Engine::new(&s, ChronoSettings::default(), 0, 360);
        let a = &e.days(d, d, d * DAY + 20 * H).unwrap()[&d];
        assert_eq!(a.screen, H);
        assert_eq!(a.hours[1], H);
        // with day start 04:00 it belongs to the previous day
        let e = Engine::new(&s, ChronoSettings::default(), 240, 360);
        let v = e.days(d - 1, d, d * DAY + 20 * H).unwrap();
        assert_eq!(v[&(d - 1)].screen, H);
        assert_eq!(v[&d].screen, 0);
    }

    #[test]
    fn views_and_goals() {
        let s = st();
        let today = 20_005;
        let t0 = today * DAY;
        for back in 0..30 {
            let b = t0 - back * DAY;
            app(&s, "code", "linux", b + 9 * H, b + 11 * H, 0);
            app(&s, "spotify", "linux", b + 20 * H, b + 20 * H + 30 * M, 0);
        }
        let mut set = ChronoSettings::default();
        set.goals.push(crate::settings::Goal { id: "g".into(), kind: "limit".into(), scope: "category".into(), target: "entertainment".into(), minutes: 20, enabled: true, created: None });
        set.goals.push(crate::settings::Goal { id: "h".into(), kind: "goal".into(), scope: "app".into(), target: "Code".into(), minutes: 60, enabled: true, created: None });
        let e = Engine::new(&s, set, 0, 0);
        let now = t0 + 23 * H;
        let v = day_view(&e, today, now).unwrap();
        assert_eq!(v.screen, 2 * H + 30 * M);
        assert_eq!(v.week_avg, 2 * H + 30 * M);
        assert_eq!(v.goals[0].state, "over");
        assert_eq!(v.goals[1].state, "met");
        assert_eq!(v.strip.len(), 2);
        let tr = trends(&e, "week", today, now).unwrap();
        assert_eq!(tr.buckets.len(), 7);
        assert_eq!(tr.apps[0].name, "code");
        assert!(tr.apps[0].prev > 0);
        let w = weekday(today);
        assert_eq!(tr.heatmap[w][9], H);
        let m = trends(&e, "month", today, now).unwrap();
        assert!(m.buckets.len() >= 28);
        let y = trends(&e, "year", today, now).unwrap();
        assert_eq!(y.buckets.len(), 12);
        let l = trends(&e, "lifetime", today, now).unwrap();
        assert!(!l.buckets.is_empty());
        assert_eq!(l.summary.screen, 30 * (2 * H + 30 * M));
        let r = report(&e, today, now).unwrap();
        assert_eq!(r.days.len(), 7);
        assert_eq!(r.goals[0].days_ok, 0);
        let tl = timeline(&e, today).unwrap();
        assert_eq!(tl.lanes.len(), 1);
        assert_eq!(tl.sessions.len(), 2);
        assert_eq!(names(&e, 90, now).unwrap().len(), 2);
    }

    fn web(s: &Store, domain: &str, browser: &str, start: i64, end: i64, tz: i64) {
        s.put("web_sessions", None, &json!({"domain": domain, "browser": browser, "device": "dev-pc", "start": start, "end": end, "tz_offset": tz})).unwrap();
    }

    fn yt_rule(days: Vec<u8>, from: i64, to: i64) -> crate::settings::CategoryRule {
        crate::settings::CategoryRule {
            id: "r1".into(),
            target: "youtube.com".into(),
            kind: "domain".into(),
            category: "work".into(),
            days,
            start_min: from,
            end_min: to,
            enabled: true,
        }
    }

    #[test]
    fn time_rules_split_totals_focus_goals_and_timeline() {
        let s = st();
        let mon = 20773; // 2026-11-16, a Monday
        let t0 = mon * DAY;
        // youtube 16:30–17:30 on Monday; rule Mon–Fri 09:00–17:00 → 30 min work, 30 min entertainment
        web(&s, "m.youtube.com", "firefox", t0 + 16 * H + 30 * M, t0 + 17 * H + 30 * M, 0);
        web(&s, "github.com", "chrome", t0 + 10 * H, t0 + 11 * H, 0);
        let mut set = ChronoSettings::default();
        set.category_rules.push(yt_rule(vec![1, 2, 3, 4, 5], 9 * 60, 17 * 60));
        set.goals.push(crate::settings::Goal { id: "g".into(), kind: "limit".into(), scope: "category".into(), target: "entertainment".into(), minutes: 20, enabled: true, created: None });
        set.goals.push(crate::settings::Goal { id: "w".into(), kind: "goal".into(), scope: "category".into(), target: "work".into(), minutes: 90, enabled: true, created: None });
        let now = t0 + 20 * H;
        let e = Engine::new(&s, set.clone(), 0, 0);
        let a = e.days(mon, mon, now).unwrap().remove(&mon).unwrap();
        let yt = a.items.iter().find(|i| i.n == "m.youtube.com").unwrap();
        assert_eq!(yt.rc["work"], 30 * M);
        let c = e.cats(&a);
        assert_eq!(c["work"], H + 30 * M);
        assert_eq!(c["entertainment"], 30 * M);
        assert_eq!(focus_score(&c), Some(75));
        let g = e.goals(&a, mon);
        assert_eq!((g[0].used, g[0].state.as_str()), (30 * M, "over"));
        assert_eq!((g[1].used, g[1].state.as_str()), (H + 30 * M, "met"));
        // Totals keep the static category and say how much a rule moved.
        let tot = e.totals(std::iter::once(&a), std::iter::empty());
        let t = tot.iter().find(|t| t.name == "m.youtube.com").unwrap();
        assert_eq!((t.cat.as_str(), t.rules["work"]), ("entertainment", 30 * M));
        // Timeline blocks change colour at 17:00.
        let tl = timeline(&e, mon).unwrap();
        let lane = tl.lanes.iter().find(|l| l.kind == "browser").unwrap();
        let yt: Vec<_> = lane.segs.iter().filter(|x| x.2 == "m.youtube.com").map(|x| (x.0, x.1, x.3.as_str())).collect();
        assert_eq!(yt, vec![(t0 + 16 * H + 30 * M, t0 + 17 * H, "work"), (t0 + 17 * H, t0 + 17 * H + 30 * M, "entertainment")]);
        // Alerts: the entertainment limit fires on the split time.
        let al = crate::alerts::due(&e, &s, now).unwrap();
        assert!(al.iter().any(|x| x.title == "Limit reached · Entertainment"));
        // Changing the rule invalidates the cache: Tuesday-only → nothing moved.
        set.category_rules[0].days = vec![2];
        let e2 = Engine::new(&s, set.clone(), 0, 0);
        let a2 = e2.days(mon, mon, now).unwrap().remove(&mon).unwrap();
        assert_eq!(e2.cats(&a2)["entertainment"], H);
        assert!(a2.items.iter().all(|i| i.rc.is_empty()));
        // A disabled rule is ignored too.
        set.category_rules[0].days = vec![1];
        set.category_rules[0].enabled = false;
        let e3 = Engine::new(&s, set, 0, 0);
        assert_eq!(e3.cats(&e3.days(mon, mon, now).unwrap()[&mon])["entertainment"], H);
    }

    #[test]
    fn time_rules_with_day_start_and_overnight() {
        // Day starts at 04:00. Friday-night rule 22:00–02:00. A session Sat 00:30–01:30
        // belongs to logical Friday and is inside the rule (it started Friday).
        let s = st();
        let fri = 20777;
        let t0 = fri * DAY;
        web(&s, "youtube.com", "firefox", t0 + DAY + 30 * M, t0 + DAY + 90 * M, 0);
        // Sat 03:00–05:00: outside the rule; 03–04 is logical Friday, 04–05 Saturday
        web(&s, "youtube.com", "firefox", t0 + DAY + 3 * H, t0 + DAY + 5 * H, 0);
        let mut set = ChronoSettings::default();
        set.category_rules.push(yt_rule(vec![5], 22 * 60, 2 * 60));
        let e = Engine::new(&s, set, 240, 0);
        let now = t0 + 3 * DAY;
        let days = e.days(fri, fri + 1, now).unwrap();
        let c = e.cats(&days[&fri]);
        assert_eq!((c["work"], c["entertainment"]), (H, H));
        assert_eq!(e.cats(&days[&(fri + 1)])["entertainment"], H);
        assert!(!e.cats(&days[&(fri + 1)]).contains_key("work"));
    }

    #[test]
    fn websites_view_and_site_detail() {
        let s = st();
        let mon = 20773;
        let t0 = mon * DAY;
        let e0 = Engine::new(&s, ChronoSettings::default(), 0, 0);
        let empty = websites(&e0, "week", mon, t0 + 12 * H).unwrap();
        assert!(!empty.has_any && empty.sites.is_empty());
        web(&s, "youtube.com", "firefox", t0 + 16 * H, t0 + 18 * H, 0);
        web(&s, "youtube.com", "chrome", t0 + DAY + 20 * H, t0 + DAY + 21 * H, 0);
        web(&s, "en.wikipedia.org", "firefox", t0 + 9 * H, t0 + 9 * H + 30 * M, 0);
        web(&s, "github.com", "firefox", t0 - 7 * DAY + 9 * H, t0 - 7 * DAY + 10 * H, 0); // last week
        web(&s, "youtube.com", "firefox", t0 - 6 * DAY + 9 * H, t0 - 6 * DAY + 9 * H + 30 * M, 0); // last week
        app(&s, "code", "linux", t0 + 9 * H, t0 + 10 * H, 0);
        let mut set = ChronoSettings::default();
        set.category_rules.push(yt_rule(vec![1], 9 * 60, 17 * 60));
        set.goals.push(crate::settings::Goal { id: "g".into(), kind: "limit".into(), scope: "domain".into(), target: "youtube.com".into(), minutes: 30, enabled: true, created: None });
        let e = Engine::new(&s, set, 0, 0);
        let now = t0 + 2 * DAY + 12 * H; // Wednesday
        let v = websites(&e, "week", mon + 2, now).unwrap();
        assert!(v.has_any);
        assert_eq!(v.sites.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), vec!["youtube.com", "en.wikipedia.org"]);
        let yt = &v.sites[0];
        assert_eq!((yt.ms, yt.prev, yt.cat.as_str(), yt.rules["work"]), (3 * H, 30 * M, "entertainment", H));
        assert_eq!(yt.browsers.len(), 2);
        assert_eq!(v.total, 3 * H + 30 * M);
        assert_eq!(v.prev_total, H + 30 * M);
        assert_eq!(v.days, 3);
        assert_eq!((v.cats["work"], v.cats["entertainment"], v.cats["study"]), (H, 2 * H, 30 * M));
        assert_eq!(v.next_anchor, None);
        let d = websites(&e, "day", mon, now).unwrap();
        assert_eq!(d.total, 2 * H + 30 * M);
        assert!(d.next_anchor.is_some());
        let m = websites(&e, "month", mon, now).unwrap();
        assert_eq!(m.total, 5 * H); // the month also has last week (9th, 10th)
        let sd = site_detail(&e, "youtube.com", "week", mon, now).unwrap();
        assert_eq!((sd.ms, sd.prev, sd.days.len()), (3 * H, 30 * M, 7));
        assert_eq!(sd.days[0].cats["work"], H);
        assert_eq!(sd.days[0].cats["entertainment"], H);
        assert_eq!(sd.days[1].cats["entertainment"], H);
        assert_eq!((sd.hours[16], sd.hours[17], sd.hours[20]), (H, H, H));
        assert_eq!(sd.browsers[0], ("firefox".to_string(), 2 * H));
        assert_eq!((sd.rules.len(), sd.goals.len(), sd.cat.as_str(), sd.cat_set, sd.excluded), (1, 1, "entertainment", false, false));
        let wk = site_detail(&e, "en.wikipedia.org", "day", mon, now).unwrap();
        assert_eq!((wk.ms, wk.cat.as_str(), wk.days.len()), (30 * M, "study", 1));
    }
}
