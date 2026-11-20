//! Chrono's privacy settings, read by the agent for both the X11/Win32 tracker
//! and the browser native host: `chrono.db`, collection `chrono_settings`,
//! record id `settings` (owned by the Chrono app, see docs/apps/chrono/README.md).
//! Only the fields below are read; unknown fields are ignored.

use std::collections::HashSet;

use selfkit_core::Store;
use serde_json::Value;

pub const COLLECTION: &str = "chrono_settings";
pub const RECORD_ID: &str = "settings";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChronoSettings {
    /// UTC ms; tracking is paused while `now < paused_until`. None = not paused.
    pub paused_until: Option<i64>,
    /// Lowercase app names (as the tracker reports them, e.g. "keepassxc").
    pub exclude_apps: HashSet<String>,
    /// Lowercase domains; an entry also matches its subdomains.
    pub exclude_domains: Vec<String>,
    /// Minutes without input that count as "away" (Chrono → Devices and sources); None =
    /// each writer's default. Handed to the browser extension by the native host (L3-2).
    pub idle_minutes: Option<i64>,
}

fn str_list(v: Option<&Value>) -> impl Iterator<Item = String> + '_ {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|s| s.trim().trim_start_matches("*.").trim_end_matches('.').to_lowercase())
        .filter(|s| !s.is_empty())
}

impl ChronoSettings {
    pub fn from_json(d: &Value) -> Self {
        Self {
            paused_until: d.get("paused_until").and_then(Value::as_i64),
            exclude_apps: str_list(d.get("exclude_apps")).collect(),
            exclude_domains: str_list(d.get("exclude_domains")).map(|s| normalize_domain(&s).unwrap_or(s)).collect(),
            idle_minutes: d.get("idle_minutes").and_then(Value::as_i64).filter(|m| (1..=240).contains(m)),
        }
    }

    /// Load from an open chrono store; defaults when the record is missing or deleted.
    pub fn load(store: &Store) -> Self {
        match store.get(RECORD_ID) {
            Ok(Some(r)) if !r.deleted && r.collection == COLLECTION => Self::from_json(&r.data),
            _ => Self::default(),
        }
    }

    pub fn paused_at(&self, now: i64) -> bool {
        self.paused_until.is_some_and(|u| now < u)
    }

    pub fn domain_excluded(&self, domain: &str) -> bool {
        self.exclude_domains.iter().any(|e| domain == e || domain.ends_with(&format!(".{e}")))
    }
}

/// Normalize a host to the domain we store: lowercase, no port, no trailing dot,
/// no leading `www.`. Returns None for anything that is not a plausible hostname.
pub fn normalize_domain(host: &str) -> Option<String> {
    let mut h = host.trim().to_lowercase();
    if h.starts_with('[') {
        if let Some(end) = h.find(']') {
            h.truncate(end + 1);
        }
    } else if h.matches(':').count() == 1
        && let Some(i) = h.rfind(':')
        && h[i + 1..].chars().all(|c| c.is_ascii_digit())
    {
        h.truncate(i);
    }
    let h = h.trim_end_matches('.');
    let h = h.strip_prefix("www.").unwrap_or(h);
    let ok = !h.is_empty()
        && h.len() <= 253
        && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '[' || c == ']' || c == ':')
        && !h.starts_with('.')
        && !h.contains("..");
    ok.then(|| h.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_matches() {
        let s = ChronoSettings::from_json(&json!({
            "paused_until": 5000, "exclude_apps": ["KeePassXC", " "],
            "exclude_domains": ["*.Bank.example", "www.mail.test"], "other": 1
        }));
        assert!(s.paused_at(4999) && !s.paused_at(5000));
        assert!(s.exclude_apps.contains("keepassxc") && s.exclude_apps.len() == 1);
        assert!(s.domain_excluded("bank.example") && s.domain_excluded("login.bank.example"));
        assert!(!s.domain_excluded("notbank.example"));
        assert!(s.domain_excluded("mail.test"));
        assert_eq!(ChronoSettings::from_json(&json!({})), ChronoSettings::default());
    }

    #[test]
    fn loads_from_store() {
        let st = Store::open_in_memory("chrono", "d", &[]).unwrap();
        assert_eq!(ChronoSettings::load(&st), ChronoSettings::default());
        st.put(COLLECTION, Some(RECORD_ID), &json!({"exclude_apps": ["steam"]})).unwrap();
        assert!(ChronoSettings::load(&st).exclude_apps.contains("steam"));
        st.delete(RECORD_ID).unwrap();
        assert!(ChronoSettings::load(&st).exclude_apps.is_empty());
    }

    #[test]
    fn domains() {
        assert_eq!(normalize_domain("WWW.Example.COM:8080").as_deref(), Some("example.com"));
        assert_eq!(normalize_domain("localhost").as_deref(), Some("localhost"));
        assert_eq!(normalize_domain("a.test.").as_deref(), Some("a.test"));
        assert_eq!(normalize_domain("[::1]:80").as_deref(), Some("[::1]"));
        assert!(normalize_domain("").is_none());
        assert!(normalize_domain("evil.com/path").is_none());
        assert!(normalize_domain("a..b").is_none());
        assert!(normalize_domain(&"a".repeat(300)).is_none());
    }
}
