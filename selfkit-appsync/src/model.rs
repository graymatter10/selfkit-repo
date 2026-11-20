//! Settings (per device, non-secret) and status/state types. Both are JSON files in
//! `<data_dir>/sync/`, the one place every Selfkit app on a device reads:
//! `settings.json` (shared) and `state-<app>.json` (per app).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use selfkit_core::Result;

/// Who runs sync for this app on this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncOwner {
    /// This app syncs itself (Android default; desktop fallback without an agent).
    #[default]
    App,
    /// A future Selfkit companion app syncs every app (through the ContentProviders);
    /// apps step back: sync_now asks the companion, background jobs are cancelled.
    Companion,
    /// Desktop: the background agent syncs every app DB; the app only asks it.
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransportMode {
    /// Same network and a peer reachable -> LAN, otherwise server.
    #[default]
    Auto,
    LanOnly,
    ServerOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncSettings {
    /// Off after install/upgrade until the user turns it on in the Sync page
    /// (test builds share a public key, so nothing syncs without consent).
    pub enabled: bool,
    pub owner: SyncOwner,
    pub transport: TransportMode,
    /// Sync on open and in the background job.
    pub auto_sync: bool,
    /// Background job period (Android WorkManager minimum is 15).
    pub background_minutes: u32,
    pub lan_enabled: bool,
    /// Extra LAN peers by address ("192.168.1.20:47617"), for networks where broadcast fails.
    pub lan_peers: Vec<String>,
    /// UDP discovery port (agent default 47616).
    pub lan_discovery_port: u16,
    /// How long one pass listens for beacons.
    pub lan_discovery_ms: u64,
    /// Shown to other devices; None = platform default.
    pub device_name: Option<String>,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            owner: SyncOwner::App,
            transport: TransportMode::Auto,
            auto_sync: true,
            background_minutes: 15,
            lan_enabled: true,
            lan_peers: vec![],
            lan_discovery_port: selfkit_core::lan::discovery::DISCOVERY_PORT,
            lan_discovery_ms: 2500,
            device_name: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Lan,
    Server,
    /// Desktop agent asked to sync (it picks its own transports).
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Manual,
    Open,
    Background,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PassReport {
    pub pulled: u64,
    pub pushed: u64,
    pub conflicts: u64,
    pub undecryptable: u64,
    pub photos_up: u64,
    pub photos_down: u64,
    /// Photos referenced by records but not on the server yet (another device will upload).
    pub photos_waiting: u64,
}

impl PassReport {
    pub fn changes(&self) -> u64 {
        self.pulled + self.pushed + self.photos_up + self.photos_down
    }
}

/// Result of one `sync_now`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncOutcome {
    pub ok: bool,
    /// False when nothing ran (owner elsewhere, already running, not due).
    pub ran: bool,
    pub transport: Option<Transport>,
    /// LAN peer names this pass synced with.
    pub peers: Vec<String>,
    pub report: PassReport,
    /// Plain-language error for the UI.
    pub error: Option<String>,
    /// Technical detail for the log.
    pub detail: Option<String>,
    /// Non-fatal note, e.g. "Photos wait for the server".
    pub note: Option<String>,
    pub started_at: i64,
    pub finished_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeenPeer {
    pub device_id: String,
    pub name: String,
    pub addr: String,
    pub last_seen: i64,
    pub last_sync: Option<i64>,
}

/// Persisted per-app state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncState {
    pub last_attempt_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub last: Option<SyncOutcome>,
    pub peers: BTreeMap<String, SeenPeer>,
    /// Newest first, max 20.
    pub log: Vec<SyncOutcome>,
}

/// What the Sync settings screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStatus {
    pub app: String,
    pub device_id: String,
    pub device_name: String,
    pub owner: SyncOwner,
    pub running: bool,
    pub unlocked: bool,
    /// True when the built-in test key is in use (test builds only).
    pub test_key: bool,
    /// Desktop without a usable keychain: sync secrets live in `<data_dir>/sync/secrets.json`
    /// (0600) instead (C80); the Sync page says so.
    #[serde(default)]
    pub secrets_in_file: bool,
    pub encryption_set_up: bool,
    /// The passphrase was changed here and the server doesn't have the new keys.json yet
    /// (R-6): new devices still unlock with the old passphrase until it does.
    #[serde(default)]
    pub keys_unpublished: bool,
    pub server_configured: bool,
    pub photos_configured: bool,
    pub last_attempt_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub last_transport: Option<Transport>,
    pub last_peers: Vec<String>,
    pub last_error: Option<String>,
    pub last_note: Option<String>,
    /// Local changes not yet sent anywhere.
    pub pending: i64,
    pub conflicts: i64,
    pub peers: Vec<SeenPeer>,
    /// Names of all devices that sync (device id → name), from the shared `devices` records.
    pub device_names: BTreeMap<String, String>,
    /// Ready-made line, e.g. "Synced locally with Linux PC · 2 min ago".
    pub headline: String,
    pub next_auto_at: Option<i64>,
}

pub(crate) fn sync_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("sync")
}

pub(crate) fn load_json<T: for<'de> Deserialize<'de> + Default>(p: &Path) -> T {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub(crate) fn save_json<T: Serialize>(p: &Path, v: &T) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(tmp, p)?;
    Ok(())
}

/// "2 min ago", "3 h ago", "yesterday", "4 days ago".
pub fn ago(now: i64, t: i64) -> String {
    let s = ((now - t) / 1000).max(0);
    match s {
        0..=44 => "just now".into(),
        45..=5399 => format!("{} min ago", ((s + 30) / 60).max(1)),
        5400..=86399 => format!("{} h ago", (s + 1800) / 3600),
        86400..=172799 => "yesterday".into(),
        _ => format!("{} days ago", s / 86400),
    }
}
