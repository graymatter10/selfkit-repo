//! Android usage importer (pure Rust, so it is unit-tested on desktop).
//!
//! The Kotlin side (`UsagePlugin.kt` / `ImportWorker.kt`) reads raw
//! `UsageEvents` since a cursor plus the notification listener's hourly
//! counts and hands them over as one JSON batch. Here we turn them into
//! `app_sessions` (platform `android`) and hourly `android_events`, with
//! deterministic ids so overlapping re-imports overwrite instead of
//! duplicating. Docs: docs/apps/chrono/android.md.

use std::collections::{BTreeMap, HashMap, HashSet};

use selfkit_core::{Result, Store};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::settings::ChronoSettings;

// android.app.usage.UsageEvents.Event constants
pub const ACTIVITY_RESUMED: i32 = 1;
pub const ACTIVITY_PAUSED: i32 = 2;
pub const NOTIFICATION_INTERRUPTION: i32 = 12;
pub const SCREEN_INTERACTIVE: i32 = 15;
pub const SCREEN_NON_INTERACTIVE: i32 = 16;
pub const KEYGUARD_SHOWN: i32 = 17;
pub const KEYGUARD_HIDDEN: i32 = 18;
pub const ACTIVITY_STOPPED: i32 = 23;
pub const DEVICE_SHUTDOWN: i32 = 26;

const HOUR: i64 = 3_600_000;
/// UsageEvents keeps roughly a week; never look back further.
pub const MAX_BACK: i64 = 7 * 24 * HOUR;
/// Paused-then-resumed within this gap (activity transitions) = same session.
const GLUE_MS: i64 = 2_000;
pub const CURSOR_KEY: &str = "chrono.import.cursor";

#[derive(Debug, Default, Deserialize)]
pub struct Batch {
    /// scan start the collector used (UTC ms, hour aligned)
    pub since: i64,
    pub now: i64,
    /// [ts, type, package, tz offset minutes]
    #[serde(default)]
    pub events: Vec<(i64, i32, String, i32)>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub launchers: Vec<String>,
    /// [hour start UTC ms, package, count] from the notification listener
    #[serde(default)]
    pub notif: Vec<(i64, String, i64)>,
    /// offset (minutes) to use for hours that only have listener counts
    #[serde(default)]
    pub tz_now: i32,
    /// installed launchable apps: [package, label, first install, last update, system]
    /// (None = the collector did not send a list; then no app changes are derived)
    #[serde(default)]
    pub packages: Option<Vec<(String, String, i64, i64, bool)>>,
    /// package → small PNG data URL (only new or updated apps)
    #[serde(default)]
    pub icons: HashMap<String, String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Imported {
    pub changes: usize,
    pub sessions: usize,
    pub hours: usize,
    pub cursor: i64,
    pub events: usize,
}

/// Where the next scan should start.
pub fn cursor(store: &Store, now: i64) -> i64 {
    let floor = floor_hour(now - MAX_BACK);
    store.kv_get(CURSOR_KEY).ok().flatten().and_then(|v| v.parse::<i64>().ok()).unwrap_or(floor).max(floor)
}

pub fn floor_hour(t: i64) -> i64 {
    t.div_euclid(HOUR) * HOUR
}

fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:x}", h & 0xffff_ffff)
}

fn short(dev: &str) -> String {
    dev.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().chars().rev().take(12).collect()
}

fn hour_id(dev: &str, hour: i64) -> String {
    let dt = chrono::DateTime::from_timestamp_millis(hour).unwrap_or_default();
    format!("e-{}-{}", short(dev), dt.format("%Y%m%d%H"))
}

/// Put only when the content changed (avoids sync churn on re-imports).
fn put_if_changed(store: &Store, col: &str, id: &str, data: &serde_json::Value) -> Result<bool> {
    if let Some(r) = store.get(id)?
        && !r.deleted
        && &r.data == data
    {
        return Ok(false);
    }
    store.put(col, Some(id), data)?;
    Ok(true)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sess {
    pub package: String,
    pub start: i64,
    pub end: i64,
    pub tz: i32,
    pub open: bool,
}

#[derive(Default, Debug)]
pub struct Hourly {
    pub tz: i32,
    pub unlocks: i64,
    pub pickups: i64,
    pub notif_ue: BTreeMap<String, i64>,
}

/// Derive foreground sessions and hourly counters from raw events.
pub fn derive(b: &Batch, set: &ChronoSettings) -> (Vec<Sess>, BTreeMap<i64, Hourly>) {
    let ignore: HashSet<&str> = b.launchers.iter().map(String::as_str).chain(["com.android.systemui", "android"]).collect();
    let mut ev = b.events.clone();
    ev.sort_by_key(|e| e.0);
    let mut out = Vec::new();
    let mut hours: BTreeMap<i64, Hourly> = BTreeMap::new();
    // (package, start, tz), pending end
    let mut cur: Option<(String, i64, i32)> = None;
    let mut pend: Option<i64> = None;
    let close = |cur: &mut Option<(String, i64, i32)>, pend: &mut Option<i64>, at: i64, out: &mut Vec<Sess>| {
        if let Some((p, s, tz)) = cur.take() {
            let mut end = pend.take().map_or(at, |x| x.min(at));
            // never count time inside a pause, even with no events during it
            if let Some(ps) = set.pause_log.iter().filter(|w| w[0] < end && w[1] > s).map(|w| w[0]).min() {
                end = end.min(ps.max(s));
            }
            if end - s >= 1000 {
                out.push(Sess { package: p, start: s, end, tz, open: false });
            }
        }
        *pend = None;
    };
    for (ts, ty, pkg, tz) in ev {
        if ts < b.since {
            continue;
        }
        if let Some(ps) = set.pause_start(ts) {
            close(&mut cur, &mut pend, ps.min(ts), &mut out);
            continue;
        }
        // a pause that ended long ago: close the session at the pause point
        if let (Some(p), Some(_)) = (pend, &cur)
            && ts - p > GLUE_MS
            && !matches!(ty, ACTIVITY_RESUMED)
        {
            close(&mut cur, &mut pend, p, &mut out);
        }
        match ty {
            ACTIVITY_RESUMED => {
                if ignore.contains(pkg.as_str()) {
                    close(&mut cur, &mut pend, ts, &mut out);
                    continue;
                }
                match &cur {
                    Some((p, _, _)) if *p == pkg && pend.is_none_or(|e| ts - e <= GLUE_MS) => pend = None,
                    Some(_) => {
                        close(&mut cur, &mut pend, ts, &mut out);
                        cur = Some((pkg, ts, tz));
                    }
                    None => cur = Some((pkg, ts, tz)),
                }
            }
            ACTIVITY_PAUSED | ACTIVITY_STOPPED => {
                if cur.as_ref().is_some_and(|c| c.0 == pkg) && pend.is_none() {
                    pend = Some(ts);
                }
            }
            SCREEN_NON_INTERACTIVE | KEYGUARD_SHOWN | DEVICE_SHUTDOWN => close(&mut cur, &mut pend, ts, &mut out),
            SCREEN_INTERACTIVE => {
                let h = hours.entry(floor_hour(ts)).or_default();
                h.tz = tz;
                h.pickups += 1;
            }
            KEYGUARD_HIDDEN => {
                let h = hours.entry(floor_hour(ts)).or_default();
                h.tz = tz;
                h.unlocks += 1;
            }
            NOTIFICATION_INTERRUPTION => {
                let h = hours.entry(floor_hour(ts)).or_default();
                h.tz = tz;
                *h.notif_ue.entry(pkg).or_default() += 1;
            }
            _ => {}
        }
    }
    if let Some((p, s, tz)) = cur.take() {
        match pend {
            Some(e) if b.now - e > GLUE_MS => {
                if e - s >= 1000 {
                    out.push(Sess { package: p, start: s, end: e, tz, open: false });
                }
            }
            _ if b.now - s >= 1000 && !set.is_paused(b.now) => out.push(Sess { package: p, start: s, end: b.now, tz, open: true }),
            _ => {}
        }
    }
    (out, hours)
}

/// Import one batch into chrono.db; returns counts and the new cursor.
pub fn ingest(store: &Store, set: &ChronoSettings, b: &Batch) -> Result<Imported> {
    let dev = store.device_id().to_string();
    let (sessions, mut hours) = derive(b, set);
    let mut res = Imported { events: b.events.len(), ..Default::default() };
    // labels (+ icons): kept forever so removed apps keep their name and icon
    let mut all_labels = b.labels.clone();
    for (p, l, ..) in b.packages.iter().flatten() {
        all_labels.entry(p.clone()).or_insert_with(|| l.clone());
    }
    for (pkg, label) in &all_labels {
        let id = format!("p-{pkg}");
        let old = store.get(&id)?.filter(|r| !r.deleted).map(|r| r.data);
        let icon = b.icons.get(pkg).cloned().or_else(|| old.as_ref().and_then(|o| o.get("icon").and_then(|v| v.as_str()).map(str::to_string)));
        let mut data = json!({"package": pkg, "label": label});
        if let Some(i) = icon {
            data["icon"] = i.into();
        }
        put_if_changed(store, "android_apps", &id, &data)?;
    }
    if let Some(pk) = &b.packages {
        res.changes = app_changes(store, &dev, pk, b)?;
    }
    let label = |p: &str| b.labels.get(p).cloned().unwrap_or_else(|| p.to_string());
    let mut earliest_open: Option<i64> = None;
    for s in &sessions {
        let name = label(&s.package);
        if s.open {
            earliest_open = Some(earliest_open.map_or(s.start, |x: i64| x.min(s.start)));
        }
        if set.app_excluded(&name, Some(&s.package)) {
            continue;
        }
        let id = format!("a-{}-{}-{}", short(&dev), s.start, fnv(&s.package));
        let data = json!({"app": name, "package": s.package, "platform": "android", "device": dev,
                          "start": s.start, "end": s.end, "tz_offset": s.tz});
        if put_if_changed(store, "app_sessions", &id, &data)? {
            res.sessions += 1;
        }
    }
    // listener counts per hour
    let mut lst: BTreeMap<i64, BTreeMap<String, i64>> = BTreeMap::new();
    for (h, p, n) in &b.notif {
        if *h >= b.since && !set.paused_at(*h) {
            *lst.entry(floor_hour(*h)).or_default().entry(p.clone()).or_default() += n;
        }
    }
    for h in lst.keys() {
        hours.entry(*h).or_insert_with(|| Hourly { tz: b.tz_now, ..Default::default() });
    }
    for (h, c) in &hours {
        let mut n: BTreeMap<String, i64> = c.notif_ue.clone();
        for (p, k) in lst.get(h).into_iter().flatten() {
            let e = n.entry(p.clone()).or_default();
            *e = (*e).max(*k);
        }
        n.retain(|p, _| !set.app_excluded(&label(p), Some(p)));
        if c.unlocks == 0 && c.pickups == 0 && n.is_empty() {
            continue;
        }
        let data = json!({"hour": h, "tz_offset": c.tz, "device": dev, "unlocks": c.unlocks, "pickups": c.pickups, "notifications": n});
        if put_if_changed(store, "android_events", &hour_id(&dev, *h), &data)? {
            res.hours += 1;
        }
    }
    let cur = floor_hour(earliest_open.unwrap_or(b.now).min(b.now)).max(b.since);
    store.kv_set(CURSOR_KEY, &cur.to_string())?;
    res.cursor = cur;
    Ok(res)
}

pub const CHANGES: &str = "app_changes";
pub const STATES: &str = "app_installs";

/// Diff the installed-app list against the last known state of this device
/// (`app_installs`, one record per device and package) and record
/// `app_changes` events: installed, uninstalled, reinstalled, updated.
/// The first list a device sends is only a baseline (no events).
pub fn app_changes(store: &Store, dev: &str, pk: &[(String, String, i64, i64, bool)], b: &Batch) -> Result<usize> {
    let d = short(dev);
    let prefix = format!("i-{d}-");
    let mut known: HashMap<String, serde_json::Value> = HashMap::new();
    for r in store.list(STATES, 100_000, 0)? {
        if r.id.starts_with(&prefix) {
            known.insert(r.data["package"].as_str().unwrap_or_default().to_string(), r.data);
        }
    }
    let baseline = known.is_empty();
    let mut n = 0;
    let mut event = |pkg: &str, label: &str, kind: &str, at: i64| -> Result<()> {
        let id = format!("c-{d}-{}-{kind}-{at}", fnv(pkg));
        let data = json!({"package": pkg, "label": label, "kind": kind, "at": at, "tz_offset": b.tz_now, "device": dev});
        if put_if_changed(store, CHANGES, &id, &data)? {
            n += 1;
        }
        Ok(())
    };
    let mut seen: HashSet<&str> = HashSet::new();
    for (pkg, label, first, upd, system) in pk {
        seen.insert(pkg.as_str());
        let old = known.get(pkg.as_str());
        let was_installed = old.and_then(|o| o["installed"].as_bool());
        let old_first = old.and_then(|o| o["first_install"].as_i64()).unwrap_or(0);
        let old_upd = old.and_then(|o| o["last_update"].as_i64()).unwrap_or(0);
        if !baseline {
            match was_installed {
                None => event(pkg, label, "installed", *first)?,
                Some(false) => event(pkg, label, "reinstalled", *first)?,
                // removed and installed again between two imports
                Some(true) if *first > old_first + 1000 => event(pkg, label, "reinstalled", *first)?,
                Some(true) if *upd > old_upd + 1000 && !system => event(pkg, label, "updated", *upd)?,
                _ => {}
            }
        }
        let mut data = json!({"package": pkg, "label": label, "device": dev, "installed": true,
                              "first_install": first, "last_update": upd, "system": system});
        if let Some(u) = old.and_then(|o| o.get("uninstalled_at")).filter(|v| !v.is_null()) {
            data["last_uninstalled_at"] = u.clone();
        } else if let Some(u) = old.and_then(|o| o.get("last_uninstalled_at")).filter(|v| !v.is_null()) {
            data["last_uninstalled_at"] = u.clone();
        }
        put_if_changed(store, STATES, &format!("{prefix}{}", fnv(pkg)), &data)?;
    }
    for (pkg, old) in &known {
        if seen.contains(pkg.as_str()) || old["installed"].as_bool() != Some(true) {
            continue;
        }
        let label = old["label"].as_str().unwrap_or(pkg).to_string();
        event(pkg, &label, "uninstalled", b.now)?;
        let mut data = old.clone();
        data["installed"] = false.into();
        data["uninstalled_at"] = b.now.into();
        put_if_changed(store, STATES, &format!("{prefix}{}", fnv(pkg)), &data)?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: i64 = 60_000;

    fn batch(events: Vec<(i64, i32, &str)>, now: i64) -> Batch {
        Batch {
            since: 0,
            now,
            events: events.into_iter().map(|(t, ty, p)| (t, ty, p.to_string(), 60)).collect(),
            labels: HashMap::from([("com.whatsapp".into(), "WhatsApp".into()), ("org.chromium".into(), "Chrome".into())]),
            launchers: vec!["com.launcher".into()],
            notif: vec![],
            tz_now: 60,
            ..Default::default()
        }
    }

    #[test]
    fn sessions_counters_and_reimport() {
        let t = 10 * HOUR;
        let ev = vec![
            (t, SCREEN_INTERACTIVE, "android"),
            (t + 1000, KEYGUARD_HIDDEN, "android"),
            (t + 2000, ACTIVITY_RESUMED, "com.launcher"),
            (t + 5000, ACTIVITY_RESUMED, "com.whatsapp"),
            // activity transition inside WhatsApp: glued
            (t + 60_000, ACTIVITY_PAUSED, "com.whatsapp"),
            (t + 60_500, ACTIVITY_RESUMED, "com.whatsapp"),
            (t + 5 * M, ACTIVITY_PAUSED, "com.whatsapp"),
            (t + 5 * M + 300, ACTIVITY_RESUMED, "org.chromium"),
            (t + 9 * M, NOTIFICATION_INTERRUPTION, "com.whatsapp"),
            (t + 10 * M, SCREEN_NON_INTERACTIVE, "android"),
            (t + 20 * M, SCREEN_INTERACTIVE, "android"),
            (t + 20 * M + 1000, ACTIVITY_RESUMED, "com.whatsapp"),
        ];
        let b = batch(ev, t + 25 * M);
        let set = ChronoSettings::default();
        let (s, h) = derive(&b, &set);
        assert_eq!(s.len(), 3, "{s:?}");
        assert_eq!((s[0].package.as_str(), s[0].end - s[0].start), ("com.whatsapp", 5 * M - 5000));
        assert_eq!((s[1].package.as_str(), s[1].end - s[1].start), ("org.chromium", 5 * M - 300));
        assert!(s[2].open && s[2].end == t + 25 * M);
        let hh = &h[&floor_hour(t)];
        assert_eq!((hh.pickups, hh.unlocks, hh.notif_ue["com.whatsapp"]), (2, 1, 1));

        let st = Store::open_in_memory("chrono", "phone-1", &[]).unwrap();
        let r = ingest(&st, &set, &b).unwrap();
        assert_eq!(r.sessions, 3);
        assert_eq!(r.cursor, floor_hour(t + 20 * M));
        assert_eq!(st.count("app_sessions").unwrap(), 3);
        // re-import later with the open session now finished: same ids, no duplicates
        let mut ev2 = b.events.clone();
        ev2.push((t + 30 * M, ACTIVITY_PAUSED, "com.whatsapp".into(), 60));
        ev2.push((t + 30 * M + 10, SCREEN_NON_INTERACTIVE, "android".into(), 60));
        let b2 = Batch { since: r.cursor, now: t + 40 * M, events: ev2, ..batch(vec![], 0) };
        let r2 = ingest(&st, &set, &b2).unwrap();
        assert_eq!(st.count("app_sessions").unwrap(), 3);
        assert_eq!(r2.sessions, 1);
        let rec = st.list("app_sessions", 10, 0).unwrap().into_iter().find(|r| r.data["start"] == t + 20 * M + 1000).unwrap();
        assert_eq!(rec.data["end"], t + 30 * M);
        assert_eq!(rec.data["app"], "WhatsApp");
        assert_eq!(st.count("android_events").unwrap(), 1);
    }

    #[test]
    fn install_uninstall_reinstall() {
        let st = Store::open_in_memory("chrono", "phone-1", &[]).unwrap();
        let set = ChronoSettings::default();
        let pk = |v: &[(&str, i64, i64)]| Some(v.iter().map(|(p, f, u)| (p.to_string(), p.to_uppercase(), *f, *u, false)).collect::<Vec<_>>());
        let mut b = Batch { since: 0, now: 1000, packages: pk(&[("a", 10, 10)]), ..Default::default() };
        b.icons.insert("a".into(), "data:image/png;base64,AA".into());
        assert_eq!(ingest(&st, &set, &b).unwrap().changes, 0, "first list is a baseline");
        b.icons.clear();
        let kinds = |st: &Store| {
            let mut v: Vec<(String, String)> = st.list(CHANGES, 100, 0).unwrap().into_iter().map(|r| (r.data["package"].as_str().unwrap().to_string(), r.data["kind"].as_str().unwrap().to_string())).collect();
            v.sort();
            v
        };
        // new app t installed, a updated
        let b2 = Batch { now: 2000, packages: pk(&[("a", 10, 1500), ("t", 1800, 1800)]), ..Default::default() };
        ingest(&st, &set, &b2).unwrap();
        assert_eq!(kinds(&st), vec![("a".into(), "updated".into()), ("t".into(), "installed".into())]);
        // t removed
        let b3 = Batch { now: 3000, packages: pk(&[("a", 10, 1500)]), ..Default::default() };
        ingest(&st, &set, &b3).unwrap();
        assert!(kinds(&st).contains(&("t".into(), "uninstalled".into())));
        // t back
        let b4 = Batch { now: 4000, packages: pk(&[("a", 10, 1500), ("t", 3500, 3500)]), ..Default::default() };
        ingest(&st, &set, &b4).unwrap();
        assert!(kinds(&st).contains(&("t".into(), "reinstalled".into())));
        assert_eq!(kinds(&st).len(), 4);
        // re-import of the same list adds nothing; icon kept on label refresh
        ingest(&st, &set, &b4).unwrap();
        assert_eq!(kinds(&st).len(), 4);
        assert_eq!(st.get("p-a").unwrap().unwrap().data["icon"], "data:image/png;base64,AA");
        // no list sent → no changes derived
        ingest(&st, &set, &Batch { now: 5000, ..Default::default() }).unwrap();
        assert_eq!(kinds(&st).len(), 4);
    }

    #[test]
    fn exclusions_pause_and_listener() {
        let t = 10 * HOUR;
        let mut set = ChronoSettings::default();
        set.exclude_apps.push("WhatsApp".into());
        set.pause_log.push([t + 30 * M, t + 40 * M]);
        let ev = vec![
            (t, ACTIVITY_RESUMED, "com.whatsapp"),
            (t + 5 * M, ACTIVITY_RESUMED, "org.chromium"),
            (t + 35 * M, KEYGUARD_HIDDEN, "android"), // paused: not counted
            (t + 50 * M, SCREEN_NON_INTERACTIVE, "android"),
        ];
        let mut b = batch(ev, t + 55 * M);
        b.notif = vec![(t, "com.x".into(), 3), (t, "com.whatsapp".into(), 2)];
        let st = Store::open_in_memory("chrono", "p", &[]).unwrap();
        ingest(&st, &set, &b).unwrap();
        let v = st.list("app_sessions", 10, 0).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].data["end"], t + 30 * M, "chrome closed at pause start");
        let e = st.list("android_events", 10, 0).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].data["unlocks"], 0);
        assert_eq!(e[0].data["notifications"]["com.x"], 3);
        assert!(e[0].data["notifications"].get("com.whatsapp").is_none());
    }
}
