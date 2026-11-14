//! State shared between the tracker thread, the sync loop and IPC handlers.

use std::sync::Mutex;

use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::ipc::Request;

#[derive(Default)]
pub struct Shared {
    pub inner: Mutex<Inner>,
    pub sync_now: Notify,
    pub lan: std::sync::Arc<selfkit_core::lan::LanState>,
}

#[derive(Default, Debug, Clone, serde::Serialize)]
pub struct Inner {
    pub started_at: i64,
    pub tracking: bool,
    pub tracking_error: Option<String>,
    pub paused: bool,
    pub current_app: Option<String>,
    pub sync_configured: bool,
    pub last_sync_at: Option<i64>,
    pub last_sync: Option<Value>,
    pub last_sync_error: Option<String>,
    pub lan_enabled: bool,
    pub lan_error: Option<String>,
}

/// Resident memory of this process in KiB (diagnostics).
pub fn rss_kib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = s.split_whitespace().nth(1)?.parse().ok()?;
        return Some(pages * 4);
    }
    #[allow(unreachable_code)]
    None
}

impl Shared {
    pub async fn handle(&self, req: Request) -> Value {
        match req {
            Request::Status => {
                let s = self.inner.lock().unwrap().clone();
                let lan = json!({"port": *self.lan.port.lock().unwrap(), "peers": self.lan.snapshot()});
                json!({"ok": true, "status": s, "lan": lan, "rss_kib": rss_kib(), "version": env!("CARGO_PKG_VERSION")})
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
            Request::Today => match crate::today() {
                Ok(v) => json!({"ok": true, "today": v}),
                Err(e) => json!({"ok": false, "error": e.to_string()}),
            },
        }
    }
}
