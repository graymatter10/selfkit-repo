//! Tauri commands shared by every app (feature `tauri`). An app adds them to its
//! own handler list, so they need no plugin permissions:
//!
//! ```ignore
//! app.manage(selfkit_appsync::tauri_cmds::SyncHandle::new(sync));
//! .invoke_handler(tauri::generate_handler![..., selfkit_appsync::tauri_cmds::sync_status, ...])
//! ```
//! The shared UI (`ui/src/sync/`) calls these by name. After a pass that changed
//! data, the event `data-changed` is emitted so screens reload.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::{AppSync, Choice, ConflictView, ServerConfig, SyncOutcome, SyncSettings, SyncStatus, Trigger};

#[derive(Clone)]
pub struct SyncHandle(pub Arc<AppSync>);

impl SyncHandle {
    pub fn new(s: AppSync) -> Self {
        Self(Arc::new(s))
    }
}

type Res<T> = Result<T, String>;

fn e(err: selfkit_core::Error) -> String {
    crate::explain(&err)
}

/// Status plus what the screen needs to know about this build/platform.
#[derive(Serialize)]
pub struct StatusView {
    #[serde(flatten)]
    pub status: SyncStatus,
    pub platform: &'static str,
    pub settings: SyncSettings,
}

#[tauri::command]
pub async fn sync_status(h: State<'_, SyncHandle>) -> Res<StatusView> {
    let s = h.0.clone();
    Ok(StatusView {
        status: s.status().await.map_err(e)?,
        platform: if cfg!(target_os = "android") { "android" } else { "desktop" },
        settings: s.settings(),
    })
}

#[tauri::command]
pub async fn sync_now(app: AppHandle, h: State<'_, SyncHandle>, trigger: Option<String>) -> Res<SyncOutcome> {
    let s = h.0.clone();
    let t = match trigger.as_deref() {
        Some("open") => Trigger::Open,
        Some("background") => Trigger::Background,
        _ => Trigger::Manual,
    };
    let o = s.sync_now(t).await;
    if o.report.pulled > 0 || o.report.photos_down > 0 {
        let _ = app.emit("data-changed", ());
    }
    Ok(o)
}

#[tauri::command]
pub fn sync_settings_set(h: State<'_, SyncHandle>, settings: SyncSettings) -> Res<SyncSettings> {
    h.0.set_settings(&settings).map_err(e)?;
    Ok(h.0.settings())
}

/// Server settings without secrets (for the form: host, database, user, bucket...).
#[derive(Serialize, Deserialize, Default)]
pub struct ServerView {
    pub configured: bool,
    /// Connection string with the password removed.
    pub pg_url: String,
    pub has_password: bool,
    pub s3_endpoint: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_access_key: String,
    pub has_s3_secret: bool,
}

fn strip_password(url: &str) -> (String, bool) {
    // postgres://user:pass@host/db -> postgres://user@host/db
    if let Some((scheme, rest)) = url.split_once("://")
        && let Some((auth, host)) = rest.split_once('@')
        && let Some((user, _pw)) = auth.split_once(':')
    {
        return (format!("{scheme}://{user}@{host}"), true);
    }
    (url.to_string(), false)
}

#[tauri::command]
pub fn sync_server_get(h: State<'_, SyncHandle>) -> Res<ServerView> {
    let Some(c) = h.0.server_config().map_err(e)? else { return Ok(ServerView::default()) };
    let (pg_url, has_password) = strip_password(&c.pg_url);
    let s3 = c.s3.unwrap_or_else(|| selfkit_core::sync::s3::S3Config {
        endpoint: String::new(),
        bucket: String::new(),
        region: String::new(),
        access_key: String::new(),
        secret_key: String::new(),
        path_style: true,
    });
    Ok(ServerView {
        configured: true,
        pg_url,
        has_password,
        s3_endpoint: s3.endpoint,
        s3_bucket: s3.bucket,
        s3_region: s3.region,
        s3_access_key: s3.access_key,
        has_s3_secret: !s3.secret_key.is_empty(),
    })
}

/// What the form sends. Empty password/secret = keep the stored one.
#[derive(Deserialize)]
pub struct ServerForm {
    pub pg_url: String,
    #[serde(default)]
    pub pg_password: String,
    #[serde(default)]
    pub s3_endpoint: String,
    #[serde(default)]
    pub s3_bucket: String,
    #[serde(default)]
    pub s3_region: String,
    #[serde(default)]
    pub s3_access_key: String,
    #[serde(default)]
    pub s3_secret: String,
}

fn build_config(sync: &AppSync, f: &ServerForm) -> Res<Option<ServerConfig>> {
    let url = f.pg_url.trim();
    if url.is_empty() {
        return Ok(None);
    }
    let old = sync.server_config().map_err(e)?;
    let old_pw = old.as_ref().and_then(|c| {
        let rest = c.pg_url.split_once("://")?.1;
        let auth = rest.split_once('@')?.0;
        auth.split_once(':').map(|(_, p)| p.to_string())
    });
    let pw = if f.pg_password.is_empty() { old_pw } else { Some(f.pg_password.clone()) };
    let pg_url = match (pw, url.split_once("://")) {
        (Some(pw), Some((scheme, rest))) if !rest.split_once('@').map(|(a, _)| a.contains(':')).unwrap_or(false) => match rest.split_once('@') {
            Some((user, host)) => format!("{scheme}://{user}:{pw}@{host}"),
            None => url.to_string(),
        },
        _ => url.to_string(),
    };
    let s3 = if f.s3_endpoint.trim().is_empty() {
        None
    } else {
        let old_secret = old.as_ref().and_then(|c| c.s3.as_ref()).map(|s| s.secret_key.clone()).unwrap_or_default();
        Some(selfkit_core::sync::s3::S3Config {
            endpoint: f.s3_endpoint.trim().into(),
            bucket: f.s3_bucket.trim().into(),
            region: if f.s3_region.trim().is_empty() { "us-east-1".into() } else { f.s3_region.trim().into() },
            access_key: f.s3_access_key.trim().into(),
            secret_key: if f.s3_secret.is_empty() { old_secret } else { f.s3_secret.clone() },
            path_style: true,
        })
    };
    Ok(Some(ServerConfig { pg_url, s3 }))
}

#[tauri::command]
pub async fn sync_server_test(h: State<'_, SyncHandle>, form: ServerForm) -> Res<()> {
    let s = h.0.clone();
    let cfg = build_config(&s, &form)?.ok_or("Enter the Postgres connection first.")?;
    s.test_server(&cfg).await
}

#[tauri::command]
pub fn sync_server_set(h: State<'_, SyncHandle>, form: ServerForm) -> Res<ServerView> {
    let cfg = build_config(&h.0, &form)?;
    h.0.set_server(cfg.as_ref()).map_err(e)?;
    sync_server_get(h)
}

/// First device: returns the recovery key (show once).
#[tauri::command]
pub fn sync_encryption_setup(h: State<'_, SyncHandle>, passphrase: String) -> Res<String> {
    if passphrase.chars().count() < 8 {
        return Err("Use at least 8 characters.".into());
    }
    h.0.setup_encryption(&passphrase).map_err(e)
}

/// Other devices: unlock with the passphrase or the recovery key.
#[tauri::command]
pub async fn sync_unlock(h: State<'_, SyncHandle>, secret: String, recovery: bool) -> Res<()> {
    let s = h.0.clone();
    let r = if recovery { s.unlock_recovery(&secret).await } else { s.unlock(&secret).await };
    r.map_err(|err| match err {
        selfkit_core::Error::BadKey => if recovery { "That recovery key doesn't match." } else { "Wrong passphrase." }.to_string(),
        selfkit_core::Error::Io(_) => "No key file here yet. Set up the server first, or use the recovery key on a device that has one.".to_string(),
        other => crate::explain(&other),
    })
}

#[tauri::command]
pub fn sync_change_passphrase(h: State<'_, SyncHandle>, old: String, new: String) -> Res<()> {
    if new.chars().count() < 8 {
        return Err("Use at least 8 characters.".into());
    }
    h.0.change_passphrase(&old, &new).map_err(|err| match err {
        selfkit_core::Error::BadKey => "The current passphrase is wrong.".to_string(),
        other => crate::explain(&other),
    })
}

#[tauri::command]
pub fn sync_conflicts(h: State<'_, SyncHandle>) -> Res<Vec<ConflictView>> {
    h.0.conflicts().map_err(e)
}

#[tauri::command]
pub fn sync_resolve(app: AppHandle, h: State<'_, SyncHandle>, store: String, id: String, choice: Choice) -> Res<()> {
    h.0.resolve(&store, &id, &choice).map_err(e)?;
    let _ = app.emit("data-changed", ());
    Ok(())
}

#[tauri::command]
pub fn sync_log(h: State<'_, SyncHandle>) -> Res<Vec<SyncOutcome>> {
    Ok(h.0.state().log)
}

/// The usual setup for an app: platform secret store (Android Keystore-sealed file /
/// desktop keychain), default photo folder `<app>-photos`, then `AppSync`.
pub fn default_handle(app: &str, data_dir: std::path::PathBuf) -> selfkit_core::Result<SyncHandle> {
    #[cfg(all(target_os = "android", feature = "android"))]
    let secrets: Arc<dyn crate::SecretStore> = Arc::new(crate::android::secrets(&data_dir));
    #[cfg(not(target_os = "android"))]
    let secrets: Arc<dyn crate::SecretStore> = Arc::new(crate::secrets::KeychainSecrets);
    #[cfg(all(target_os = "android", not(feature = "android")))]
    let secrets: Arc<dyn crate::SecretStore> = Arc::new(crate::secrets::FileSecrets::new(data_dir.join("sync").join("secrets.json"), None));
    let cfg = crate::AppSyncConfig::new(app, data_dir, secrets);
    Ok(SyncHandle::new(AppSync::new(cfg)?))
}

/// Desktop: one pass on start (asks the agent when it owns sync). Android runs
/// "on open" through the Kotlin plugin (WorkManager) instead.
pub fn spawn_on_open(app: &AppHandle, h: &SyncHandle) {
    if cfg!(target_os = "android") {
        return;
    }
    let (app, s) = (app.clone(), h.0.clone());
    tauri::async_runtime::spawn(async move {
        let o = s.sync_now(Trigger::Open).await;
        if o.report.pulled > 0 {
            let _ = app.emit("data-changed", ());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_is_stripped() {
        assert_eq!(strip_password("postgres://me:hunter2@db.host/selfkit?sslmode=require"), ("postgres://me@db.host/selfkit?sslmode=require".to_string(), true));
        assert_eq!(strip_password("postgres://me@h/db"), ("postgres://me@h/db".to_string(), false));
    }
}
