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
pub async fn sync_photos(blobs: &BlobStore, key: &SecretKey, store: &Store, data_dir: &Path, pc: &PhotoConfig, rep: &mut PassReport) -> Result<()> {
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
