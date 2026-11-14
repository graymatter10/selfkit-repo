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
}

fn yes() -> bool {
    true
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
        }
    }
}

pub fn norm_domain(d: &str) -> String {
    let d = d.trim().to_lowercase();
    let d = d.split("://").last().unwrap_or(&d).to_string();
    let d = d.split('/').next().unwrap_or("").to_string();
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

    /// Stable fingerprint of what changes the per-day cache.
    pub fn cache_key(&self, day_start_min: u16) -> String {
        let mut a: Vec<String> = self.exclude_apps.iter().map(|s| s.to_lowercase()).collect();
        let mut d: Vec<String> = self.exclude_domains.iter().map(|s| norm_domain(s)).collect();
        a.sort();
        d.sort();
        format!("v1|{day_start_min}|{}|{}", a.join(","), d.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_and_pause() {
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
}
