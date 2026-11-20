//! Server transport: Postgres records (selfkit-core `sync_once`) plus photos on S3.
//! Guards added here: a key check stored on the server (a device with the wrong
//! key stops before pushing anything) and a schema check (a server set up by a
//! newer Selfkit is refused instead of half-understood).

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde::{Deserialize, Serialize};
use selfkit_core::crypto::{self, SecretKey};
use selfkit_core::sync::pg::PgRemote;
use selfkit_core::sync::s3::{BlobStore, S3Config};
use selfkit_core::sync::sync_once;
use selfkit_core::{Error, Result, Store};

use crate::explain;
use crate::model::PassReport;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub pg_url: String,
    pub s3: Option<S3Config>,
}

/// Where an app keeps photos: records in `collection` with a `file` field,
/// bytes at `<data_dir>/<dir>/<file>`. Blob id on S3 = record id.
#[derive(Debug, Clone)]
pub struct PhotoConfig {
    pub collection: String,
    pub dir: String,
}

const KEYCHECK: &str = "keycheck";
/// `keys.json` (data key wrapped by passphrase and recovery key; safe to store) in sk_meta,
/// so a new device or app can unlock without S3.
const KEYFILE: &str = "keyfile";
const KEYCHECK_PLAIN: &[u8] = b"selfkit-keycheck-v1";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

pub async fn connect(url: &str) -> Result<PgRemote> {
    tokio::time::timeout(CONNECT_TIMEOUT, PgRemote::connect(url))
        .await
        .map_err(|_| Error::Remote(format!("{}: postgres connect", explain::TIMEOUT)))?
}

/// Refuse servers from a newer Selfkit; prove our key matches (or claim the server).
pub async fn preflight(remote: &PgRemote, key: &SecretKey) -> Result<()> {
    let (srv, known) = (remote.server_schema_version().await?, PgRemote::known_schema_version());
    if srv > known {
        return Err(Error::Remote(format!("{}: server schema {srv} > {known}", explain::SCHEMA_NEWER)));
    }
    match remote.meta_get(KEYCHECK).await? {
        Some(v) => {
            let ct = B64.decode(v.trim()).map_err(|_| Error::Format("keycheck base64".into()))?;
            if crypto::open(key, &ct, KEYCHECK.as_bytes()).ok().as_deref() != Some(KEYCHECK_PLAIN) {
                return Err(Error::Remote(format!("{}: server key check failed", explain::WRONG_KEY)));
            }
        }
        None => remote.meta_set(KEYCHECK, &B64.encode(crypto::seal(key, KEYCHECK_PLAIN, KEYCHECK.as_bytes()))).await?,
    }
    Ok(())
}

pub async fn sync_records(remote: &PgRemote, key: &SecretKey, stores: &mut HashMap<String, Store>, rep: &mut PassReport) -> Result<()> {
    let mut names: Vec<String> = stores.keys().cloned().collect();
    names.sort();
    for app in names {
        let store = stores.get_mut(&app).expect("store");
        let r = sync_once(store, remote, key).await?;
        rep.pulled += r.pulled;
        rep.pushed += r.pushed;
        rep.conflicts += r.conflicts;
        rep.undecryptable += r.undecryptable;
    }
    Ok(())
}

fn uploaded_key(id: &str) -> String {
    format!("appsync.blob.up.{id}")
}

/// Upload photos this device has and the server lacks; download photos records
/// point to but this device lacks. Whole files in RAM (document-quality WebP, a few MB).
pub async fn sync_photos(blobs: &BlobStore, key: &SecretKey, store: &mut Store, data_dir: &Path, pc: &PhotoConfig, rep: &mut PassReport) -> Result<()> {
    let rows: Vec<(String, String, bool)> = {
        let mut st = store
            .conn()
            .prepare("SELECT id, json_extract(data, '$.file'), deleted FROM records WHERE collection=? AND json_extract(data, '$.file') IS NOT NULL")?;
        st.query_map([&pc.collection], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0)))?
            .collect::<std::result::Result<_, _>>()?
    };
    let dir = data_dir.join(&pc.dir);
    for (id, file, deleted) in rows {
        if deleted || file.contains(['/', '\\']) || file.contains("..") {
            continue;
        }
        let path = dir.join(&file);
        let done = store.kv_get(&uploaded_key(&id))?.is_some();
        if path.exists() {
            if !done {
                let bytes = tokio::fs::read(&path).await?;
                blobs.put_blob(key, &id, &bytes).await?;
                store.kv_set(&uploaded_key(&id), "1")?;
                rep.photos_up += 1;
            }
        } else {
            match blobs.get_blob(key, &id).await? {
                Some(bytes) => {
                    tokio::fs::create_dir_all(&dir).await?;
                    let tmp = dir.join(format!(".{file}.part"));
                    tokio::fs::write(&tmp, &bytes).await?;
                    tokio::fs::rename(&tmp, &path).await?;
                    store.kv_set(&uploaded_key(&id), "1")?;
                    rep.photos_down += 1;
                }
                None => rep.photos_waiting += 1,
            }
        }
    }
    Ok(())
}

/// Publish this device's `keys.json` (wrapped keys only) when the server has none or an
/// older one (`changed_at`). Before, any difference uploaded, so a device that still had the
/// pre-change file put the old passphrase back after a passphrase change (R-2).
pub async fn upload_keyfile(remote: &PgRemote, s3: Option<&S3Config>, path: &Path) -> Result<()> {
    let Ok(local) = std::fs::read_to_string(path) else { return Ok(()) };
    let stamp = |t: &str| serde_json::from_str::<selfkit_core::keys::KeyFile>(t).map(|k| k.changed_at).unwrap_or(0);
    let server = remote.meta_get(KEYFILE).await?;
    if server.as_deref().is_none_or(|srv| srv != local && stamp(&local) > stamp(srv)) {
        remote.meta_set(KEYFILE, &local).await?;
        if let Some(s3) = s3 {
            // Same place the desktop agent uses (restore, `selfkit-agent unlock`).
            let _ = BlobStore::new(s3.clone())?.put_bundle("_keys", "keys.json", path).await;
        }
    } else if let Some(srv) = server.as_deref()
        && stamp(srv) > stamp(&local)
        && serde_json::from_str::<selfkit_core::keys::KeyFile>(srv).is_ok()
    {
        // The passphrase was changed on another device: keep its newer file here too, so
        // this device publishes nothing stale and unlocks with the new passphrase later.
        std::fs::write(path, srv)?;
    }
    Ok(())
}

/// The published `keys.json`, if any.
pub async fn download_keyfile(remote: &PgRemote) -> Result<Option<String>> {
    remote.meta_get(KEYFILE).await
}

/// Moving from the public test key to the user's own key: if the server was set up
/// with the test key, wipe its rows and claim it with the new key (every device
/// re-uploads after unlocking). Returns true when it did.
pub async fn take_over_from_test_key(remote: &PgRemote, new_key: &SecretKey) -> Result<bool> {
    let Some(v) = remote.meta_get(KEYCHECK).await? else { return Ok(false) };
    let ct = B64.decode(v.trim()).map_err(|_| Error::Format("keycheck base64".into()))?;
    if crypto::open(&crate::test_data_key(), &ct, KEYCHECK.as_bytes()).ok().as_deref() != Some(KEYCHECK_PLAIN) {
        return Ok(false);
    }
    remote.wipe_records().await?;
    remote.meta_set(KEYCHECK, &B64.encode(crypto::seal(new_key, KEYCHECK_PLAIN, KEYCHECK.as_bytes()))).await?;
    Ok(true)
}

/// "Test" button in server setup: connect, migrate, check schema/key, write+read storage.
pub async fn test(cfg: &ServerConfig, key: Option<&SecretKey>) -> Result<()> {
    let remote = connect(&cfg.pg_url).await?;
    if let Some(k) = key {
        let (srv, known) = (remote.server_schema_version().await?, PgRemote::known_schema_version());
        if srv > known {
            return Err(Error::Remote(format!("{}: server schema {srv} > {known}", explain::SCHEMA_NEWER)));
        }
        if let Some(v) = remote.meta_get(KEYCHECK).await? {
            let ct = B64.decode(v.trim()).map_err(|_| Error::Format("keycheck base64".into()))?;
            if crypto::open(k, &ct, KEYCHECK.as_bytes()).is_err() {
                return Err(Error::Remote(format!("{}: server key check failed", explain::WRONG_KEY)));
            }
        }
    }
    if let Some(s3) = &cfg.s3 {
        let b = BlobStore::new(s3.clone())?;
        b.ensure_bucket().await?;
        b.check().await?;
    }
    Ok(())
}
