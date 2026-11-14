//! Sync engine: encrypts dirty records, pushes with optimistic concurrency,
//! pulls by server `rev` cursor. Transport-agnostic via [`Remote`].
//! See docs/data-and-sync.md.

use std::collections::HashSet;
use std::future::Future;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::crypto::{self, SecretKey};
use crate::store::{Applied, Record};
use crate::{Error, Result, Store};

pub mod memory;
#[cfg(feature = "remote")]
pub mod pg;
#[cfg(feature = "remote")]
pub mod s3;

/// What the server stores: routing metadata in clear, everything else encrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRecord {
    pub id: String,
    pub app: String,
    pub collection: String,
    pub updated_at: i64,
    pub device_id: String,
    pub ciphertext: Vec<u8>,
    /// Server revision (ignored on push).
    pub rev: i64,
}

pub trait Remote: Send + Sync {
    /// Insert (base_rev = 0) or update if the server row still has `base_rev`.
    /// Returns the new rev, or `None` on a concurrency conflict.
    fn push(&self, rec: &RemoteRecord, base_rev: i64) -> impl Future<Output = Result<Option<i64>>> + Send;
    /// Rows of `app` with rev > since, ascending, at most `limit`.
    fn pull(&self, app: &str, since: i64, limit: i64) -> impl Future<Output = Result<Vec<RemoteRecord>>> + Send;
    fn fetch(&self, id: &str) -> impl Future<Output = Result<Option<RemoteRecord>>> + Send;
}

#[derive(Serialize, Deserialize)]
struct Payload {
    data: Value,
    tz_offset: i32,
    deleted: bool,
    /// Version history, so LAN-received copies don't false-conflict with the server.
    #[serde(default)]
    hist: Vec<String>,
}

fn aad(app: &str, id: &str, collection: &str) -> Vec<u8> {
    format!("{app}|{id}|{collection}").into_bytes()
}

pub fn encrypt_record(key: &SecretKey, app: &str, r: &Record) -> Result<RemoteRecord> {
    let p = serde_json::to_vec(&Payload { data: r.data.clone(), tz_offset: r.tz_offset, deleted: r.deleted, hist: r.hist.clone() })?;
    Ok(RemoteRecord {
        id: r.id.clone(),
        app: app.into(),
        collection: r.collection.clone(),
        updated_at: r.updated_at,
        device_id: r.device_id.clone(),
        ciphertext: crypto::seal(key, &p, &aad(app, &r.id, &r.collection)),
        rev: 0,
    })
}

pub fn decrypt_record(key: &SecretKey, rr: &RemoteRecord) -> Result<Record> {
    let pt = crypto::open(key, &rr.ciphertext, &aad(&rr.app, &rr.id, &rr.collection))?;
    let p: Payload = serde_json::from_slice(&pt)?;
    Ok(Record {
        id: rr.id.clone(),
        collection: rr.collection.clone(),
        data: p.data,
        updated_at: rr.updated_at,
        tz_offset: p.tz_offset,
        device_id: rr.device_id.clone(),
        deleted: p.deleted,
        hist: p.hist,
    })
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub pulled: u64,
    pub pushed: u64,
    pub conflicts: u64,
    /// Rows that failed to decrypt (wrong key / corrupt); skipped, not fatal.
    pub undecryptable: u64,
}

const CURSOR_KEY: &str = "sync.cursor";
const PAGE: i64 = 500;

async fn pull<R: Remote>(store: &mut Store, remote: &R, key: &SecretKey, rep: &mut SyncReport) -> Result<()> {
    let mut cursor: i64 = store.kv_get(CURSOR_KEY)?.and_then(|s| s.parse().ok()).unwrap_or(0);
    loop {
        let rows = remote.pull(store.app(), cursor, PAGE).await?;
        let n = rows.len() as i64;
        for rr in rows {
            cursor = cursor.max(rr.rev);
            match decrypt_record(key, &rr) {
                Ok(rec) => {
                    match store.apply_remote(&rec, rr.rev)? {
                        Applied::Inserted | Applied::Updated => rep.pulled += 1,
                        Applied::Conflict => rep.conflicts += 1,
                        Applied::Unchanged => {}
                    }
                }
                Err(Error::Decrypt) => {
                    tracing::warn!(id = rr.id, "undecryptable remote record skipped");
                    rep.undecryptable += 1;
                }
                Err(e) => return Err(e),
            }
        }
        store.kv_set(CURSOR_KEY, &cursor.to_string())?;
        if n < PAGE {
            return Ok(());
        }
    }
}

/// One full sync round for one app store: pull, then push all dirty records.
pub async fn sync_once<R: Remote>(store: &mut Store, remote: &R, key: &SecretKey) -> Result<SyncReport> {
    let mut rep = SyncReport::default();
    pull(store, remote, key, &mut rep).await?;
    let mut tried: HashSet<String> = HashSet::new();
    loop {
        let batch: Vec<_> = store.dirty(200)?.into_iter().filter(|(r, _)| !tried.contains(&r.id)).collect();
        if batch.is_empty() {
            break;
        }
        for (rec, base) in batch {
            tried.insert(rec.id.clone());
            let rr = encrypt_record(key, store.app(), &rec)?;
            match remote.push(&rr, base).await? {
                Some(rev) => {
                    store.mark_synced(&rec.id, rec.updated_at, rev)?;
                    rep.pushed += 1;
                }
                None => {
                    // Someone else changed it: fetch theirs and let the store decide.
                    if let Some(srv) = remote.fetch(&rec.id).await? {
                        let theirs = decrypt_record(key, &srv)?;
                        if store.apply_remote(&theirs, srv.rev)? == Applied::Conflict {
                            rep.conflicts += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryRemote;
    use super::*;
    use crate::store::Resolution;
    use serde_json::json;

    #[tokio::test]
    async fn two_devices_sync_and_conflict() {
        let key = SecretKey::generate();
        let remote = MemoryRemote::default();
        let mut a = Store::open_in_memory("tickd", "A", &[]).unwrap();
        let mut b = Store::open_in_memory("tickd", "B", &[]).unwrap();

        let t = a.put("tasks", None, &json!({"title": "one"})).unwrap();
        b.put("tasks", None, &json!({"title": "two"})).unwrap();
        assert_eq!(sync_once(&mut a, &remote, &key).await.unwrap().pushed, 1);
        let rb = sync_once(&mut b, &remote, &key).await.unwrap();
        assert_eq!((rb.pulled, rb.pushed), (1, 1));
        sync_once(&mut a, &remote, &key).await.unwrap();
        assert_eq!(a.count("tasks").unwrap(), 2);
        assert_eq!(b.count("tasks").unwrap(), 2);

        // Server only ever sees ciphertext.
        assert!(remote.rows().iter().all(|r| !String::from_utf8_lossy(&r.ciphertext).contains("one")));

        // Concurrent edits -> conflict on the second pusher.
        a.put("tasks", Some(&t.id), &json!({"title": "A edit"})).unwrap();
        b.put("tasks", Some(&t.id), &json!({"title": "B edit"})).unwrap();
        sync_once(&mut a, &remote, &key).await.unwrap();
        let rb = sync_once(&mut b, &remote, &key).await.unwrap();
        assert_eq!(rb.conflicts, 1);
        b.resolve_conflict(&t.id, Resolution::KeepLocal).unwrap();
        assert_eq!(sync_once(&mut b, &remote, &key).await.unwrap().pushed, 1);
        sync_once(&mut a, &remote, &key).await.unwrap();
        assert_eq!(a.get(&t.id).unwrap().unwrap().data["title"], "B edit");

        // Deletes propagate.
        a.delete(&t.id).unwrap();
        sync_once(&mut a, &remote, &key).await.unwrap();
        sync_once(&mut b, &remote, &key).await.unwrap();
        assert_eq!(b.count("tasks").unwrap(), 1);
    }

    #[tokio::test]
    async fn restored_device_does_not_conflict_with_itself() {
        let key = SecretKey::generate();
        let remote = MemoryRemote::default();
        let mut a = Store::open_in_memory("hisab", "A", &[]).unwrap();
        a.put("transactions", None, &json!({"amount": 5})).unwrap();
        sync_once(&mut a, &remote, &key).await.unwrap();
        // "Upload everything" to the same server again: no conflicts, nothing lost.
        a.reset_sync_state().unwrap();
        let r = sync_once(&mut a, &remote, &key).await.unwrap();
        assert_eq!(r.conflicts, 0);
        assert_eq!(a.dirty_count().unwrap(), 0);
    }
}
