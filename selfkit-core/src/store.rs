//! Local SQLite store: generic `(id, collection, json)` records with tombstones,
//! dirty tracking for sync, FTS5 search, conflicts and named migrations.
//! See docs/data-and-sync.md.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result, time};

/// A migration step. `name` must be globally unique, e.g. `"agenda/001_init"`.
pub struct Migration {
    pub name: &'static str,
    pub sql: &'static str,
}

const CORE_MIGRATIONS: &[Migration] = &[Migration {
    name: "core/001_init",
    sql: r#"
CREATE TABLE records (
    id          TEXT PRIMARY KEY,
    collection  TEXT NOT NULL,
    data        TEXT NOT NULL,
    updated_at  INTEGER NOT NULL,
    tz_offset   INTEGER NOT NULL DEFAULT 0,
    device_id   TEXT NOT NULL,
    deleted     INTEGER NOT NULL DEFAULT 0,
    dirty       INTEGER NOT NULL DEFAULT 1,
    base_rev    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX records_collection ON records(collection, deleted, updated_at);
CREATE INDEX records_dirty ON records(dirty) WHERE dirty = 1;

CREATE VIRTUAL TABLE records_fts USING fts5(data, content='records', content_rowid='rowid');
CREATE TRIGGER records_ai AFTER INSERT ON records BEGIN
  INSERT INTO records_fts(rowid, data) VALUES (new.rowid, new.data);
END;
CREATE TRIGGER records_ad AFTER DELETE ON records BEGIN
  INSERT INTO records_fts(records_fts, rowid, data) VALUES ('delete', old.rowid, old.data);
END;
CREATE TRIGGER records_au AFTER UPDATE OF data ON records BEGIN
  INSERT INTO records_fts(records_fts, rowid, data) VALUES ('delete', old.rowid, old.data);
  INSERT INTO records_fts(rowid, data) VALUES (new.rowid, new.data);
END;

CREATE TABLE conflicts (
    id          TEXT PRIMARY KEY,
    collection  TEXT NOT NULL,
    remote      TEXT NOT NULL,   -- JSON of the remote Record
    remote_rev  INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT NOT NULL);
"#,
}, Migration {
    // Version history (for server-less LAN sync) + local change sequence.
    // `hist` = JSON array of the last HIST_LEN versions "updated_at:device_id".
    // `lseq` bumps on every content change (local edit, server pull, peer pull).
    name: "core/002_versions",
    sql: r#"
ALTER TABLE records ADD COLUMN hist TEXT NOT NULL DEFAULT '[]';
ALTER TABLE records ADD COLUMN lseq INTEGER NOT NULL DEFAULT 0;
ALTER TABLE conflicts ADD COLUMN source TEXT NOT NULL DEFAULT 'server';
UPDATE records SET hist = json_array(updated_at || ':' || device_id), lseq = rowid;
CREATE TABLE lseq_counter (n INTEGER NOT NULL);
INSERT INTO lseq_counter VALUES ((SELECT COALESCE(MAX(lseq), 0) FROM records));
CREATE INDEX records_lseq ON records(lseq);
CREATE TRIGGER records_ver_ai AFTER INSERT ON records BEGIN
  UPDATE lseq_counter SET n = n + 1;
  UPDATE records SET lseq = (SELECT n FROM lseq_counter),
    hist = CASE WHEN new.hist = '[]' THEN sk_hist_push('[]', new.updated_at || ':' || new.device_id) ELSE new.hist END
  WHERE rowid = new.rowid;
END;
CREATE TRIGGER records_ver_au AFTER UPDATE OF data, deleted, updated_at, device_id ON records BEGIN
  UPDATE lseq_counter SET n = n + 1;
  UPDATE records SET lseq = (SELECT n FROM lseq_counter),
    hist = CASE WHEN new.hist <> old.hist THEN new.hist
                ELSE sk_hist_push(old.hist, new.updated_at || ':' || new.device_id) END
  WHERE rowid = new.rowid;
END;
"#,
}];

/// How many past versions each record remembers.
pub const HIST_LEN: usize = 16;

/// Append `ver` to a JSON history array, de-duplicated, keeping the last HIST_LEN.
pub fn hist_push(hist: &str, ver: &str) -> String {
    let mut v: Vec<String> = serde_json::from_str(hist).unwrap_or_default();
    v.retain(|x| x != ver);
    v.push(ver.to_string());
    if v.len() > HIST_LEN {
        v.drain(..v.len() - HIST_LEN);
    }
    serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
}

/// New time-ordered record id (UUIDv7).
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub id: String,
    pub collection: String,
    pub data: Value,
    pub updated_at: i64,
    pub tz_offset: i32,
    pub device_id: String,
    pub deleted: bool,
    /// Recent versions, oldest first, ending with this one. Filled by the store.
    #[serde(default)]
    pub hist: Vec<String>,
}

impl Record {
    /// Version id of this edit: `"updated_at:device_id"`.
    pub fn ver(&self) -> String {
        format!("{}:{}", self.updated_at, self.device_id)
    }
    /// Same user-visible content (ignores who/when).
    pub fn same_content(&self, o: &Record) -> bool {
        self.data == o.data && self.deleted == o.deleted && self.collection == o.collection
    }
}

/// What happened when a remote record was applied locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Inserted,
    Updated,
    /// Already had this exact version.
    Unchanged,
    /// Local copy had unsynced edits; stored in `conflicts`.
    Conflict,
}

#[derive(Debug, Clone)]
pub struct Conflict {
    pub local: Record,
    pub remote: Record,
    pub remote_rev: i64,
    /// "server" or "peer" (LAN).
    pub source: String,
}

pub enum Resolution {
    KeepLocal,
    KeepRemote,
    Merged(Value),
}

pub struct Store {
    conn: Connection,
    app: String,
    device_id: String,
}

const COLS: &str = "id, collection, data, updated_at, tz_offset, device_id, deleted, hist";
/// Index of the first column after COLS.
const NCOLS: usize = 8;

fn row_to_record(r: &rusqlite::Row) -> rusqlite::Result<Record> {
    let data: String = r.get(2)?;
    Ok(Record {
        id: r.get(0)?,
        collection: r.get(1)?,
        data: serde_json::from_str(&data).unwrap_or(Value::Null),
        updated_at: r.get(3)?,
        tz_offset: r.get(4)?,
        device_id: r.get(5)?,
        deleted: r.get::<_, i64>(6)? != 0,
        hist: serde_json::from_str(&r.get::<_, String>(7)?).unwrap_or_default(),
    })
}

impl Store {
    /// Open (or create) an app database and run core + app migrations.
    pub fn open(path: impl AsRef<Path>, app: &str, device_id: &str, app_migrations: &[Migration]) -> Result<Self> {
        if let Some(d) = path.as_ref().parent() {
            std::fs::create_dir_all(d)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn, app, device_id, app_migrations)
    }

    /// In-memory store (tests, previews).
    pub fn open_in_memory(app: &str, device_id: &str, app_migrations: &[Migration]) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, app, device_id, app_migrations)
    }

    /// Open another app's DB read-only (Reflect/Dusk reading Chrono/Hisab).
    pub fn open_read_only(path: impl AsRef<Path>, app: &str) -> Result<Self> {
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(Self { conn, app: app.into(), device_id: String::new() })
    }

    fn init(conn: Connection, app: &str, device_id: &str, app_migrations: &[Migration]) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;
             PRAGMA cache_size=-4000; PRAGMA temp_store=MEMORY;
             CREATE TABLE IF NOT EXISTS schema_migrations (name TEXT PRIMARY KEY, applied_at INTEGER NOT NULL);",
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.create_scalar_function(
            "sk_hist_push",
            2,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| Ok(hist_push(&ctx.get::<String>(0)?, &ctx.get::<String>(1)?)),
        )?;
        let mut s = Self { conn, app: app.into(), device_id: device_id.into() };
        s.migrate(CORE_MIGRATIONS)?;
        s.migrate(app_migrations)?;
        Ok(s)
    }

    fn migrate(&mut self, migs: &[Migration]) -> Result<()> {
        for m in migs {
            let done: bool = self
                .conn
                .query_row("SELECT 1 FROM schema_migrations WHERE name=?", [m.name], |_| Ok(true))
                .optional()?
                .unwrap_or(false);
            if done {
                continue;
            }
            // IMMEDIATE takes the write lock first, so two processes opening a fresh DB
            // at once (agent LAN session + app) don't both run the same migration.
            let tx = self.conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let done_now = tx
                .query_row("SELECT 1 FROM schema_migrations WHERE name=?", [m.name], |_| Ok(true))
                .optional()?
                .unwrap_or(false);
            if done_now {
                continue;
            }
            tx.execute_batch(m.sql)?;
            tx.execute("INSERT INTO schema_migrations(name, applied_at) VALUES (?, ?)", params![m.name, time::now_ms()])?;
            tx.commit()?;
            tracing::info!(migration = m.name, "applied");
        }
        Ok(())
    }

    pub fn app(&self) -> &str {
        &self.app
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    /// Escape hatch for app-specific queries/indexes.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // ---- CRUD -----------------------------------------------------------

    /// Create (id = None → new UUIDv7) or replace a record's data.
    pub fn put(&self, collection: &str, id: Option<&str>, data: &Value) -> Result<Record> {
        let rec = Record {
            id: id.map(str::to_owned).unwrap_or_else(new_id),
            collection: collection.into(),
            data: data.clone(),
            updated_at: time::now_ms(),
            tz_offset: time::local_offset_min(),
            device_id: self.device_id.clone(),
            deleted: false,
            hist: Vec::new(),
        };
        self.write_local(&rec)?;
        self.get(&rec.id)?.ok_or_else(|| Error::NotFound(rec.id))
    }

    fn write_local(&self, r: &Record) -> Result<()> {
        self.conn.execute(
            "INSERT INTO records (id, collection, data, updated_at, tz_offset, device_id, deleted, dirty)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)
             ON CONFLICT(id) DO UPDATE SET collection=excluded.collection, data=excluded.data,
               updated_at=excluded.updated_at, tz_offset=excluded.tz_offset, device_id=excluded.device_id,
               deleted=excluded.deleted, dirty=1",
            params![r.id, r.collection, r.data.to_string(), r.updated_at, r.tz_offset, r.device_id, r.deleted as i64],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<Record>> {
        Ok(self.conn.query_row(&format!("SELECT {COLS} FROM records WHERE id=?"), [id], row_to_record).optional()?)
    }

    /// Tombstone (kept for sync; `restore` is the undo).
    pub fn delete(&self, id: &str) -> Result<()> {
        self.set_deleted(id, true)
    }
    pub fn restore(&self, id: &str) -> Result<()> {
        self.set_deleted(id, false)
    }
    fn set_deleted(&self, id: &str, deleted: bool) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE records SET deleted=?2, dirty=1, updated_at=?3, tz_offset=?4, device_id=?5 WHERE id=?1",
            params![id, deleted as i64, time::now_ms(), time::local_offset_min(), self.device_id],
        )?;
        if n == 0 {
            return Err(Error::NotFound(id.into()));
        }
        Ok(())
    }

    /// Live records of a collection, newest-created first (UUIDv7 order).
    pub fn list(&self, collection: &str, limit: u32, offset: u32) -> Result<Vec<Record>> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {COLS} FROM records WHERE collection=? AND deleted=0 ORDER BY id DESC LIMIT ? OFFSET ?"
        ))?;
        Ok(st.query_map(params![collection, limit, offset], row_to_record)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Records whose updated_at falls in [from, to) (UTC ms), including tombstones if asked.
    pub fn list_range(&self, collection: Option<&str>, from: i64, to: i64, include_deleted: bool) -> Result<Vec<Record>> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {COLS} FROM records WHERE (?1 IS NULL OR collection=?1) AND updated_at>=?2 AND updated_at<?3
             AND (?4 OR deleted=0) ORDER BY updated_at"
        ))?;
        Ok(st.query_map(params![collection, from, to, include_deleted], row_to_record)?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn count(&self, collection: &str) -> Result<i64> {
        Ok(self.conn.query_row("SELECT count(*) FROM records WHERE collection=? AND deleted=0", [collection], |r| r.get(0))?)
    }

    /// Full-text search (FTS5 query syntax). Matches JSON text of live records.
    pub fn search(&self, collection: Option<&str>, query: &str, limit: u32) -> Result<Vec<Record>> {
        let mut st = self.conn.prepare_cached(
            "SELECT r.id, r.collection, r.data, r.updated_at, r.tz_offset, r.device_id, r.deleted, r.hist
             FROM records_fts f JOIN records r ON r.rowid=f.rowid
             WHERE records_fts MATCH ?1 AND r.deleted=0 AND (?2 IS NULL OR r.collection=?2)
             ORDER BY rank LIMIT ?3",
        )?;
        Ok(st.query_map(params![query, collection, limit], row_to_record)?.collect::<rusqlite::Result<_>>()?)
    }

    // ---- key/value ------------------------------------------------------

    pub fn kv_get(&self, k: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT v FROM kv WHERE k=?", [k], |r| r.get(0)).optional()?)
    }
    pub fn kv_set(&self, k: &str, v: &str) -> Result<()> {
        self.conn.execute("INSERT INTO kv(k, v) VALUES (?, ?) ON CONFLICT(k) DO UPDATE SET v=excluded.v", [k, v])?;
        Ok(())
    }

    // ---- sync support ---------------------------------------------------

    /// Unsynced records (excluding ones with an open conflict), with their base rev.
    pub fn dirty(&self, limit: u32) -> Result<Vec<(Record, i64)>> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {COLS}, base_rev FROM records WHERE dirty=1 AND id NOT IN (SELECT id FROM conflicts)
             ORDER BY updated_at LIMIT ?"
        ))?;
        Ok(st
            .query_map([limit], |r| Ok((row_to_record(r)?, r.get::<_, i64>(NCOLS)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn dirty_count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT count(*) FROM records WHERE dirty=1", [], |r| r.get(0))?)
    }

    /// After a successful push. Stays dirty if edited again while pushing.
    pub fn mark_synced(&self, id: &str, pushed_updated_at: i64, rev: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE records SET base_rev=?2, dirty=CASE WHEN updated_at=?3 THEN 0 ELSE 1 END WHERE id=?1",
            params![id, rev, pushed_updated_at],
        )?;
        Ok(())
    }

    fn local_state(&self, id: &str) -> Result<Option<(Record, bool, i64)>> {
        Ok(self
            .conn
            .query_row(&format!("SELECT {COLS}, dirty, base_rev FROM records WHERE id=?"), [id], |r| {
                Ok((row_to_record(r)?, r.get::<_, i64>(NCOLS)? != 0, r.get(NCOLS + 1)?))
            })
            .optional()?)
    }

    /// Write a record from elsewhere, keeping its version history.
    fn write_foreign(&self, r: &Record, dirty: bool, base_rev: i64) -> Result<()> {
        let hist = if r.hist.is_empty() { "[]".to_string() } else { serde_json::to_string(&r.hist)? };
        self.conn.execute(
            "INSERT INTO records (id, collection, data, updated_at, tz_offset, device_id, deleted, dirty, base_rev, hist)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET collection=excluded.collection, data=excluded.data,
               updated_at=excluded.updated_at, tz_offset=excluded.tz_offset, device_id=excluded.device_id,
               deleted=excluded.deleted, dirty=excluded.dirty, base_rev=excluded.base_rev, hist=excluded.hist",
            params![r.id, r.collection, r.data.to_string(), r.updated_at, r.tz_offset, r.device_id, r.deleted as i64,
                dirty as i64, base_rev, hist],
        )?;
        Ok(())
    }

    /// Same content arrived under another version id: remember it as ours too
    /// (hist-only update, no lseq bump) so later edits on either side fast-forward.
    fn merge_hist(&self, local: &Record, other: &Record) -> Result<()> {
        let mut hist = serde_json::to_string(&local.hist)?;
        for v in other.hist.iter().chain(std::iter::once(&other.ver())) {
            if !local.hist.contains(v) {
                hist = hist_push(&hist, v);
            }
        }
        // Keep our own current version last.
        let hist = hist_push(&hist, &local.ver());
        self.conn.execute("UPDATE records SET hist=?2 WHERE id=?1", params![local.id, hist])?;
        Ok(())
    }

    fn record_conflict(&self, remote: &Record, remote_rev: i64, source: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO conflicts (id, collection, remote, remote_rev, created_at, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET remote=excluded.remote, remote_rev=excluded.remote_rev, source=excluded.source",
            params![remote.id, remote.collection, serde_json::to_string(remote)?, remote_rev, time::now_ms(), source],
        )?;
        Ok(())
    }

    /// An open conflict whose other version is now part of an incoming record's
    /// history was settled elsewhere (e.g. the other device resolved it): drop it.
    fn clear_superseded_conflict(&self, inc: &Record) -> Result<()> {
        let remote: Option<String> =
            self.conn.query_row("SELECT remote FROM conflicts WHERE id=?", [&inc.id], |r| r.get(0)).optional()?;
        if let Some(j) = remote
            && let Ok(r) = serde_json::from_str::<Record>(&j)
            && (inc.ver() == r.ver() || inc.hist.contains(&r.ver()))
        {
            self.conn.execute("DELETE FROM conflicts WHERE id=?", [&inc.id])?;
        }
        Ok(())
    }

    /// Apply a record pulled from the server (rev = its server revision).
    pub fn apply_remote(&self, remote: &Record, rev: i64) -> Result<Applied> {
        match self.local_state(&remote.id)? {
            None => {
                self.write_foreign(remote, false, rev)?;
                Ok(Applied::Inserted)
            }
            Some((_, _, base)) if base >= rev => Ok(Applied::Unchanged),
            Some((l, dirty, _)) if l.same_content(remote) || (dirty && l.hist.contains(&remote.ver())) => {
                // Same content (restored/LAN copy) or we're already ahead of it:
                // adopt the rev; stay dirty only if our version is newer.
                let still_dirty = dirty && !l.same_content(remote);
                if l.ver() != remote.ver() {
                    self.merge_hist(&l, remote)?;
                }
                self.conn.execute(
                    "UPDATE records SET base_rev=?2, dirty=?3 WHERE id=?1",
                    params![l.id, rev, still_dirty as i64],
                )?;
                Ok(Applied::Unchanged)
            }
            Some((l, dirty, _)) if !dirty || remote.hist.contains(&l.ver()) => {
                // Clean local, or remote descends from our version: fast-forward.
                self.write_foreign(remote, false, rev)?;
                self.clear_superseded_conflict(remote)?;
                Ok(Applied::Updated)
            }
            Some(_) => {
                self.record_conflict(remote, rev, "server")?;
                Ok(Applied::Conflict)
            }
        }
    }

    /// Apply a record received from a LAN peer (no server revision involved).
    /// Uses version histories: fast-forward, ignore if we're ahead, else conflict.
    pub fn apply_peer(&self, inc: &Record) -> Result<Applied> {
        match self.local_state(&inc.id)? {
            None => {
                self.write_foreign(inc, true, 0)?;
                Ok(Applied::Inserted)
            }
            Some((l, _, _)) if l.ver() == inc.ver() || l.hist.contains(&inc.ver()) => Ok(Applied::Unchanged),
            Some((l, _, _)) if l.same_content(inc) => {
                self.merge_hist(&l, inc)?;
                Ok(Applied::Unchanged)
            }
            Some((l, _, base)) if inc.hist.contains(&l.ver()) => {
                self.write_foreign(inc, true, base)?;
                self.clear_superseded_conflict(inc)?;
                Ok(Applied::Updated)
            }
            Some((_, _, base)) => {
                self.record_conflict(inc, base, "peer")?;
                Ok(Applied::Conflict)
            }
        }
    }

    /// Records changed locally (any source) with lseq > since, ascending.
    pub fn changes_since(&self, since: i64, limit: u32) -> Result<Vec<(Record, i64)>> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {COLS}, lseq FROM records WHERE lseq > ? ORDER BY lseq LIMIT ?"
        ))?;
        Ok(st
            .query_map(params![since, limit], |r| Ok((row_to_record(r)?, r.get::<_, i64>(NCOLS)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn conflicts(&self) -> Result<Vec<Conflict>> {
        let mut st = self.conn.prepare("SELECT id, remote, remote_rev, source FROM conflicts ORDER BY created_at")?;
        let rows: Vec<(String, String, i64, String)> =
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        rows.into_iter()
            .map(|(id, remote, remote_rev, source)| {
                let local = self.get(&id)?.ok_or_else(|| Error::NotFound(id.clone()))?;
                Ok(Conflict { local, remote: serde_json::from_str(&remote)?, remote_rev, source })
            })
            .collect()
    }

    pub fn resolve_conflict(&self, id: &str, how: Resolution) -> Result<()> {
        let (remote_json, remote_rev, source): (String, i64, String) = self.conn.query_row(
            "SELECT remote, remote_rev, source FROM conflicts WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let remote: Record = serde_json::from_str(&remote_json)?;
        let tx = self.conn.unchecked_transaction()?;
        // Whatever wins now supersedes the other side's version too, so peers
        // holding it fast-forward instead of conflicting again.
        tx.execute("UPDATE records SET hist = sk_hist_push(hist, ?2) WHERE id=?1", params![id, remote.ver()])?;
        match how {
            Resolution::KeepRemote => {
                // A peer's version isn't on the server yet, so it must be pushed.
                let dirty = source == "peer";
                let mut hist: Vec<String> = remote.hist.clone();
                let local_hist: String = tx.query_row("SELECT hist FROM records WHERE id=?", [id], |r| r.get(0))?;
                for v in serde_json::from_str::<Vec<String>>(&local_hist).unwrap_or_default() {
                    if !hist.contains(&v) {
                        hist.insert(0, v);
                    }
                }
                let hist = hist_push(&serde_json::to_string(&hist)?, &remote.ver());
                tx.execute(
                    "UPDATE records SET collection=?2, data=?3, updated_at=?4, tz_offset=?5, device_id=?6, deleted=?7,
                     dirty=?8, base_rev=?9, hist=?10 WHERE id=?1",
                    params![id, remote.collection, remote.data.to_string(), remote.updated_at, remote.tz_offset,
                        remote.device_id, remote.deleted as i64, dirty as i64, remote_rev, hist],
                )?;
            }
            Resolution::KeepLocal => {
                tx.execute(
                    "UPDATE records SET base_rev=?2, dirty=1, updated_at=?3, device_id=?4 WHERE id=?1",
                    params![id, remote_rev, time::now_ms(), self.device_id],
                )?;
            }
            Resolution::Merged(v) => {
                tx.execute(
                    "UPDATE records SET data=?2, base_rev=?3, dirty=1, deleted=0, updated_at=?4, tz_offset=?5, device_id=?6 WHERE id=?1",
                    params![id, v.to_string(), remote_rev, time::now_ms(), time::local_offset_min(), self.device_id],
                )?;
            }
        }
        tx.execute("DELETE FROM conflicts WHERE id=?", [id])?;
        tx.commit()?;
        Ok(())
    }

    /// Bundle import: insert if missing or older locally ("newer wins"); marked dirty
    /// so it gets pushed. Returns true if written.
    pub fn import_record(&self, r: &Record) -> Result<bool> {
        let local_ts: Option<i64> =
            self.conn.query_row("SELECT updated_at FROM records WHERE id=?", [&r.id], |x| x.get(0)).optional()?;
        if matches!(local_ts, Some(t) if t >= r.updated_at) {
            return Ok(false);
        }
        self.write_local(r)?;
        Ok(true)
    }

    /// Server migration ("Upload everything"): forget server state, re-push all.
    pub fn reset_sync_state(&self) -> Result<()> {
        self.conn.execute_batch("UPDATE records SET dirty=1, base_rev=0; DELETE FROM conflicts;")?;
        self.conn.execute("DELETE FROM kv WHERE k LIKE 'sync.%'", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> Store {
        Store::open_in_memory("agenda", "dev-a", &[]).unwrap()
    }

    #[test]
    fn crud_and_tombstones() {
        let s = store();
        let r = s.put("tasks", None, &json!({"title": "pay rent"})).unwrap();
        assert_eq!(s.get(&r.id).unwrap().unwrap().data["title"], "pay rent");
        assert_eq!(s.count("tasks").unwrap(), 1);
        s.delete(&r.id).unwrap();
        assert_eq!(s.count("tasks").unwrap(), 0);
        assert!(s.get(&r.id).unwrap().unwrap().deleted);
        s.restore(&r.id).unwrap();
        assert_eq!(s.list("tasks", 10, 0).unwrap().len(), 1);
    }

    #[test]
    fn fts_search() {
        let s = store();
        s.put("tasks", None, &json!({"title": "buy groceries"})).unwrap();
        s.put("tasks", None, &json!({"title": "call mom"})).unwrap();
        let hits = s.search(Some("tasks"), "groceries", 10).unwrap();
        assert_eq!(hits.len(), 1);
        let id = hits[0].id.clone();
        s.put("tasks", Some(&id), &json!({"title": "buy milk"})).unwrap();
        assert!(s.search(None, "groceries", 10).unwrap().is_empty());
        assert_eq!(s.search(None, "milk", 10).unwrap().len(), 1);
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.db");
        const M: &[Migration] = &[Migration { name: "x/001", sql: "CREATE INDEX x_idx ON records(device_id);" }];
        Store::open(&p, "x", "d", M).unwrap();
        let s = Store::open(&p, "x", "d", M).unwrap();
        let n: i64 = s.conn().query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 3); // core/001, core/002, x/001
    }

    /// A remote edit made by another device on top of `base`.
    fn edit(base: &Record, dev: &str, data: Value, dt: i64) -> Record {
        let mut r = base.clone();
        r.data = data;
        r.device_id = dev.into();
        r.updated_at = base.updated_at + dt;
        r.hist.push(r.ver());
        r
    }

    #[test]
    fn remote_apply_and_conflicts() {
        let s = store();
        let r = s.put("tasks", None, &json!({"title": "a"})).unwrap();
        assert_eq!(r.hist, vec![r.ver()]);
        let (d, base) = s.dirty(10).unwrap().pop().unwrap();
        assert_eq!(base, 0);
        s.mark_synced(&d.id, d.updated_at, 5).unwrap();
        assert!(s.dirty(10).unwrap().is_empty());

        // clean local + newer remote -> updated
        let remote = edit(&r, "dev-b", json!({"title": "b"}), 10);
        assert_eq!(s.apply_remote(&remote, 6).unwrap(), Applied::Updated);
        assert_eq!(s.get(&r.id).unwrap().unwrap().data["title"], "b");
        assert_eq!(s.apply_remote(&remote, 6).unwrap(), Applied::Unchanged);

        // dirty local + concurrent remote edit -> conflict, not pushed
        s.put("tasks", Some(&r.id), &json!({"title": "local"})).unwrap();
        let remote2 = edit(&remote, "dev-b", json!({"title": "remote"}), 10);
        assert_eq!(s.apply_remote(&remote2, 7).unwrap(), Applied::Conflict);
        assert!(s.dirty(10).unwrap().is_empty());
        let c = s.conflicts().unwrap();
        assert_eq!((c[0].local.data["title"].as_str(), c[0].remote.data["title"].as_str()), (Some("local"), Some("remote")));
        s.resolve_conflict(&r.id, Resolution::Merged(json!({"title": "both"}))).unwrap();
        assert!(s.conflicts().unwrap().is_empty());
        let (d, base) = s.dirty(10).unwrap().pop().unwrap();
        assert_eq!((d.data["title"].as_str(), base), (Some("both"), 7));
        assert!(d.hist.contains(&remote2.ver()), "merge supersedes the remote version");
    }

    #[test]
    fn lseq_bumps_on_content_changes_only() {
        let s = store();
        let a = s.put("tasks", None, &json!({"t": 1})).unwrap();
        let b = s.put("tasks", None, &json!({"t": 2})).unwrap();
        let ch = s.changes_since(0, 10).unwrap();
        assert_eq!(ch.iter().map(|(r, _)| r.id.clone()).collect::<Vec<_>>(), [a.id.clone(), b.id.clone()]);
        let last = ch[1].1;
        s.mark_synced(&a.id, a.updated_at, 3).unwrap(); // bookkeeping only
        assert!(s.changes_since(last, 10).unwrap().is_empty());
        s.put("tasks", Some(&a.id), &json!({"t": 3})).unwrap();
        assert_eq!(s.changes_since(last, 10).unwrap()[0].0.id, a.id);
    }

    #[test]
    fn peer_fast_forward_ahead_and_conflict() {
        let a = Store::open_in_memory("agenda", "A", &[]).unwrap();
        let b = Store::open_in_memory("agenda", "B", &[]).unwrap();
        let r0 = a.put("tasks", None, &json!({"t": "v0"})).unwrap();
        assert_eq!(b.apply_peer(&r0).unwrap(), Applied::Inserted);
        // A edits twice; B fast-forwards (history covers both edits).
        std::thread::sleep(std::time::Duration::from_millis(2));
        a.put("tasks", Some(&r0.id), &json!({"t": "v1"})).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let r2 = a.put("tasks", Some(&r0.id), &json!({"t": "v2"})).unwrap();
        assert_eq!(b.apply_peer(&r2).unwrap(), Applied::Updated);
        // Sending B's copy back to A: A already has it.
        let back = b.get(&r0.id).unwrap().unwrap();
        assert_eq!(a.apply_peer(&back).unwrap(), Applied::Unchanged);
        // B edits on top -> A fast-forwards.
        std::thread::sleep(std::time::Duration::from_millis(2));
        let rb = b.put("tasks", Some(&r0.id), &json!({"t": "B"})).unwrap();
        assert_eq!(a.apply_peer(&rb).unwrap(), Applied::Updated);
        // Old version arriving late is ignored.
        assert_eq!(a.apply_peer(&r2).unwrap(), Applied::Unchanged);
        // Concurrent edits -> conflict on both sides.
        std::thread::sleep(std::time::Duration::from_millis(2));
        let xa = a.put("tasks", Some(&r0.id), &json!({"t": "xa"})).unwrap();
        let xb = b.put("tasks", Some(&r0.id), &json!({"t": "xb"})).unwrap();
        assert_eq!(a.apply_peer(&xb).unwrap(), Applied::Conflict);
        assert_eq!(b.apply_peer(&xa).unwrap(), Applied::Conflict);
        assert_eq!(a.conflicts().unwrap()[0].source, "peer");
        // A keeps B's; B keeps its own -> both converge without a new conflict.
        a.resolve_conflict(&r0.id, Resolution::KeepRemote).unwrap();
        b.resolve_conflict(&r0.id, Resolution::KeepLocal).unwrap();
        let bb = b.get(&r0.id).unwrap().unwrap();
        assert_eq!(a.apply_peer(&bb).unwrap(), Applied::Unchanged); // same content, hist merged
        assert_eq!(a.get(&r0.id).unwrap().unwrap().data, json!({"t": "xb"}));
        // Next edits on either side fast-forward, no false conflict.
        std::thread::sleep(std::time::Duration::from_millis(2));
        let na = a.put("tasks", Some(&r0.id), &json!({"t": "after"})).unwrap();
        assert_eq!(b.apply_peer(&na).unwrap(), Applied::Updated);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let nb = b.put("tasks", Some(&r0.id), &json!({"t": "after2"})).unwrap();
        assert_eq!(a.apply_peer(&nb).unwrap(), Applied::Updated);
        assert!(a.dirty(10).unwrap().iter().any(|(r, _)| r.id == r0.id), "peer data must still go to the server");
    }

}
