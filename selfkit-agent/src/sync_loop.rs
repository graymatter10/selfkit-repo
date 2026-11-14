//! Periodic sync of every app DB in the shared data folder (every 5 min, or
//! on `sync_now`). Needs the data key (keychain or SELFKIT_DATA_KEY) and a
//! Postgres URL (SELFKIT_PG_URL or keychain). Without them it idles.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use selfkit_core::crypto::SecretKey;
use selfkit_core::sync::{SyncReport, pg::PgRemote, sync_once};
use selfkit_core::{Store, keychain, paths, time};

use crate::state::Shared;

fn interval() -> Duration {
    Duration::from_secs(std::env::var("SELFKIT_SYNC_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(300))
}

pub fn data_key() -> Result<Option<SecretKey>> {
    let b64 = match std::env::var("SELFKIT_DATA_KEY") {
        Ok(v) => Some(v),
        Err(_) => keychain::get(keychain::DATA_KEY).unwrap_or(None),
    };
    b64.map(|s| {
        let raw = base64::engine::general_purpose::STANDARD.decode(s.trim()).context("data key base64")?;
        Ok(SecretKey::from_slice(&raw)?)
    })
    .transpose()
}

fn pg_url() -> Option<String> {
    std::env::var("SELFKIT_PG_URL").ok().or_else(|| keychain::get(keychain::PG_URL).ok().flatten())
}

async fn sync_all(key: &SecretKey, url: &str) -> Result<SyncReport> {
    let remote = PgRemote::connect(url).await?;
    let device = paths::device_id()?;
    let mut total = SyncReport::default();
    let apps = paths::APPS.iter().copied().chain(std::iter::once(selfkit_core::settings::APP));
    for app in apps {
        let p = paths::app_db(app);
        if !p.exists() {
            continue;
        }
        let mut store = Store::open(&p, app, &device, &[])?;
        let r = sync_once(&mut store, &remote, key).await.with_context(|| format!("sync {app}"))?;
        total.pulled += r.pulled;
        total.pushed += r.pushed;
        total.conflicts += r.conflicts;
        total.undecryptable += r.undecryptable;
    }
    Ok(total)
}

pub async fn run(shared: Arc<Shared>) {
    loop {
        let cfg = (data_key(), pg_url());
        match cfg {
            (Ok(Some(key)), Some(url)) => {
                shared.inner.lock().unwrap().sync_configured = true;
                let res = sync_all(&key, &url).await;
                let mut g = shared.inner.lock().unwrap();
                g.last_sync_at = Some(time::now_ms());
                match res {
                    Ok(r) => {
                        tracing::info!(?r, "sync done");
                        g.last_sync = serde_json::to_value(r).ok();
                        g.last_sync_error = None;
                    }
                    Err(e) => {
                        tracing::warn!("sync failed: {e:#}");
                        g.last_sync_error = Some(format!("{e:#}"));
                    }
                }
            }
            (Err(e), _) => shared.inner.lock().unwrap().last_sync_error = Some(format!("{e:#}")),
            _ => shared.inner.lock().unwrap().sync_configured = false,
        }
        tokio::select! {
            _ = tokio::time::sleep(interval()) => {}
            _ = shared.sync_now.notified() => {}
        }
    }
}

fn device_name() -> String {
    std::env::var("SELFKIT_DEVICE_NAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok().map(|s| s.trim().to_string()))
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "selfkit-device".into())
}

/// LAN sync (server-less, encrypted). Disabled with SELFKIT_LAN=0. Extra peers
/// by address: SELFKIT_LAN_PEERS="192.168.1.20:47617,10.0.0.5:47617".
pub async fn run_lan(shared: Arc<Shared>) {
    if std::env::var("SELFKIT_LAN").as_deref() == Ok("0") {
        return;
    }
    // Wait until unlocked (data key available).
    let key = loop {
        if let Ok(Some(k)) = data_key() {
            break k;
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    };
    let device = match paths::device_id() {
        Ok(d) => d,
        Err(e) => {
            shared.inner.lock().unwrap().lan_error = Some(e.to_string());
            return;
        }
    };
    let env_u16 = |k: &str| std::env::var(k).ok().and_then(|s| s.parse().ok());
    let static_peers = std::env::var("SELFKIT_LAN_PEERS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let cfg = selfkit_core::lan::LanConfig {
        data_key: key,
        device_id: device.clone(),
        device_name: device_name(),
        tcp_port: env_u16("SELFKIT_LAN_PORT").unwrap_or(0),
        discovery_port: env_u16("SELFKIT_LAN_DISCOVERY_PORT").unwrap_or(selfkit_core::lan::discovery::DISCOVERY_PORT),
        static_peers,
        interval: Duration::from_secs(env_u16("SELFKIT_LAN_SECS").map(u64::from).unwrap_or(120)),
    };
    // Every app DB is opened (created if missing) so a fresh device can receive everything.
    let stores: selfkit_core::lan::StoreFactory = Arc::new(move || {
        let mut m = std::collections::HashMap::new();
        for app in paths::APPS.iter().copied().chain(std::iter::once(selfkit_core::settings::APP)) {
            m.insert(app.to_string(), Store::open(paths::app_db(app), app, &device, &[])?);
        }
        Ok(m)
    });
    shared.inner.lock().unwrap().lan_enabled = true;
    if let Err(e) = selfkit_core::lan::serve(cfg, stores, shared.lan.clone()).await {
        tracing::warn!("lan sync stopped: {e}");
        let mut g = shared.inner.lock().unwrap();
        g.lan_enabled = false;
        g.lan_error = Some(e.to_string());
    }
}
