//! State shared between the tracker thread, the sync loop and IPC handlers.

use std::sync::Mutex;

use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::ipc::Request;

#[derive(Default)]
pub struct Shared {
    pub inner: Mutex<Inner>,
    pub sync_now: Notify,
    pub backup_now: Notify,
    /// Set by `backup_now {full: true}`.
    pub backup_full: std::sync::atomic::AtomicBool,
    pub lan: std::sync::Arc<selfkit_core::lan::LanState>,
}

#[derive(Default, Debug, Clone, serde::Serialize)]
pub struct Inner {
    pub started_at: i64,
    pub tracking: bool,
    pub tracking_error: Option<String>,
    /// Paused through the agent (`pause` command / IPC); in memory only.
    pub paused: bool,
    /// Chrono's `chrono_settings.paused_until` (UTC ms), as last read by the tracker.
    pub paused_until: Option<i64>,
    pub excluded_apps: usize,
    pub current_app: Option<String>,
    pub sync_configured: bool,
    pub last_sync_at: Option<i64>,
    pub last_sync: Option<Value>,
    pub last_sync_error: Option<String>,
    pub last_sync_ok_at: Option<i64>,
    pub server_db_bytes: Option<i64>,
    pub reminders_fired: u64,
    pub reminders_last_fired_at: Option<i64>,
    pub lan_enabled: bool,
    pub lan_error: Option<String>,
}

impl Shared {
    pub async fn handle(&self, req: Request) -> Value {
        match req {
            Request::Status => {
                let s = self.inner.lock().unwrap().clone();
                let lan = json!({"port": *self.lan.port.lock().unwrap(), "peers": self.lan.snapshot()});
                json!({"ok": true, "status": s, "lan": lan, "rss_kib": crate::health::rss_kib(), "version": env!("CARGO_PKG_VERSION")})
            }
            Request::Pause | Request::Resume => {
                let mut g = self.inner.lock().unwrap();
                g.paused = matches!(req, Request::Pause);
                json!({"ok": true, "paused": g.paused})
            }
            Request::SyncNow => {
                self.sync_now.notify_one();
                self.lan.sync_now.notify_one();
                json!({"ok": true})
            }
            Request::Health => {
                // Touches the disk briefly (DB sizes, log tail); a few ms.
                json!({"ok": true, "health": crate::health::snapshot(self)})
            }
            Request::BackupNow { full } => {
                self.backup_full.store(full, std::sync::atomic::Ordering::Relaxed);
                self.backup_now.notify_one();
                json!({"ok": true, "started": true})
            }
            Request::Today => match crate::today() {
                Ok(v) => json!({"ok": true, "today": v}),
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            },
        }
    }
}
