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
}, Migration {
    // Per-field change clocks: {"field": UTC ms of its last local change}. Lets edits
    // to different fields of one record on two devices merge without a conflict.
    name: "core/003_field_clocks",
    sql: "ALTER TABLE records ADD COLUMN fclock TEXT NOT NULL DEFAULT '{}';",
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

/// Collection of the device directory (one record per device: name, platform).
pub const DEVICES: &str = "devices";

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
    /// When each top-level data field last changed (UTC ms). Filled by the store.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub fclock: std::collections::BTreeMap<String, i64>,
}

impl Record {
    /// Version id of this edit: `"updated_at:device_id"`.
    pub fn ver(&self) -> String {
        format!("{}:{}", self.updated_at, self.device_id)
    }
    /// Counts as a user change in sync reports ("12 changes pulled"). The device
    /// directory (`devices`, shared settings DB) syncs like any record but is
    /// bookkeeping: counting it made every pass report one change too many.
    pub fn is_user_data(&self) -> bool {
        self.collection != DEVICES
    }
    /// Same user-visible content (ignores who/when).
    pub fn same_content(&self, o: &Record) -> bool {
        crate::conflict::json_eq(&self.data, &o.data) && self.deleted == o.deleted && self.collection == o.collection
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

const COLS: &str = "id, collection, data, updated_at, tz_offset, device_id, deleted, hist, fclock";
/// Index of the first column after COLS.
const NCOLS: usize = 9;

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
        fclock: serde_json::from_str(&r.get::<_, String>(8)?).unwrap_or_default(),
    })
}

/// New field clocks after a local edit: fields whose value changed get `now`. A field
/// without a clock that did not change (record written before field clocks, or by an
/// older build) gets the old record's `updated_at`, not `now`, so a field merge doesn't
/// count it as changed on this side (C1).
fn stamp_clocks(old: Option<&Record>, data: &Value, now: i64) -> std::collections::BTreeMap<String, i64> {
    let mut clock = old.map(|o| o.fclock.clone()).unwrap_or_default();
    let empty = serde_json::Map::new();
    let old_obj = old.and_then(|o| o.data.as_object()).unwrap_or(&empty);
    let new_obj = data.as_object().unwrap_or(&empty);
    for k in old_obj.keys().chain(new_obj.keys()) {
        if !crate::conflict::opt_eq(old_obj.get(k), new_obj.get(k)) {
            clock.insert(k.clone(), now);
        } else if !clock.contains_key(k) {
            clock.insert(k.clone(), old.map_or(now, |o| o.updated_at.min(now)));
        }
    }
    clock
}

/// Cross-field rules of the apps' own validation that an automatic field merge must
/// keep (C20): Agenda `tasks` stop not before start (and no stop without a start), Dusk
/// `sleep_log` wake after bed and at most 20 h later, Hisab `loans` repayments not more
/// than the amount. Keep in sync with tasks.rs / dusk data.rs / hisab model.rs.
fn merge_is_valid(collection: &str, v: &Value) -> bool {
    match collection {
        "tasks" => {
            let when = |k: &str| v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(|s| if s.len() == 10 { format!("{s}T00:00") } else { s.to_string() });
            match (when("start"), when("stop")) {
                (None, Some(_)) => false,
                (Some(a), Some(b)) => b >= a,
                _ => true,
            }
        }
        "sleep_log" => match (v.get("bed").and_then(Value::as_i64), v.get("wake").and_then(Value::as_i64)) {
            (Some(b), Some(w)) => w > b && w - b <= 20 * 3_600_000,
            _ => true,
        },
        "loans" => {
            let paid: f64 = v.get("repayments").and_then(Value::as_array).map(|a| a.iter().filter_map(|r| r.get("amount").and_then(Value::as_f64)).sum()).unwrap_or(0.0);
            v.get("amount").and_then(Value::as_f64).is_none_or(|amount| paid <= amount + 0.005)
        }
        _ => true,
    }
}

/// Merge two versions of a list field changed on both devices: items (objects with an
/// `id`, or plain values) from both sides; the same item changed on both → the newer
/// side's copy. Known limit: an item removed on one device while the other device also
/// changed that list comes back.
fn union_lists(newer: &[Value], older: &[Value]) -> Value {
    let key = |v: &Value| v.get("id").map(|i| i.to_string()).unwrap_or_else(|| v.to_string());
    let mut seen: std::collections::HashSet<String> = newer.iter().map(key).collect();
    let mut out: Vec<Value> = newer.to_vec();
    for v in older {
        if seen.insert(key(v)) {
            out.push(v.clone());
        }
    }
    Value::Array(out)
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
        // A DB not yet opened by a build with field clocks (core/003) has no `fclock`
        // column; a temp view with an empty one keeps every query working (C8).
        let (cols, has_fclock): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(name='fclock'), 0) FROM pragma_table_info('records')",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if cols > 0 && has_fclock == 0 {
            conn.execute_batch("CREATE TEMP VIEW records AS SELECT rowid AS rowid, *, '{}' AS fclock FROM main.records;")?;
        }
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
        let id = id.map(str::to_owned).unwrap_or_else(new_id);
        let now = time::now_ms();
        let old = self.get(&id)?;
        let rec = Record {
            fclock: stamp_clocks(old.as_ref(), data, now),
            id,
            collection: collection.into(),
            data: data.clone(),
            updated_at: now,
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
            "INSERT INTO records (id, collection, data, updated_at, tz_offset, device_id, deleted, dirty, fclock)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)
             ON CONFLICT(id) DO UPDATE SET collection=excluded.collection, data=excluded.data,
               updated_at=excluded.updated_at, tz_offset=excluded.tz_offset, device_id=excluded.device_id,
               deleted=excluded.deleted, dirty=1, fclock=excluded.fclock",
            params![r.id, r.collection, r.data.to_string(), r.updated_at, r.tz_offset, r.device_id, r.deleted as i64,
                serde_json::to_string(&r.fclock)?],
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
            "SELECT r.id, r.collection, r.data, r.updated_at, r.tz_offset, r.device_id, r.deleted, r.hist, r.fclock
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
            "INSERT INTO records (id, collection, data, updated_at, tz_offset, device_id, deleted, dirty, base_rev, hist, fclock)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET collection=excluded.collection, data=excluded.data,
               updated_at=excluded.updated_at, tz_offset=excluded.tz_offset, device_id=excluded.device_id,
               deleted=excluded.deleted, dirty=excluded.dirty, base_rev=excluded.base_rev, hist=excluded.hist,
               fclock=excluded.fclock",
            params![r.id, r.collection, r.data.to_string(), r.updated_at, r.tz_offset, r.device_id, r.deleted as i64,
                dirty as i64, base_rev, hist, serde_json::to_string(&r.fclock)?],
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

    /// Automatic merge of two concurrent versions, field by field, using the per-field
    /// clocks and the last version both share (from `hist`): a field changed on one side
    /// only takes that side; a field changed on both takes the newer change, lists are
    /// combined (`union_lists`). `None` = leave it to the user: a deletion, long text
    /// changed on both (line merge on the conflict screen), a side without clocks
    /// (written by an older build), or an exact tie.
    pub fn field_merge(local: &Record, remote: &Record) -> Option<Value> {
        if local.deleted || remote.deleted || local.collection != remote.collection {
            return None;
        }
        // Records from builds without field clocks: only the shared settings merge
        // anyway (each whole record counts as changed at its own time), so the first
        // sync of a new device never shows a settings conflict.
        let settings = local.collection == crate::settings::COLLECTION;
        // One side without clocks (older build, or an edit made before the upgrade) would
        // count all its fields as unchanged and lose them silently: ask instead (C2).
        if (local.fclock.is_empty() || remote.fclock.is_empty()) && !settings {
            return None;
        }
        let (l, r) = (local.data.as_object()?, remote.data.as_object()?);
        let legacy = |rec: &Record, k: &str| rec.fclock.get(k).copied().or(if settings && rec.fclock.is_empty() { Some(rec.updated_at) } else { None });
        let ts = |v: &String| v.split(':').next().and_then(|t| t.parse::<i64>().ok()).unwrap_or(0);
        let base = local.hist.iter().filter(|v| remote.hist.contains(v)).map(ts).max().unwrap_or(0);
        let empty = |v: Option<&Value>| match v {
            None | Some(Value::Null) => true,
            Some(Value::String(s)) => s.is_empty(),
            Some(Value::Array(a)) => a.is_empty(),
            _ => false,
        };
        let long = |v: Option<&Value>| matches!(v, Some(Value::String(s)) if s.contains('\n') || s.chars().count() > 80);
        let mut out = serde_json::Map::new();
        let keys: std::collections::BTreeSet<&String> = l.keys().chain(r.keys()).collect();
        for k in keys {
            let (lv, rv) = (l.get(k), r.get(k));
            let pick = |v: Option<&Value>, out: &mut serde_json::Map<String, Value>| {
                if let Some(v) = v {
                    out.insert(k.clone(), v.clone());
                }
            };
            if crate::conflict::opt_eq(lv, rv) {
                pick(lv, &mut out);
                continue;
            }
            let (tl, tr) = (legacy(local, k).unwrap_or(0), legacy(remote, k).unwrap_or(0));
            let (cl, cr) = (tl > base, tr > base);
            if cl && !cr {
                pick(lv, &mut out);
            } else if cr && !cl {
                pick(rv, &mut out);
            } else if empty(lv) != empty(rv) {
                // Filled on one side, empty on the other (typical when the same record
                // was created on two devices): keep what was filled in.
                pick(if empty(lv) { rv } else { lv }, &mut out);
            } else if let (Some(Value::Array(a)), Some(Value::Array(b))) = (lv, rv) {
                let v = if tl >= tr { union_lists(a, b) } else { union_lists(b, a) };
                out.insert(k.clone(), v);
            } else if long(lv) && long(rv) {
                return None;
            } else if tl > tr {
                pick(lv, &mut out);
            } else if tr > tl {
                pick(rv, &mut out);
            } else {
                return None;
            }
        }
        // Fields merged one by one can break a rule that spans several fields; such a
        // result goes to the conflict screen instead (C20).
        let v = Value::Object(out);
        merge_is_valid(&local.collection, &v).then_some(v)
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
            Some((l, _, _)) => {
                self.record_conflict(remote, rev, "server")?;
                if let Some(v) = Self::field_merge(&l, remote) {
                    self.resolve_conflict(&remote.id, Resolution::Merged(v))?;
                    return Ok(Applied::Updated);
                }
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
            Some((l, _, base)) => {
                self.record_conflict(inc, base, "peer")?;
                if let Some(v) = Self::field_merge(&l, inc) {
                    self.resolve_conflict(&inc.id, Resolution::Merged(v))?;
                    return Ok(Applied::Updated);
                }
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
        let local = self.get(id)?.ok_or_else(|| Error::NotFound(id.into()))?;
        // Field clocks of the result: the winning side's, edits made here now are "now".
        let now = time::now_ms();
        let fclock = {
            let differs = |a: &Value, b: &Value, k: &String| !crate::conflict::opt_eq(a.get(k), b.get(k));
            let keys: Vec<String> = local.fclock.keys().chain(remote.fclock.keys()).cloned().collect();
            match &how {
                Resolution::KeepRemote => remote.fclock.clone(),
                Resolution::KeepLocal => {
                    let mut c = local.fclock.clone();
                    let obj_keys: Vec<String> = local.data.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
                    for k in keys.iter().chain(obj_keys.iter()) {
                        if differs(&local.data, &remote.data, k) {
                            c.insert(k.clone(), now);
                        }
                    }
                    c
                }
                Resolution::Merged(v) => {
                    let mut c = std::collections::BTreeMap::new();
                    for k in &keys {
                        let t = local.fclock.get(k).copied().unwrap_or(0).max(remote.fclock.get(k).copied().unwrap_or(0));
                        c.insert(k.clone(), t);
                    }
                    for k in v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() {
                        if differs(v, &local.data, &k) && differs(v, &remote.data, &k) {
                            c.insert(k, now);
                        }
                    }
                    c
                }
            }
        };
        // Also called from apply_remote/apply_peer inside a sync transaction.
        let tx = if self.conn.is_autocommit() { Some(self.conn.unchecked_transaction()?) } else { None };
        let db = &self.conn;
        // Whatever wins now supersedes the other side's version too, so peers
        // holding it fast-forward instead of conflicting again.
        db.execute("UPDATE records SET hist = sk_hist_push(hist, ?2) WHERE id=?1", params![id, remote.ver()])?;
        match how {
            Resolution::KeepRemote => {
                // A peer's version isn't on the server yet, so it must be pushed.
                let dirty = source == "peer";
                let mut hist: Vec<String> = remote.hist.clone();
                let local_hist: String = db.query_row("SELECT hist FROM records WHERE id=?", [id], |r| r.get(0))?;
                for v in serde_json::from_str::<Vec<String>>(&local_hist).unwrap_or_default() {
                    if !hist.contains(&v) {
                        hist.insert(0, v);
                    }
                }
                let hist = hist_push(&serde_json::to_string(&hist)?, &remote.ver());
                db.execute(
                    "UPDATE records SET collection=?2, data=?3, updated_at=?4, tz_offset=?5, device_id=?6, deleted=?7,
                     dirty=?8, base_rev=?9, hist=?10 WHERE id=?1",
                    params![id, remote.collection, remote.data.to_string(), remote.updated_at, remote.tz_offset,
                        remote.device_id, remote.deleted as i64, dirty as i64, remote_rev, hist],
                )?;
            }
            Resolution::KeepLocal => {
                db.execute(
                    "UPDATE records SET base_rev=?2, dirty=1, updated_at=?3, device_id=?4 WHERE id=?1",
                    params![id, remote_rev, time::now_ms(), self.device_id],
                )?;
            }
            Resolution::Merged(v) => {
                db.execute(
                    "UPDATE records SET data=?2, base_rev=?3, dirty=1, deleted=0, updated_at=?4, tz_offset=?5, device_id=?6 WHERE id=?1",
                    params![id, v.to_string(), remote_rev, time::now_ms(), time::local_offset_min(), self.device_id],
                )?;
            }
        }
        db.execute("UPDATE records SET fclock=?2 WHERE id=?1", params![id, serde_json::to_string(&fclock)?])?;
        db.execute("DELETE FROM conflicts WHERE id=?", [id])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// Bundle import: insert if missing or older locally ("newer wins"); marked dirty
    /// so it gets pushed. Returns true if written.
    pub fn import_record(&self, r: &Record) -> Result<bool> {
        let old = self.get(&r.id)?;
        if matches!(&old, Some(o) if o.updated_at >= r.updated_at) {
            return Ok(false);
        }
        // Fields that differ from this device's copy count as changed at the bundle's
        // time (newer than any shared base), so a later field merge keeps the restored
        // values instead of seeing them as unchanged (C3). A record new to this device
        // keeps the bundle's clocks when it has them.
        let mut rec = r.clone();
        if old.is_some() || rec.fclock.is_empty() {
            rec.fclock = stamp_clocks(old.as_ref(), &r.data, r.updated_at);
        }
        self.write_local(&rec)?;
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
        assert_eq!(n, 4); // core/001, core/002, core/003_field_clocks, x/001
    }

    /// A remote edit made by another device on top of `base`.
    fn edit(base: &Record, dev: &str, data: Value, dt: i64) -> Record {
        let mut r = base.clone();
        r.data = data;
        r.device_id = dev.into();
        r.updated_at = base.updated_at + dt;
        // Like Store::put on the other device: changed fields get its clock.
        if let Some(o) = r.data.as_object() {
            for (k, v) in o {
                if base.data.get(k) != Some(v) {
                    r.fclock.insert(k.clone(), r.updated_at);
                }
            }
        }
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

        // dirty local + concurrent remote edit of long text -> conflict, not pushed
        // (short fields changed on both sides merge automatically, newer wins)
        std::thread::sleep(std::time::Duration::from_millis(20)); // both edits newer than the shared version
        s.put("tasks", Some(&r.id), &json!({"title": "b\nlocal"})).unwrap();
        let remote2 = edit(&remote, "dev-b", json!({"title": "b\nremote"}), 10);
        assert_eq!(s.apply_remote(&remote2, 7).unwrap(), Applied::Conflict);
        assert!(s.dirty(10).unwrap().is_empty());
        let c = s.conflicts().unwrap();
        assert_eq!((c[0].local.data["title"].as_str(), c[0].remote.data["title"].as_str()), (Some("b\nlocal"), Some("b\nremote")));
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
        // Concurrent edits of long text -> conflict on both sides (short fields auto-merge).
        std::thread::sleep(std::time::Duration::from_millis(2));
        let xa = a.put("tasks", Some(&r0.id), &json!({"t": "x\nxa"})).unwrap();
        let xb = b.put("tasks", Some(&r0.id), &json!({"t": "x\nxb"})).unwrap();
        assert_eq!(a.apply_peer(&xb).unwrap(), Applied::Conflict);
        assert_eq!(b.apply_peer(&xa).unwrap(), Applied::Conflict);
        assert_eq!(a.conflicts().unwrap()[0].source, "peer");
        // A keeps B's; B keeps its own -> both converge without a new conflict.
        a.resolve_conflict(&r0.id, Resolution::KeepRemote).unwrap();
        b.resolve_conflict(&r0.id, Resolution::KeepLocal).unwrap();
        let bb = b.get(&r0.id).unwrap().unwrap();
        assert_eq!(a.apply_peer(&bb).unwrap(), Applied::Unchanged); // same content, hist merged
        assert_eq!(a.get(&r0.id).unwrap().unwrap().data, json!({"t": "x\nxb"}));
        // Next edits on either side fast-forward, no false conflict.
        std::thread::sleep(std::time::Duration::from_millis(2));
        let na = a.put("tasks", Some(&r0.id), &json!({"t": "after"})).unwrap();
        assert_eq!(b.apply_peer(&na).unwrap(), Applied::Updated);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let nb = b.put("tasks", Some(&r0.id), &json!({"t": "after2"})).unwrap();
        assert_eq!(a.apply_peer(&nb).unwrap(), Applied::Updated);
        assert!(a.dirty(10).unwrap().iter().any(|(r, _)| r.id == r0.id), "peer data must still go to the server");
    }

    fn plain(data: Value, at: i64, clocks: &[(&str, i64)], hist: &[&str]) -> Record {
        Record {
            id: "r".into(),
            collection: "tasks".into(),
            data,
            updated_at: at,
            tz_offset: 0,
            device_id: "X".into(),
            deleted: false,
            hist: hist.iter().map(|h| h.to_string()).collect(),
            fclock: clocks.iter().map(|(k, t)| (k.to_string(), *t)).collect(),
        }
    }

    #[test]
    fn clockless_record_first_edit_stamps_only_changed_fields() {
        // C1: a record from before field clocks; editing the title must not mark `due` as changed.
        let old = plain(json!({"title": "a", "due": "x"}), 100, &[], &["100:A"]);
        let c = stamp_clocks(Some(&old), &json!({"title": "b", "due": "x"}), 200);
        assert_eq!((c["title"], c["due"]), (200, 100));
        // Both devices edit a different field first after upgrading -> both edits survive.
        let mut local = plain(json!({"title": "phone", "due": "x"}), 200, &[], &["100:A", "200:A"]);
        local.fclock = stamp_clocks(Some(&old), &local.data, 200);
        let mut remote = plain(json!({"title": "a", "due": "y"}), 300, &[], &["100:A", "300:B"]);
        remote.fclock = stamp_clocks(Some(&old), &remote.data, 300);
        assert_eq!(Store::field_merge(&local, &remote), Some(json!({"title": "phone", "due": "y"})));
    }

    #[test]
    fn one_side_without_clocks_is_a_conflict() {
        // C2: an edit from an older build (no clocks) must not lose to the clocked side.
        let local = plain(json!({"title": "old build edit", "due": "x"}), 200, &[], &["100:A", "200:A"]);
        let remote = plain(json!({"title": "a", "due": "y"}), 300, &[("title", 100), ("due", 300)], &["100:A", "300:B"]);
        assert_eq!(Store::field_merge(&local, &remote), None);
        assert_eq!(Store::field_merge(&remote, &local), None);
    }

    #[test]
    fn imported_fields_count_as_changed() {
        // C3: restoring an older bundle's values stamps the differing fields with the bundle time.
        let s = store();
        let r = s.put("tasks", None, &json!({"title": "now", "due": "x"})).unwrap();
        let mut b = r.clone();
        b.data = json!({"title": "restored", "due": "x"});
        b.updated_at = r.updated_at + 1000;
        b.fclock.clear();
        assert!(s.import_record(&b).unwrap());
        let got = s.get(&r.id).unwrap().unwrap();
        assert_eq!(got.fclock["title"], b.updated_at);
        assert_eq!(got.fclock["due"], r.fclock["due"]);
    }
    #[test]
    fn read_only_without_field_clocks() {
        // C8: another app's DB from before core/003 (no `fclock` column) still reads.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("old.db");
        let id = {
            let s = Store::open(&p, "chrono", "d", &[]).unwrap();
            let r = s.put("settings", None, &json!({"exclude": ["x"]})).unwrap();
            s.conn().execute_batch("ALTER TABLE records DROP COLUMN fclock;").unwrap();
            r.id
        };
        let ro = Store::open_read_only(&p, "chrono").unwrap();
        let r = ro.get(&id).unwrap().unwrap();
        assert_eq!((r.data["exclude"][0].as_str(), r.fclock.is_empty()), (Some("x"), true));
        assert_eq!(ro.list("settings", 10, 0).unwrap().len(), 1);
    }
    #[test]
    fn merge_that_breaks_a_cross_field_rule_is_a_conflict() {
        // C20: phone moves the task to Fri 10-11, PC (later) sets stop Thu 12:00.
        let base = |c: &[(&str, i64)], h: &str, data: Value, at: i64| plain(data, at, c, &["100:A", h]);
        let local = base(&[("start", 200), ("stop", 200)], "200:A", json!({"title": "t", "start": "2026-11-20T10:00", "stop": "2026-11-20T11:00"}), 200);
        let remote = base(&[("start", 100), ("stop", 300)], "300:B", json!({"title": "t", "start": "2026-11-18T10:00", "stop": "2026-11-19T12:00"}), 300);
        assert_eq!(Store::field_merge(&local, &remote), None);
        assert!(merge_is_valid("tasks", &json!({"start": "2026-11-20", "stop": "2026-11-20T09:00"})));
        assert!(!merge_is_valid("sleep_log", &json!({"bed": 10, "wake": 5})));
        assert!(!merge_is_valid("loans", &json!({"amount": 100, "repayments": [{"amount": 60}, {"amount": 50}]})));
    }
}
