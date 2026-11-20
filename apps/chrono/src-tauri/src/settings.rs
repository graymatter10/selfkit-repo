//! `chrono_settings` (single record id `settings`): pause, exclude lists,
//! category mapping, goals. Shared with the desktop agent and the browser
//! native host, which read it. Schema: docs/apps/chrono/README.md.

use std::collections::BTreeMap;

use selfkit_core::{Result, Store};
use serde::{Deserialize, Serialize};

pub const COLLECTION: &str = "chrono_settings";
pub const ID: &str = "settings";
/// "Paused until I resume" (2100-01-01).
pub const FOREVER: i64 = 4_102_444_800_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Goal {
    pub id: String,
    /// "limit" (stay under) or "goal" (reach at least)
    pub kind: String,
    /// "total" | "category" | "app" | "domain"
    pub scope: String,
    #[serde(default)]
    pub target: String,
    pub minutes: i64,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Logical day the goal was added ("YYYY-MM-DD"); days before it aren't judged
    /// (L3-12). Missing on older goals = always existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
}

impl Goal {
    /// First day (day index) the goal counts for.
    pub fn first_day(&self) -> Option<i64> {
        self.created.as_deref().and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).map(crate::stats::day_of_date)
    }
}

fn yes() -> bool {
    true
}

/// Time-based category: `target` counts as `category` on `days` (1 = Monday …
/// 7 = Sunday) from `start_min` to `end_min` (minutes after local midnight).
/// `end_min <= start_min` runs into the next day (22:00–02:00); equal = 24 h.
/// Resolution: categories::category_at. Docs: docs/apps/chrono/README.md.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CategoryRule {
    pub id: String,
    /// App name / Android package, or a domain (also matches its subdomains).
    pub target: String,
    /// "app" | "domain"
    pub kind: String,
    pub category: String,
    #[serde(default)]
    pub days: Vec<u8>,
    #[serde(default)]
    pub start_min: i64,
    #[serde(default)]
    pub end_min: i64,
    #[serde(default = "yes")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChronoSettings {
    pub paused_until: Option<i64>,
    pub exclude_apps: Vec<String>,
    pub exclude_domains: Vec<String>,
    pub categories: BTreeMap<String, String>,
    pub goals: Vec<Goal>,
    pub alerts: bool,
    /// Chrono-only: device id → display name.
    pub device_names: BTreeMap<String, String>,
    /// Chrono-only: past and current pauses `[from, to]` (UTC ms), newest last,
    /// so the Android importer can skip paused stretches it imports later.
    pub pause_log: Vec<[i64; 2]>,
    /// Chrono shows a weekly report notification on Monday mornings.
    pub weekly_report: bool,
    /// Minutes without keyboard/mouse after which time is not counted
    /// (desktop agent / native host). None = writer default (2 min).
    pub idle_minutes: Option<i64>,
    /// Chrono-only: time-based categories (merged by item id in sync).
    pub category_rules: Vec<CategoryRule>,
}

impl Default for ChronoSettings {
    fn default() -> Self {
        Self {
            paused_until: None,
            exclude_apps: vec![],
            exclude_domains: vec![],
            categories: BTreeMap::new(),
            goals: vec![],
            alerts: true,
            device_names: BTreeMap::new(),
            pause_log: vec![],
            weekly_report: true,
            idle_minutes: None,
            category_rules: vec![],
        }
    }
}

pub fn norm_domain(d: &str) -> String {
    let d = d.trim().to_lowercase();
    let d = d.split("://").last().unwrap_or(&d).to_string();
    let d = d.split('/').next().unwrap_or("").to_string();
    // No port and no trailing dot, like the agent's normalize_domain (L3-10).
    let d = match d.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') && !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => h.to_string(),
        _ => d,
    };
    let d = d.trim_end_matches('.').to_string();
    d.strip_prefix("www.").map(str::to_string).unwrap_or(d)
}

/// `example.com` matches `example.com` and `*.example.com`.
pub fn domain_matches(domain: &str, pattern: &str) -> bool {
    let d = norm_domain(domain);
    let p = norm_domain(pattern);
    !p.is_empty() && (d == p || d.ends_with(&format!(".{p}")))
}

impl ChronoSettings {
    pub fn load(store: &Store) -> Result<Self> {
        Ok(match store.get(ID)? {
            Some(r) if !r.deleted => serde_json::from_value(r.data).unwrap_or_default(),
            _ => Self::default(),
        })
    }

    pub fn save(&self, store: &Store) -> Result<()> {
        store.put(COLLECTION, Some(ID), &serde_json::to_value(self)?)?;
        Ok(())
    }

    pub fn is_paused(&self, now: i64) -> bool {
        self.paused_until.is_some_and(|t| now < t)
    }

    /// Was `t` inside a pause (current or logged)?
    pub fn paused_at(&self, t: i64) -> bool {
        self.pause_start(t).is_some()
    }

    /// Start of the pause that covers `t`, if any.
    pub fn pause_start(&self, t: i64) -> Option<i64> {
        self.pause_log.iter().find(|[a, b]| t >= *a && t < *b).map(|p| p[0])
    }

    /// Pause for `minutes` (None = until resumed).
    pub fn pause(&mut self, now: i64, minutes: Option<i64>) {
        let until = minutes.map(|m| now + m * 60_000).unwrap_or(FOREVER);
        self.paused_until = Some(until);
        if let Some(last) = self.pause_log.last_mut()
            && last[1] > now
        {
            last[1] = until;
        } else {
            self.pause_log.push([now, until]);
        }
        let n = self.pause_log.len();
        if n > 50 {
            self.pause_log.drain(..n - 50);
        }
    }

    pub fn resume(&mut self, now: i64) {
        self.paused_until = None;
        if let Some(last) = self.pause_log.last_mut()
            && last[1] > now
        {
            last[1] = now;
        }
    }

    pub fn app_excluded(&self, app: &str, package: Option<&str>) -> bool {
        self.exclude_apps
            .iter()
            .any(|x| x.eq_ignore_ascii_case(app) || package.is_some_and(|p| x.eq_ignore_ascii_case(p)))
    }

    pub fn domain_excluded(&self, domain: &str) -> bool {
        self.exclude_domains.iter().any(|p| domain_matches(domain, p))
    }

    /// Stable fingerprint of what changes the per-day cache (excludes, day
    /// start, and the enabled time rules, whose split is cached per item).
    pub fn cache_key(&self, day_start_min: u16) -> String {
        let mut a: Vec<String> = self.exclude_apps.iter().map(|s| s.to_lowercase()).collect();
        let mut d: Vec<String> = self.exclude_domains.iter().map(|s| norm_domain(s)).collect();
        a.sort();
        d.sort();
        let r: Vec<String> = self
            .category_rules
            .iter()
            .filter(|r| r.enabled)
            .map(|r| format!("{}:{}:{}:{:?}:{}-{}", r.kind, r.target.to_lowercase(), r.category, r.days, r.start_min, r.end_min))
            .collect();
        format!("v2|{day_start_min}|{}|{}|{}", a.join(","), d.join(","), r.join(";"))
    }

    /// Clean up what the UI (or an older/other device) sent: valid rule kinds,
    /// days 1–7 sorted, minutes within a day, domains normalised.
    pub fn sanitize(&mut self) {
        self.category_rules.retain(|r| (r.kind == "app" || r.kind == "domain") && !r.target.trim().is_empty() && !r.id.is_empty());
        for r in &mut self.category_rules {
            r.target = if r.kind == "domain" { norm_domain(&r.target) } else { r.target.trim().to_string() };
            r.category = r.category.trim().to_lowercase();
            if r.category.is_empty() {
                r.category = "other".into();
            }
            r.days.retain(|d| (1..=7).contains(d));
            r.days.sort_unstable();
            r.days.dedup();
            r.start_min = r.start_min.clamp(0, 1439);
            r.end_min = r.end_min.clamp(0, 1440) % 1440;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_and_pause() {
        assert_eq!(norm_domain("https://WWW.Example.org/path"), "example.org"); // L3-10
        assert_eq!(norm_domain("sub.test.localhost:8765"), "sub.test.localhost");
        assert!(domain_matches("mail.google.com", "google.com"));
        assert!(domain_matches("www.Google.com", "google.com"));
        assert!(!domain_matches("notgoogle.com", "google.com"));
        let mut s = ChronoSettings::default();
        s.pause(1000, Some(10));
        assert!(s.is_paused(1000 + 5 * 60_000));
        assert!(!s.is_paused(1000 + 11 * 60_000));
        s.resume(2000);
        assert!(!s.is_paused(2500));
        assert!(s.paused_at(1500));
        assert!(!s.paused_at(2500));
        let st = Store::open_in_memory("chrono", "d", &[]).unwrap();
        s.exclude_apps.push("KeePassXC".into());
        s.save(&st).unwrap();
        let l = ChronoSettings::load(&st).unwrap();
        assert!(l.app_excluded("keepassxc", None));
        // Unknown/missing fields tolerated.
        st.put(COLLECTION, Some(ID), &serde_json::json!({"paused_until": null, "exclude_domains": ["x.com"]})).unwrap();
        let l = ChronoSettings::load(&st).unwrap();
        assert!(l.alerts && l.domain_excluded("www.x.com"));
    }

    /// Two devices change `category_rules` after sharing version "100:A"; the
    /// shared core field merge (`Store::field_merge`, lists combined by item id).
    fn merged(a: Vec<CategoryRule>, b: Vec<CategoryRule>, a_alerts: bool) -> Vec<String> {
        use selfkit_core::Record;
        let rec = |rules: Vec<CategoryRule>, at: i64, dev: &str, alerts: bool, rules_at: i64| Record {
            id: ID.into(),
            collection: COLLECTION.into(),
            data: serde_json::json!({"alerts": alerts, "category_rules": rules}),
            updated_at: at,
            tz_offset: 0,
            device_id: dev.into(),
            deleted: false,
            hist: vec!["100:A".into(), format!("{at}:{dev}")],
            fclock: [("alerts".to_string(), if alerts { 100 } else { at }), ("category_rules".to_string(), rules_at)].into_iter().collect(),
        };
        let rules_b_at = 300;
        let l = rec(a, 200, "A", a_alerts, 200);
        let r = rec(b, 300, "B", true, rules_b_at);
        let v = Store::field_merge(&l, &r).expect("merges without a conflict");
        let m: ChronoSettings = serde_json::from_value(v).unwrap();
        m.category_rules.iter().map(|r| format!("{}:{}:{}", r.id, r.category, r.start_min)).collect()
    }

    fn cr(id: &str, cat: &str, start: i64) -> CategoryRule {
        CategoryRule { id: id.into(), target: "youtube.com".into(), kind: "domain".into(), category: cat.into(), days: vec![1], start_min: start, end_min: 600, enabled: true }
    }

    #[test]
    fn category_rules_merge_by_id_across_devices() {
        let base = || vec![cr("r1", "work", 540)];
        // Each device adds a rule → both kept.
        let mut a = base();
        a.push(cr("ra", "study", 60));
        let mut b = base();
        b.push(cr("rb", "social", 120));
        let m = merged(a, b, true);
        assert_eq!(m, vec!["r1:work:540", "rb:social:120", "ra:study:60"]);
        // The same rule edited on both → the newer device's copy (B).
        let m = merged(vec![cr("r1", "study", 540)], vec![cr("r1", "social", 480)], true);
        assert_eq!(m, vec!["r1:social:480"]);
        // Only A touched the rules (B changed another field): A's list as it is,
        // including a deletion and an edit.
        let l = vec![cr("r2", "work", 0)];
        let rec = |data: serde_json::Value, at: i64, dev: &str, clocks: &[(&str, i64)]| selfkit_core::Record {
            id: ID.into(),
            collection: COLLECTION.into(),
            data,
            updated_at: at,
            tz_offset: 0,
            device_id: dev.into(),
            deleted: false,
            hist: vec!["100:A".into(), format!("{at}:{dev}")],
            fclock: clocks.iter().map(|(k, t)| (k.to_string(), *t)).collect(),
        };
        let la = rec(serde_json::json!({"alerts": true, "category_rules": l}), 200, "A", &[("alerts", 100), ("category_rules", 200)]);
        let rb = rec(serde_json::json!({"alerts": false, "category_rules": base()}), 300, "B", &[("alerts", 300), ("category_rules", 100)]);
        let v: ChronoSettings = serde_json::from_value(Store::field_merge(&la, &rb).unwrap()).unwrap();
        assert_eq!((v.category_rules.len(), v.category_rules[0].id.as_str(), v.alerts), (1, "r2", false));
        // Known limits of the shared list merge (core `union_lists`), when BOTH devices
        // changed the list: a rule deleted on one device comes back, and an older edit
        // of a rule loses to the newer device's unchanged copy of it.
        let m = merged(vec![], vec![cr("r1", "work", 540), cr("rb", "social", 120)], true);
        assert_eq!(m, vec!["r1:work:540", "rb:social:120"]);
        let m = merged(vec![cr("r1", "study", 540)], vec![cr("r1", "work", 540), cr("rb", "social", 120)], true);
        assert_eq!(m, vec!["r1:work:540", "rb:social:120"]);
    }

    #[test]
    fn category_rules_sanitize_and_cache_key() {
        let st = Store::open_in_memory("chrono", "d", &[]).unwrap();
        // Records from builds without rules load with an empty list.
        st.put(COLLECTION, Some(ID), &serde_json::json!({"categories": {"code": "work"}})).unwrap();
        let mut s = ChronoSettings::load(&st).unwrap();
        assert!(s.category_rules.is_empty());
        let k0 = s.cache_key(0);
        let r = |id: &str, kind: &str, target: &str| CategoryRule {
            id: id.into(),
            target: target.into(),
            kind: kind.into(),
            category: " Work ".into(),
            days: vec![5, 1, 9, 1, 0],
            start_min: -5,
            end_min: 1440,
            enabled: true,
        };
        s.category_rules = vec![r("a", "domain", "https://WWW.YouTube.com/watch"), r("b", "web", "x.com"), r("", "app", "code"), r("c", "app", "  Telegram ")];
        s.sanitize();
        assert_eq!(s.category_rules.len(), 2);
        let y = &s.category_rules[0];
        assert_eq!((y.target.as_str(), y.category.as_str(), y.days.clone(), y.start_min, y.end_min), ("youtube.com", "work", vec![1, 5], 0, 0));
        assert_eq!(s.category_rules[1].target, "Telegram");
        // enabled rules change the cache key, disabled ones don't
        let k1 = s.cache_key(0);
        assert_ne!(k0, k1);
        s.category_rules[1].enabled = false;
        assert_ne!(s.cache_key(0), k1);
        s.category_rules.iter_mut().for_each(|r| r.enabled = false);
        assert_eq!(s.cache_key(0), k0);
        s.save(&st).unwrap();
        assert_eq!(ChronoSettings::load(&st).unwrap().category_rules.len(), 2);
    }
}
