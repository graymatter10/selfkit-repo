//! JNI entry points for the Kotlin plugin (`android-plugins/selfkit-sync`,
//! class `com.selfkit.sync.SelfkitSyncNative`). Every app's Rust library links
//! this crate with feature `android`, so the same symbols exist in each app's
//! `lib<app>_lib.so`; the plugin loads that library by name and calls in.
//! All calls take/return JSON strings so the Kotlin side stays tiny.
//!
//! Kotlin signature (see SelfkitSyncNative.kt):
//!   external fun call(app: String, dataDir: String, wrapKey: ByteArray?, deviceName: String, method: String, argsJson: String): String

use std::sync::Arc;

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::jstring;
use serde_json::{Value, json};
use selfkit_core::crypto::SecretKey;

use crate::secrets::FileSecrets;

static WRAP_KEY: std::sync::OnceLock<SecretKey> = std::sync::OnceLock::new();

/// The Keystore-protected wrapping key handed over by `SelfkitSync.init()` (Kotlin).
pub fn wrap_key() -> Option<SecretKey> {
    WRAP_KEY.get().cloned()
}

/// Secret store for an app's own Rust code on Android (same file the plugin uses).
/// The wrapping key is looked up on every call, so it works even if the app's Rust
/// setup ran before `SelfkitSync.init()` handed the key over.
pub fn secrets(data_dir: &std::path::Path) -> AndroidSecrets {
    AndroidSecrets { path: data_dir.join("sync").join("secrets.json") }
}

pub struct AndroidSecrets {
    path: std::path::PathBuf,
}

impl AndroidSecrets {
    fn inner(&self) -> selfkit_core::Result<FileSecrets> {
        let k = wrap_key().ok_or_else(|| selfkit_core::Error::Keychain("secure storage not ready yet".into()))?;
        Ok(FileSecrets::new(&self.path, Some(k)))
    }
}

impl crate::secrets::SecretStore for AndroidSecrets {
    fn get(&self, name: &str) -> selfkit_core::Result<Option<String>> {
        self.inner()?.get(name)
    }
    fn set(&self, name: &str, value: &str) -> selfkit_core::Result<()> {
        self.inner()?.set(name, value)
    }
    fn delete(&self, name: &str) -> selfkit_core::Result<()> {
        self.inner()?.delete(name)
    }
}

use crate::{AppSync, AppSyncConfig, Choice, SyncSettings, Trigger};

fn run(app: &str, data_dir: &str, wrap: Option<Vec<u8>>, device_name: &str, method: &str, args: &str) -> Value {
    let wrap = wrap.and_then(|b| SecretKey::from_slice(&b).ok()).or_else(wrap_key);
    let secrets = Arc::new(FileSecrets::new(std::path::Path::new(data_dir).join("sync").join("secrets.json"), wrap));
    let mut cfg = AppSyncConfig::new(app, data_dir, secrets);
    if !device_name.is_empty() {
        cfg.device_name = device_name.into();
    }
    let sync = match AppSync::new(cfg) {
        Ok(s) => s,
        Err(e) => return json!({"ok": false, "error": crate::explain(&e)}),
    };
    sync.set_platform_device_name(device_name);
    let args: Value = serde_json::from_str(args).unwrap_or(Value::Null);
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => return json!({"ok": false, "error": e.to_string()}),
    };
    let res: selfkit_core::Result<Value> = rt.block_on(async {
        Ok(match method {
            "status" => serde_json::to_value(sync.status().await?)?,
            "sync_now" => {
                let trigger = match args["trigger"].as_str() {
                    Some("open") => Trigger::Open,
                    Some("background") => Trigger::Background,
                    _ => Trigger::Manual,
                };
                serde_json::to_value(sync.sync_now(trigger).await)?
            }
            "schedule" => serde_json::to_value(sync.schedule())?,
            "settings" => serde_json::to_value(sync.settings())?,
            "set_settings" => {
                let s: SyncSettings = serde_json::from_value(args)?;
                sync.set_settings(&s)?;
                json!({"ok": true})
            }
            "conflicts" => serde_json::to_value(sync.conflicts()?)?,
            "resolve" => {
                let choice: Choice = serde_json::from_value(args["choice"].clone())?;
                sync.resolve(args["store"].as_str().unwrap_or(app), args["id"].as_str().unwrap_or(""), &choice)?;
                json!({"ok": true})
            }
            // ContentProvider support (companion-ready): read changes, write a record.
            "changes" => {
                let since = args["since"].as_i64().unwrap_or(0);
                let stores = sync.open_stores()?;
                let s = stores.get(args["store"].as_str().unwrap_or(app)).ok_or_else(|| selfkit_core::Error::NotFound("store".into()))?;
                let rows: Vec<Value> = s
                    .changes_since(since, args["limit"].as_u64().unwrap_or(500) as u32)?
                    .into_iter()
                    .map(|(r, l)| json!({"lseq": l, "record": r}))
                    .collect();
                json!({"rows": rows})
            }
            "put" => {
                let stores = sync.open_stores()?;
                let s = stores.get(args["store"].as_str().unwrap_or(app)).ok_or_else(|| selfkit_core::Error::NotFound("store".into()))?;
                let r = s.put(args["collection"].as_str().unwrap_or(""), args["id"].as_str(), &args["data"])?;
                serde_json::to_value(r)?
            }
            // SelfkitMirror (read-only copies of other apps' data, e.g. for Reflect).
            "mirror_cursor" => {
                let store = mirror_store(&sync, args["store"].as_str().unwrap_or(""))?;
                json!({"cursor": store.kv_get("mirror.cursor")?.and_then(|s| s.parse::<i64>().ok()).unwrap_or(0)})
            }
            // Shared settings from sibling Selfkit apps on this phone (their providers):
            // merged field by field into this app's settings.db.
            "settings_adopt" => {
                let stores = sync.open_stores()?;
                let s = stores.get(selfkit_core::settings::APP).ok_or_else(|| selfkit_core::Error::NotFound("settings".into()))?;
                let recs: Vec<selfkit_core::Record> = serde_json::from_value(args["records"].clone())?;
                let mut changed = 0;
                for r in recs.iter().filter(|r| r.collection == selfkit_core::settings::COLLECTION) {
                    if s.apply_peer(r)? != selfkit_core::store::Applied::Unchanged {
                        changed += 1;
                    }
                }
                if changed > 0 {
                    // Tell the open page (tauri_cmds watcher) to reload (C16).
                    let _ = std::fs::write(crate::settings_adopted_marker(&sync.config().data_dir), selfkit_core::time::now_ms().to_string());
                }
                json!({"ok": true, "changed": changed})
            }
            "mirror_apply" => {
                let store = mirror_store(&sync, args["store"].as_str().unwrap_or(""))?;
                let recs: Vec<selfkit_core::Record> = serde_json::from_value(args["records"].clone())?;
                let tx = store.conn().unchecked_transaction()?;
                for r in &recs {
                    store.apply_peer(r)?;
                }
                store.kv_set("mirror.cursor", &args["upto"].as_i64().unwrap_or(0).to_string())?;
                tx.commit()?;
                json!({"ok": true, "applied": recs.len()})
            }
            // Local (non-record) tables an app shares read-only with sibling mirrors (B2:
            // Dusk's `dusk_nights` cache for Reflect). Never synced; phone-local only.
            "table_rows" => {
                let name = args["name"].as_str().unwrap_or("");
                if !SHARED_TABLES.contains(&(app, name)) {
                    return Err(selfkit_core::Error::Format(format!("not a shared table: {name}")));
                }
                let stores = sync.open_stores()?;
                let s = stores.get(app).ok_or_else(|| selfkit_core::Error::NotFound("store".into()))?;
                // Paged (offset/limit) so one cursor row stays under Android's ~2 MB
                // CursorWindow; no limit = whole table (older mirrors).
                let (offset, limit) = (args["offset"].as_i64().unwrap_or(0).max(0), args["limit"].as_i64().unwrap_or(-1));
                let (sql, rows, count) = table_dump(s, name, offset, limit)?;
                json!({"ok": true, "sql": sql, "rows": rows, "count": count})
            }
            "mirror_table" => {
                let from = args["store"].as_str().unwrap_or("");
                let name = args["name"].as_str().unwrap_or("");
                if !SHARED_TABLES.contains(&(from, name)) {
                    return Err(selfkit_core::Error::Format(format!("not a shared table: {from}/{name}")));
                }
                let store = mirror_store(&sync, from)?;
                let n = table_replace(&store, name, args["sql"].as_str().unwrap_or(""), args["rows"].as_str().unwrap_or("[]"))?;
                json!({"ok": true, "rows": n})
            }
            other => json!({"ok": false, "error": format!("unknown method {other}")}),
        })
    });
    res.unwrap_or_else(|e| json!({"ok": false, "error": crate::explain(&e), "detail": e.to_string()}))
}

/// (app, table) pairs an app's provider may hand out whole (`/table?name=`), and a mirror
/// may copy into its `<app>.db`. Keep in sync with `TABLES` in SelfkitMirror.kt.
const SHARED_TABLES: &[(&str, &str)] = &[("dusk", "dusk_nights")];

fn table_columns(s: &selfkit_core::Store, name: &str) -> selfkit_core::Result<Vec<String>> {
    let mut st = s.conn().prepare("SELECT name FROM pragma_table_info(?1)")?;
    let cols = st.query_map([name], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(cols)
}

/// The table's CREATE statement, one page of rows (rowid order; `limit` -1 = all) as a
/// JSON array of objects, and how many rows that page holds.
fn table_dump(s: &selfkit_core::Store, name: &str, offset: i64, limit: i64) -> selfkit_core::Result<(String, String, i64)> {
    let sql: String = s.conn().query_row("SELECT sql FROM sqlite_master WHERE type='table' AND name=?1", [name], |r| r.get(0))?;
    let obj = table_columns(s, name)?.iter().map(|c| format!("'{c}', \"{c}\"")).collect::<Vec<_>>().join(", ");
    let (rows, count): (String, i64) = s.conn().query_row(
        &format!("SELECT COALESCE(json_group_array(json_object({obj})), '[]'), COUNT(*) FROM (SELECT * FROM \"{name}\" ORDER BY rowid LIMIT ?1 OFFSET ?2)"),
        [limit, offset],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((sql, rows, count))
}

/// Replace the mirror's copy of a shared table (recreated from the source's schema).
fn table_replace(s: &selfkit_core::Store, name: &str, create: &str, rows: &str) -> selfkit_core::Result<usize> {
    let create = create.trim().trim_end_matches(';');
    if !create.to_ascii_uppercase().starts_with("CREATE TABLE") || !create.contains(name) || create.contains(';') {
        return Err(selfkit_core::Error::Format(format!("bad schema for {name}")));
    }
    let tx = s.conn().unchecked_transaction()?;
    s.conn().execute(&format!("DROP TABLE IF EXISTS \"{name}\""), [])?;
    s.conn().execute(create, [])?;
    let cols = table_columns(s, name)?;
    let list = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let vals = cols.iter().map(|c| format!("json_extract(value, '$.{c}')")).collect::<Vec<_>>().join(", ");
    let n = s.conn().execute(&format!("INSERT OR REPLACE INTO \"{name}\" ({list}) SELECT {vals} FROM json_each(?1)"), [rows])?;
    tx.commit()?;
    Ok(n)
}

/// `<data_dir>/<app>.db` holding another app's records (never this app's own DB).
fn mirror_store(sync: &AppSync, app: &str) -> selfkit_core::Result<selfkit_core::Store> {
    if !selfkit_core::paths::APPS.contains(&app) || app == sync.config().app {
        return Err(selfkit_core::Error::Format(format!("not a mirror store: {app}")));
    }
    let dev = sync.device_id()?;
    selfkit_core::Store::open(sync.config().data_dir.join(format!("{app}.db")), app, &dev, &[])
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_selfkit_sync_SelfkitSyncNative_call<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    app: JString<'l>,
    data_dir: JString<'l>,
    wrap_key: JByteArray<'l>,
    device_name: JString<'l>,
    method: JString<'l>,
    args: JString<'l>,
) -> jstring {
    let mut s = |j: &JString| -> String { env.get_string(j).map(|s| s.into()).unwrap_or_default() };
    let (app, data_dir, device_name, method, args) = (s(&app), s(&data_dir), s(&device_name), s(&method), s(&args));
    let wrap = if wrap_key.is_null() { None } else { env.convert_byte_array(&wrap_key).ok() };
    let out = std::panic::catch_unwind(|| run(&app, &data_dir, wrap, &device_name, &method, &args))
        .unwrap_or_else(|_| json!({"ok": false, "error": "Sync crashed. Details are in the log."}));
    env.new_string(out.to_string()).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_selfkit_sync_SelfkitSyncNative_setWrapKey<'l>(env: JNIEnv<'l>, _class: JClass<'l>, key: JByteArray<'l>) {
    if let Ok(b) = env.convert_byte_array(&key)
        && let Ok(k) = SecretKey::from_slice(&b)
    {
        let _ = WRAP_KEY.set(k);
    }
}
