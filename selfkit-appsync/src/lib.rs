//! selfkit-appsync: the app side of Selfkit sync, shared by every app (and by a
//! future companion app). One small interface:
//!
//! - [`AppSync::status`] — what the Sync settings screen shows.
//! - [`AppSync::sync_now`] — one pass: LAN to a reachable peer (usually the
//!   desktop agent) when on the same network, otherwise Postgres + S3.
//! - [`AppSync::settings`] / [`AppSync::set_settings`] — per-device settings incl. the sync owner.
//! - [`AppSync::conflicts`] / [`AppSync::resolve`] — the Keep A / Keep B / Merge screen.
//! - [`AppSync::due`] / [`AppSync::schedule`] — scheduler hook (on open, ~15 min job).
//!
//! Docs: docs/components/selfkit-appsync.md, docs/data-and-sync.md (interface).

pub mod agent;
#[cfg(feature = "android")]
pub mod android;
pub mod conflicts;
pub mod explain;
pub mod lan;
#[cfg(feature = "tauri")]
pub mod lock_cmds;
pub mod model;
pub mod secrets;
pub mod server;
#[cfg(feature = "tauri")]
pub mod tauri_cmds;

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use selfkit_core::crypto::SecretKey;
use selfkit_core::keys::{KdfParams, KeyFile};
use selfkit_core::sync::s3::{BlobStore, S3Config};
use selfkit_core::{Error, Result, Store, time};
use sha2::Digest;

pub use conflicts::{Choice, ConflictView};
pub use explain::explain;
pub use model::*;
pub use secrets::SecretStore;
pub use server::{PhotoConfig, ServerConfig};

/// What one app tells the sync module about itself.
#[derive(Clone)]
pub struct AppSyncConfig {
    /// "agenda", "hisab", "dusk", "chrono", "reflect".
    pub app: String,
    /// Selfkit data folder (desktop: shared folder; Android: the app's files dir).
    pub data_dir: PathBuf,
    /// Also sync the shared `settings` DB (day start, currency, theme). Default true.
    pub include_settings: bool,
    pub photos: Option<PhotoConfig>,
    pub secrets: Arc<dyn SecretStore>,
    /// Use the built-in test key when no real key is set (test builds: no password prompts).
    pub test_key: bool,
    /// Owner when the settings file doesn't exist yet (Android: App; desktop: Agent).
    pub default_owner: SyncOwner,
    /// Fallback device name ("Pixel phone", hostname).
    pub device_name: String,
    /// Compile-time/test server used only together with the test key when no server is configured.
    pub test_server: Option<ServerConfig>,
}

impl AppSyncConfig {
    pub fn new(app: &str, data_dir: impl Into<PathBuf>, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            app: app.into(),
            data_dir: data_dir.into(),
            include_settings: true,
            photos: Some(PhotoConfig { collection: "photos".into(), dir: format!("{app}-photos") }),
            secrets,
            test_key: cfg!(feature = "test-key"),
            default_owner: if cfg!(target_os = "android") { SyncOwner::App } else { SyncOwner::Agent },
            device_name: default_device_name(),
            test_server: None,
        }
    }
}

pub fn default_device_name() -> String {
    if cfg!(target_os = "android") {
        return "Android phone".into();
    }
    std::env::var("SELFKIT_DEVICE_NAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok().map(|s| s.trim().to_string()))
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Selfkit device".into())
}

/// True in builds made by the release workflows (`SELFKIT_RELEASE=1` at compile time,
/// see .github/workflows/release.yml). Such builds never use the public test key or the
/// test server, whatever the Cargo features say. See docs/security.md.
pub fn release_build() -> bool {
    option_env!("SELFKIT_RELEASE") == Some("1")
}

/// The public, built-in test key. Anything encrypted with it is NOT private.
pub fn test_data_key() -> SecretKey {
    let h = sha2::Sha256::digest(b"selfkit-test-key-v1 (public; test builds only, never real data)");
    SecretKey::from_slice(&h).expect("32 bytes")
}

/// Scheduler info for the platform job (WorkManager on Android).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Schedule {
    /// False = cancel the periodic job (owner elsewhere or auto sync off).
    pub enabled: bool,
    pub every_minutes: u32,
    pub run_on_open: bool,
}

static RUNNING: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);
const LOCK_STALE_MS: i64 = 15 * 60 * 1000;
const PASS_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const OPEN_MIN_GAP_MS: i64 = 60 * 1000;
const PEER_MEMORY_MS: i64 = 14 * 24 * 3600 * 1000;

/// Held for the duration of one pass (in-process set + lock file for other processes).
struct PassLock {
    path: PathBuf,
}

impl PassLock {
    fn acquire(path: PathBuf) -> Option<Self> {
        {
            let mut g = RUNNING.lock().unwrap();
            let set = g.get_or_insert_with(HashSet::new);
            if !set.insert(path.clone()) {
                return None;
            }
        }
        let release = |p: &Path| {
            RUNNING.lock().unwrap().as_mut().map(|s| s.remove(p));
        };
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let me = format!("{} {}", std::process::id(), time::now_ms());
        for _ in 0..2 {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    use std::io::Write;
                    let _ = f.write_all(me.as_bytes());
                    return Some(Self { path });
                }
                Err(_) => {
                    // Someone holds it: take over only if stale (crashed / killed pass).
                    let held = std::fs::read_to_string(&path).unwrap_or_default();
                    let mut it = held.split_whitespace();
                    let pid: Option<u32> = it.next().and_then(|s| s.parse().ok());
                    let at: i64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    let dead = match pid {
                        Some(p) if p == std::process::id() => true, // left over in this process
                        #[cfg(any(target_os = "linux", target_os = "android"))]
                        Some(p) => !Path::new(&format!("/proc/{p}")).exists(),
                        _ => false,
                    };
                    if dead || time::now_ms() - at > LOCK_STALE_MS {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    release(&path);
                    return None;
                }
            }
        }
        release(&path);
        None
    }
}

impl Drop for PassLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        RUNNING.lock().unwrap().as_mut().map(|s| s.remove(&self.path));
    }
}

pub struct AppSync {
    cfg: AppSyncConfig,
}

impl AppSync {
    pub fn new(cfg: AppSyncConfig) -> Result<Self> {
        std::fs::create_dir_all(model::sync_dir(&cfg.data_dir))?;
        Ok(Self { cfg })
    }

    pub fn config(&self) -> &AppSyncConfig {
        &self.cfg
    }

    // ---- paths, identity, stores ------------------------------------------

    fn settings_path(&self) -> PathBuf {
        model::sync_dir(&self.cfg.data_dir).join("settings.json")
    }
    fn state_path(&self) -> PathBuf {
        model::sync_dir(&self.cfg.data_dir).join(format!("state-{}.json", self.cfg.app))
    }
    fn lock_path(&self) -> PathBuf {
        model::sync_dir(&self.cfg.data_dir).join(format!("{}.lock", self.cfg.app))
    }
    fn key_file_path(&self) -> PathBuf {
        self.cfg.data_dir.join("keys.json")
    }

    /// Same file and format as `selfkit_core::paths::device_id`, but for this data dir.
    pub fn device_id(&self) -> Result<String> {
        let p = self.cfg.data_dir.join("device_id");
        if let Ok(s) = std::fs::read_to_string(&p) {
            let s = s.trim().to_string();
            if !s.is_empty() {
                return Ok(s);
            }
        }
        std::fs::create_dir_all(&self.cfg.data_dir)?;
        let id = selfkit_core::store::new_id();
        std::fs::write(&p, &id)?;
        Ok(id)
    }

    /// Settings name, else the name the platform reported (Android: "<maker> <model>",
    /// written by the Kotlin plugin), else the config default.
    pub fn device_name(&self) -> String {
        self.settings()
            .device_name
            .filter(|s| !s.trim().is_empty())
            .or_else(|| std::fs::read_to_string(model::sync_dir(&self.cfg.data_dir).join("device-name")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| self.cfg.device_name.clone())
    }

    /// Remember the platform's device name (used when the user hasn't set one).
    pub fn set_platform_device_name(&self, name: &str) {
        let p = model::sync_dir(&self.cfg.data_dir).join("device-name");
        if !name.trim().is_empty() && std::fs::read_to_string(&p).ok().as_deref() != Some(name) {
            let _ = std::fs::write(p, name);
        }
    }

    fn store_names(&self) -> Vec<String> {
        let mut v = vec![self.cfg.app.clone()];
        if self.cfg.include_settings && self.cfg.app != selfkit_core::settings::APP {
            v.push(selfkit_core::settings::APP.into());
        }
        v
    }

    /// Open the stores this app syncs (its own DB and the shared settings DB).
    pub fn open_stores(&self) -> Result<HashMap<String, Store>> {
        let dev = self.device_id()?;
        self.store_names()
            .into_iter()
            .map(|n| Ok((n.clone(), Store::open(self.cfg.data_dir.join(format!("{n}.db")), &n, &dev, &[])?)))
            .collect()
    }

    // ---- settings -----------------------------------------------------------

    pub fn settings(&self) -> SyncSettings {
        if !self.settings_path().exists() {
            return SyncSettings { owner: self.cfg.default_owner, ..Default::default() };
        }
        model::load_json(&self.settings_path())
    }

    pub fn set_settings(&self, s: &SyncSettings) -> Result<()> {
        model::save_json(&self.settings_path(), s)
    }

    pub fn state(&self) -> SyncState {
        model::load_json(&self.state_path())
    }

    // ---- keys -----------------------------------------------------------------

    /// The data key: secure storage first, then the built-in test key (test builds).
    pub fn data_key(&self) -> Result<Option<(SecretKey, bool)>> {
        match self.cfg.secrets.get(secrets::DATA_KEY) {
            Ok(Some(b)) => {
                let raw = B64.decode(b.trim()).map_err(|_| Error::Format("data key base64".into()))?;
                Ok(Some((SecretKey::from_slice(&raw)?, false)))
            }
            // Test builds keep working when secure storage isn't available, but only
            // while no real encryption is set up: a device with keys.json whose secure
            // storage is unreadable is Locked, never silently on the public key.
            Ok(None) | Err(_) if self.test_key_allowed() => Ok(Some((test_data_key(), true))),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The public test key may be used: test build (feature/config), not a release
    /// build, and no real key file on this device.
    fn test_key_allowed(&self) -> bool {
        self.cfg.test_key && !release_build() && !self.encryption_set_up()
    }

    pub fn encryption_set_up(&self) -> bool {
        self.key_file_path().exists()
    }

    fn store_key(&self, k: &SecretKey) -> Result<()> {
        self.cfg.secrets.set(secrets::DATA_KEY, &B64.encode(k.as_bytes()))
    }

    /// First device: create the key, wrap it with the passphrase and a new recovery
    /// key (returned once, for the UI to show), and unlock this device.
    pub fn setup_encryption(&self, passphrase: &str) -> Result<String> {
        self.setup_encryption_with(passphrase, KdfParams::default())
    }

    pub fn setup_encryption_with(&self, passphrase: &str, params: KdfParams) -> Result<String> {
        if self.encryption_set_up() {
            return Err(Error::Format("encryption is already set up; unlock instead".into()));
        }
        let was_test = matches!(self.data_key(), Ok(Some((_, true))));
        let (kf, data, rk) = KeyFile::create(passphrase, params)?;
        kf.save(&self.key_file_path())?;
        self.store_key(&data)?;
        if was_test {
            // Data synced so far was encrypted with the public test key: re-upload it all
            // with the new key, and let the next server pass claim the server.
            self.reset_for_new_key()?;
            std::fs::write(model::sync_dir(&self.cfg.data_dir).join("rekey-from-test"), "1")?;
        }
        Ok(rk.as_str().to_string())
    }

    /// After a key change: forget server revisions and uploaded-photo marks so the
    /// next pass re-uploads everything encrypted with the new key.
    fn reset_for_new_key(&self) -> Result<()> {
        for store in self.open_stores()?.values() {
            store.reset_sync_state()?;
            store.conn().execute("DELETE FROM kv WHERE k LIKE 'appsync.blob.up.%'", [])?;
        }
        Ok(())
    }

    /// Get the newest published `keys.json`: the server (sk_meta, works without S3),
    /// else S3 (`bundles/_keys/keys.json`, also written by the agent), else the local copy.
    async fn ensure_key_file(&self) -> Result<KeyFile> {
        let p = self.key_file_path();
        if let Some(cfg) = self.server_config()? {
            let fetched = async {
                let remote = server::connect(&cfg.pg_url).await?;
                server::download_keyfile(&remote).await
            }
            .await;
            match fetched {
                Ok(Some(text)) => {
                    std::fs::create_dir_all(&self.cfg.data_dir)?;
                    std::fs::write(&p, text)?;
                }
                _ if !p.exists() => {
                    if let Some(s3) = cfg.s3 {
                        BlobStore::new(s3)?.get_bundle("bundles/_keys/keys.json", &p).await?;
                    }
                }
                _ => {}
            }
        }
        KeyFile::load(&p)
    }

    pub async fn unlock(&self, passphrase: &str) -> Result<()> {
        let k = self.ensure_key_file().await?.unlock_passphrase(passphrase)?;
        self.store_new_key(&k)
    }

    pub async fn unlock_recovery(&self, recovery_key: &str) -> Result<()> {
        let k = self.ensure_key_file().await?.unlock_recovery(recovery_key)?;
        self.store_new_key(&k)
    }

    /// Store an unlocked key; when this device was on the test key, re-upload its data.
    fn store_new_key(&self, k: &SecretKey) -> Result<()> {
        let was_test = matches!(self.data_key(), Ok(Some((_, true))));
        self.store_key(k)?;
        if was_test {
            self.reset_for_new_key()?;
        }
        Ok(())
    }

    /// Re-wraps the key with the new passphrase; the next server pass publishes it.
    pub fn change_passphrase(&self, old: &str, new: &str) -> Result<()> {
        let mut kf = KeyFile::load(&self.key_file_path())?;
        let k = kf.unlock_passphrase(old)?;
        kf.change_passphrase(&k, new, KdfParams::default())?;
        kf.save(&self.key_file_path())
    }

    /// Forget the key on this device (data stays; sync stops until unlocked).
    pub fn lock(&self) -> Result<()> {
        self.cfg.secrets.delete(secrets::DATA_KEY)
    }

    // ---- server ---------------------------------------------------------------

    pub fn server_config(&self) -> Result<Option<ServerConfig>> {
        match self.cfg.secrets.get(secrets::PG_URL)? {
            Some(url) if !url.trim().is_empty() => {
                let s3 = match self.cfg.secrets.get(secrets::S3_CONFIG)? {
                    Some(j) if !j.trim().is_empty() => Some(serde_json::from_str::<S3Config>(&j)?),
                    _ => None,
                };
                Ok(Some(ServerConfig { pg_url: url, s3 }))
            }
            _ => Ok(if self.cfg.test_key && !release_build() { self.cfg.test_server.clone() } else { None }),
        }
    }

    pub fn set_server(&self, cfg: Option<&ServerConfig>) -> Result<()> {
        match cfg {
            Some(c) => {
                self.cfg.secrets.set(secrets::PG_URL, &c.pg_url)?;
                match &c.s3 {
                    Some(s3) => self.cfg.secrets.set(secrets::S3_CONFIG, &serde_json::to_string(s3)?)?,
                    None => self.cfg.secrets.delete(secrets::S3_CONFIG)?,
                }
            }
            None => {
                self.cfg.secrets.delete(secrets::PG_URL)?;
                self.cfg.secrets.delete(secrets::S3_CONFIG)?;
            }
        }
        Ok(())
    }

    /// The "Test" button: Ok(()) or a plain-language error.
    pub async fn test_server(&self, cfg: &ServerConfig) -> std::result::Result<(), String> {
        let key = self.data_key().ok().flatten().map(|(k, _)| k);
        server::test(cfg, key.as_ref()).await.map_err(|e| explain(&e))
    }

    // ---- conflicts ------------------------------------------------------------

    pub fn conflicts(&self) -> Result<Vec<ConflictView>> {
        let mut out = vec![];
        for (name, store) in self.open_stores()? {
            out.extend(conflicts::view(&name, &store)?);
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.b.updated_at.max(c.a.updated_at)));
        Ok(out)
    }

    /// `store` = ConflictView.store. Resolutions are pushed on the next sync.
    pub fn resolve(&self, store: &str, id: &str, choice: &Choice) -> Result<()> {
        let stores = self.open_stores()?;
        let s = stores.get(store).ok_or_else(|| Error::NotFound(store.into()))?;
        conflicts::resolve(s, id, choice)
    }

    // ---- scheduler hook -----------------------------------------------------

    pub fn schedule(&self) -> Schedule {
        let s = self.settings();
        let mine = s.owner == SyncOwner::App;
        Schedule { enabled: s.enabled && mine && s.auto_sync, every_minutes: s.background_minutes.max(15), run_on_open: s.enabled && mine && s.auto_sync }
    }

    /// Should a pass run now for this trigger? Manual always; Open at most once a
    /// minute; Background when the period has (almost) passed.
    pub fn due(&self, trigger: Trigger) -> bool {
        let s = self.settings();
        if !s.enabled {
            return false;
        }
        if trigger == Trigger::Manual {
            return true;
        }
        if !s.auto_sync || s.owner != SyncOwner::App {
            return false;
        }
        let last = self.state().last_attempt_at.unwrap_or(0);
        let gap = time::now_ms() - last;
        match trigger {
            Trigger::Open => gap >= OPEN_MIN_GAP_MS,
            Trigger::Background => gap >= (s.background_minutes.max(15) as i64 - 2) * 60_000,
            Trigger::Manual => true,
        }
    }

    // ---- status ---------------------------------------------------------------

    pub async fn status(&self) -> Result<SyncStatus> {
        let s = self.settings();
        let st = self.state();
        let now = time::now_ms();
        let (mut pending, mut nconf) = (0, 0);
        if let Ok(stores) = self.open_stores() {
            let me = self.device_id().unwrap_or_default();
            for store in stores.values() {
                // Only this device's own edits: records received over LAN stay dirty (so they
                // still reach the server) but aren't "your changes".
                pending += store
                    .conn()
                    .query_row("SELECT count(*) FROM records WHERE dirty=1 AND device_id=? AND collection<>'devices'", [&me], |r| r.get::<_, i64>(0))
                    .unwrap_or(0);
                nconf += store.conflicts().map(|c| c.len() as i64).unwrap_or(0);
            }
        }
        let key = self.data_key().ok().flatten();
        let server = self.server_config().ok().flatten();
        let running = RUNNING.lock().unwrap().as_ref().is_some_and(|r| r.contains(&self.lock_path()));
        let last = st.last.clone();
        let mut device_names = std::collections::BTreeMap::new();
        if let Ok(stores) = self.open_stores()
            && let Some(s) = stores.get(selfkit_core::settings::APP)
            && let Ok(mut q) = s.conn().prepare("SELECT id, json_extract(data, '$.name') FROM records WHERE collection='devices' AND deleted=0")
            && let Ok(rows) = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))
        {
            for (id, name) in rows.flatten() {
                if let (Some(dev), Some(n)) = (id.strip_prefix("dev-"), name) {
                    device_names.insert(dev.to_string(), n);
                }
            }
        }
        let mut peers: Vec<SeenPeer> = st.peers.values().cloned().collect();
        peers.sort_by_key(|p| std::cmp::Reverse(p.last_seen));

        let mut status = SyncStatus {
            app: self.cfg.app.clone(),
            device_id: self.device_id()?,
            device_name: self.device_name(),
            owner: s.owner,
            running,
            unlocked: key.is_some(),
            test_key: key.as_ref().is_some_and(|k| k.1),
            encryption_set_up: self.encryption_set_up(),
            server_configured: server.is_some(),
            photos_configured: server.as_ref().is_some_and(|c| c.s3.is_some()),
            last_attempt_at: st.last_attempt_at,
            last_ok_at: st.last_ok_at,
            last_transport: last.as_ref().and_then(|l| l.transport),
            last_peers: last.as_ref().map(|l| l.peers.clone()).unwrap_or_default(),
            last_error: last.as_ref().and_then(|l| l.error.clone()),
            last_note: last.as_ref().and_then(|l| l.note.clone()),
            pending,
            conflicts: nconf,
            peers,
            device_names,
            headline: String::new(),
            next_auto_at: (s.enabled && s.owner == SyncOwner::App && s.auto_sync)
                .then(|| st.last_attempt_at.map(|t| t + s.background_minutes.max(15) as i64 * 60_000))
                .flatten(),
        };
        if s.owner == SyncOwner::Agent
            && let Ok(v) = agent::status().await
        {
            let a = &v["status"];
            status.last_transport = Some(Transport::Agent);
            status.last_ok_at = a["last_sync_ok_at"].as_i64().or(status.last_ok_at);
            status.last_attempt_at = a["last_sync_at"].as_i64().or(status.last_attempt_at);
            status.last_error = a["last_sync_error"].as_str().map(|e| explain(&Error::Remote(e.into())));
        }
        status.headline = if s.enabled { headline(&status, now) } else { "Sync is off".into() };
        Ok(status)
    }

    // ---- the pass -----------------------------------------------------------

    /// Run one sync pass now (or explain why not). Never panics; errors are in the outcome.
    pub async fn sync_now(&self, trigger: Trigger) -> SyncOutcome {
        let started = time::now_ms();
        let skip = |note: &str| SyncOutcome {
            ok: true,
            ran: false,
            transport: None,
            peers: vec![],
            report: PassReport::default(),
            error: None,
            detail: None,
            note: Some(note.into()),
            started_at: started,
            finished_at: time::now_ms(),
        };
        let settings = self.settings();
        if !settings.enabled {
            return skip("Sync is off. Turn it on in Sync settings.");
        }
        let mut agent_note = None;
        match settings.owner {
            SyncOwner::Companion => return skip("The Selfkit companion syncs this app."),
            SyncOwner::Agent => match agent::sync_now().await {
                Ok(()) => {
                    let mut o = skip("Asked the Selfkit agent to sync.");
                    o.ran = true;
                    o.transport = Some(Transport::Agent);
                    return o;
                }
                // No agent running: sync here so the button still works.
                Err(_) => agent_note = Some("The Selfkit agent isn't running, so this app synced by itself.".to_string()),
            },
            SyncOwner::App => {}
        }
        if !self.due(trigger) && settings.owner == SyncOwner::App {
            return skip("Not due yet.");
        }
        let Some(_lock) = PassLock::acquire(self.lock_path()) else {
            return skip("Already syncing.");
        };
        let mut out = match tokio::time::timeout(PASS_TIMEOUT, self.pass(&settings)).await {
            Ok(o) => o,
            Err(_) => SyncOutcome {
                ok: false,
                ran: true,
                transport: None,
                peers: vec![],
                report: PassReport::default(),
                error: Some(explain(&Error::Remote(explain::TIMEOUT.into()))),
                detail: Some("pass timed out".into()),
                note: None,
                started_at: started,
                finished_at: 0,
            },
        };
        out.started_at = started;
        out.finished_at = time::now_ms();
        if out.note.is_none() {
            out.note = agent_note;
        }
        if let Some(d) = &out.detail {
            tracing::warn!(app = self.cfg.app, "sync failed: {d}");
        }
        let mut st = self.state();
        st.last_attempt_at = Some(started);
        if out.ok {
            st.last_ok_at = Some(out.finished_at);
        }
        st.last = Some(out.clone());
        st.log.insert(0, out.clone());
        st.log.truncate(20);
        // Merge peers seen in this pass (written by pass() into a side file).
        if let Ok(extra) = std::fs::read(self.state_path().with_extension("peers"))
            && let Ok(m) = serde_json::from_slice::<Vec<SeenPeer>>(&extra)
        {
            for p in m {
                let e = st.peers.entry(p.device_id.clone()).or_insert(p.clone());
                e.name = p.name;
                e.addr = p.addr;
                e.last_seen = e.last_seen.max(p.last_seen);
                e.last_sync = p.last_sync.or(e.last_sync);
            }
            let _ = std::fs::remove_file(self.state_path().with_extension("peers"));
        }
        st.peers.retain(|_, p| out.finished_at - p.last_seen < PEER_MEMORY_MS);
        if let Err(e) = model::save_json(&self.state_path(), &st) {
            tracing::warn!("saving sync state: {e}");
        }
        out
    }

    async fn pass(&self, settings: &SyncSettings) -> SyncOutcome {
        let mut out = SyncOutcome {
            ok: false,
            ran: true,
            transport: None,
            peers: vec![],
            report: PassReport::default(),
            error: None,
            detail: None,
            note: None,
            started_at: 0,
            finished_at: 0,
        };
        if let Err(e) = self.pass_inner(settings, &mut out).await {
            out.ok = false;
            out.error = Some(explain(&e));
            out.detail = Some(e.to_string());
        }
        out
    }

    async fn pass_inner(&self, settings: &SyncSettings, out: &mut SyncOutcome) -> Result<()> {
        let (key, _test) = self.data_key()?.ok_or_else(|| Error::Remote(explain::LOCKED.into()))?;
        let mut stores = self.open_stores()?;
        // Device directory (shared settings DB): lets every device show names on the
        // conflict screen, also for devices it never met directly.
        if let Some(s) = stores.get(selfkit_core::settings::APP) {
            let (id, name) = (format!("dev-{}", self.device_id()?), self.device_name());
            let platform = if cfg!(target_os = "android") { "android" } else { std::env::consts::OS };
            if s.get(&id)?.map(|r| r.data["name"] != name.as_str() || r.deleted).unwrap_or(true) {
                s.put(selfkit_core::store::DEVICES, Some(&id), &serde_json::json!({"name": name, "platform": platform}))?;
            }
        }
        let server_cfg = self.server_config()?;
        let lan_allowed = settings.lan_enabled && settings.transport != TransportMode::ServerOnly;
        let server_allowed = settings.transport != TransportMode::LanOnly && server_cfg.is_some();
        let now = time::now_ms();

        let mut lan_problem: Option<Error> = None;
        if lan_allowed {
            let state = self.state();
            let mut candidates: Vec<SocketAddr> = settings.lan_peers.iter().filter_map(|s| s.trim().parse().ok()).collect();
            candidates.extend(state.peers.values().filter(|p| now - p.last_seen < PEER_MEMORY_MS).filter_map(|p| p.addr.parse::<SocketAddr>().ok()));
            let cfg = lan::LanPassConfig {
                device_id: self.device_id()?,
                device_name: self.device_name(),
                candidates,
                discovery_port: (settings.lan_discovery_ms > 0).then_some(settings.lan_discovery_port),
                discovery_wait: Duration::from_millis(settings.lan_discovery_ms),
                connect_timeout: Duration::from_secs(3),
            };
            let res = lan::pass(&key, &cfg, &mut stores).await;
            let synced_ids: HashSet<&String> = res.sessions.iter().map(|(_, r)| &r.peer_id).collect();
            let seen: Vec<SeenPeer> = res
                .seen
                .iter()
                .map(|(id, (name, addr))| SeenPeer {
                    device_id: id.clone(),
                    name: name.clone(),
                    addr: addr.to_string(),
                    last_seen: now,
                    last_sync: synced_ids.contains(id).then_some(now),
                })
                .collect();
            let _ = model::save_json(&self.state_path().with_extension("peers"), &seen);
            for (_, rep) in &res.sessions {
                out.report.pulled += rep.received;
                out.report.pushed += rep.sent;
                out.report.conflicts += rep.conflicts;
                if !out.peers.contains(&rep.peer_name) {
                    out.peers.push(rep.peer_name.clone());
                }
            }
            // Only errors that mean "found a device but it refused us" matter to the user;
            // unreachable remembered addresses are normal.
            lan_problem = res.errors.into_iter().map(|(_, e)| e).find(|e| {
                let t = e.to_string();
                matches!(e, Error::BadKey | Error::Decrypt) || t.contains("protocol") || t.contains("early eof") || t.contains("frame")
            });
            if res.sessions.iter().any(|_| true) {
                out.transport = Some(Transport::Lan);
                out.ok = true;
                if let (Some(pc), Some(ServerConfig { s3: Some(s3), .. })) = (&self.cfg.photos, &server_cfg)
                    && settings.transport != TransportMode::LanOnly
                {
                    // Photos only travel through S3; try it, but a LAN pass is a success either way.
                    if let Err(e) = self.photos(&key, &mut stores, pc, s3, &mut out.report).await {
                        out.note = Some(format!("Photos wait for the server: {}", explain(&e)));
                    }
                }
                return Ok(());
            }
        }

        if server_allowed {
            let cfg = server_cfg.expect("checked");
            let remote = server::connect(&cfg.pg_url).await?;
            let rekey = model::sync_dir(&self.cfg.data_dir).join("rekey-from-test");
            if rekey.exists() {
                if server::take_over_from_test_key(&remote, &key).await? {
                    out.note = Some("Moved to your own key: this device re-uploaded its data. Unlock your other devices with your passphrase.".into());
                }
                let _ = std::fs::remove_file(&rekey);
            }
            server::preflight(&remote, &key).await?;
            server::upload_keyfile(&remote, cfg.s3.as_ref(), &self.key_file_path()).await?;
            server::sync_records(&remote, &key, &mut stores, &mut out.report).await?;
            out.transport = Some(Transport::Server);
            out.ok = true;
            if let (Some(pc), Some(s3)) = (&self.cfg.photos, &cfg.s3)
                && let Err(e) = self.photos(&key, &mut stores, pc, s3, &mut out.report).await
            {
                out.note = Some(format!("Photos didn't sync: {}", explain(&e)));
            }
            if out.report.undecryptable > 0 {
                out.note = Some(format!("{} items on the server couldn't be read with this device's key.", out.report.undecryptable));
            }
            return Ok(());
        }

        if let Some(e) = lan_problem {
            let msg = e.to_string();
            return Err(if matches!(e, Error::BadKey | Error::Decrypt) || msg.contains("early eof") || msg.contains("frame") {
                Error::Remote(format!("{}: lan peer refused us ({msg})", explain::WRONG_KEY))
            } else {
                e
            });
        }
        Err(Error::Remote(
            match settings.transport {
                TransportMode::LanOnly => explain::NO_PEER,
                TransportMode::ServerOnly => explain::NO_SERVER,
                TransportMode::Auto if lan_allowed => explain::NOTHING,
                TransportMode::Auto => explain::NO_SERVER,
            }
            .into(),
        ))
    }

    async fn photos(&self, key: &SecretKey, stores: &mut HashMap<String, Store>, pc: &PhotoConfig, s3: &S3Config, rep: &mut PassReport) -> Result<()> {
        let bs = BlobStore::new(s3.clone())?;
        if let Some(store) = stores.get_mut(&self.cfg.app) {
            server::sync_photos(&bs, key, store, &self.cfg.data_dir, pc, rep).await?;
        }
        Ok(())
    }
}

fn headline(s: &SyncStatus, now: i64) -> String {
    if s.running {
        return "Syncing…".into();
    }
    if s.owner == SyncOwner::Companion {
        return "Synced by the Selfkit companion".into();
    }
    if !s.unlocked {
        return if s.encryption_set_up { "Locked · unlock to sync".into() } else { "Not set up · add encryption to sync".into() };
    }
    let failed_last = matches!((s.last_attempt_at, s.last_ok_at), (Some(a), Some(o)) if a > o) || (s.last_attempt_at.is_some() && s.last_ok_at.is_none());
    if failed_last {
        let e = s.last_error.clone().unwrap_or_default();
        if e.starts_with("Can't reach") || e.contains("timed out") || e.starts_with("No Selfkit PC") || e.starts_with("Nothing to sync") {
            return if s.pending > 0 && s.server_configured {
                format!("Waiting to sync · {} change{} saved here", s.pending, if s.pending == 1 { "" } else { "s" })
            } else {
                "Waiting to sync · no connection".into()
            };
        }
        return format!("Not synced · {e}");
    }
    match (s.last_ok_at, s.last_transport) {
        (Some(t), Some(Transport::Lan)) => {
            let who = s.last_peers.first().cloned().unwrap_or_else(|| "your PC".into());
            format!("Synced locally with {who} · {}", ago(now, t))
        }
        (Some(t), Some(Transport::Server)) => format!("Synced via server · {}", ago(now, t)),
        (Some(t), Some(Transport::Agent)) => format!("Synced by the Selfkit agent · {}", ago(now, t)),
        _ => "Not synced yet".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ago_words() {
        assert_eq!(ago(200_000, 80_000), "2 min ago");
        assert_eq!(ago(10_000, 0), "just now");
        assert_eq!(ago(3 * 3600_000, 0), "3 h ago");
    }

    #[test]
    fn test_key_is_stable() {
        assert_eq!(test_data_key().as_bytes(), test_data_key().as_bytes());
    }

    #[test]
    fn test_key_never_replaces_a_real_key() {
        // Security: once real encryption is set up (keys.json exists), an empty or
        // unreadable secret store means Locked, not the public test key.
        let d = tempfile::tempdir().unwrap();
        let mut cfg = AppSyncConfig::new("agenda", d.path(), Arc::new(secrets::MemorySecrets::default()));
        cfg.test_key = true;
        let s = AppSync::new(cfg).unwrap();
        if !release_build() {
            assert!(s.data_key().unwrap().unwrap().1, "test build without keys.json uses the test key");
        }
        s.setup_encryption_with("correct horse battery", KdfParams::insecure_fast()).unwrap();
        let (k, is_test) = s.data_key().unwrap().unwrap();
        assert!(!is_test);
        assert_ne!(k.as_bytes(), test_data_key().as_bytes());
        s.lock().unwrap();
        assert!(s.data_key().unwrap().is_none(), "locked device must not fall back to the public key");
    }

    #[test]
    fn pass_lock_is_exclusive_and_recovers_stale() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.lock");
        let a = PassLock::acquire(p.clone()).unwrap();
        assert!(PassLock::acquire(p.clone()).is_none());
        drop(a);
        // A lock left by a dead process is taken over.
        std::fs::write(&p, "999999999 1").unwrap();
        assert!(PassLock::acquire(p).is_some());
    }
}
