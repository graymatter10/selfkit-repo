//! Non-secret agent settings: `<data_dir>/agent.json` (optional; every field has
//! a default) plus env overrides. Secrets (Postgres URL, S3 keys, data key) live
//! in the OS keychain, or in env vars for dev/test.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupCfg {
    /// Automatic monthly + weekly backups.
    pub enabled: bool,
    /// Also keep a copy of each bundle in `<data_dir>/backups/`.
    pub keep_local: bool,
    /// Weekly full exports kept on S3 per app and device (older are deleted).
    pub weekly_keep: usize,
    /// Weekly full exports kept locally when `keep_local` is on.
    pub local_weekly_keep: usize,
}

impl Default for BackupCfg {
    fn default() -> Self {
        Self { enabled: true, keep_local: false, weekly_keep: 4, local_weekly_keep: 2 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RemindersCfg {
    /// Desktop notifications for Agenda task reminders.
    pub enabled: bool,
}

impl Default for RemindersCfg {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub backup: BackupCfg,
    pub reminders: RemindersCfg,
    /// Server database size limit for the health warning (MB). Neon free = 512.
    pub server_db_limit_mb: Option<u64>,
}

fn env_bool(k: &str) -> Option<bool> {
    std::env::var(k).ok().map(|v| !matches!(v.as_str(), "0" | "false" | "no" | "off" | ""))
}

impl Config {
    pub fn path() -> std::path::PathBuf {
        selfkit_core::paths::data_dir().join("agent.json")
    }

    pub fn load() -> Self {
        let mut c: Config = std::fs::read(Self::path())
            .ok()
            .and_then(|b| match serde_json::from_slice(&b) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!("agent.json ignored: {e}");
                    None
                }
            })
            .unwrap_or_default();
        if let Some(b) = env_bool("SELFKIT_BACKUP") {
            c.backup.enabled = b;
        }
        if let Some(b) = env_bool("SELFKIT_BACKUP_KEEP_LOCAL") {
            c.backup.keep_local = b;
        }
        if let Some(b) = env_bool("SELFKIT_REMINDERS") {
            c.reminders.enabled = b;
        }
        c
    }

    pub fn server_db_limit_bytes(&self) -> u64 {
        self.server_db_limit_mb.unwrap_or(512) * 1024 * 1024
    }
}
