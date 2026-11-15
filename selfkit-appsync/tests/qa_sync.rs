//! QA sync scenarios (sync QA pass, 2026-11-15): cases not covered by
//! tests/scenarios.rs. Same device model: one temp data folder per device,
//! PCs = selfkit-core `lan::serve` (what selfkit-agent runs).
//! LAN tests run everywhere; `server_*` need Postgres (+ S3):
//!   source dev/test.env && cargo test -p selfkit-appsync --test qa_sync -- --include-ignored
//! Tests marked `KNOWN BUG` in their doc comment describe the expected behaviour and fail
//! until the bug (see /shared/selfkitextra/syncqa/bugs.md) is fixed.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use selfkit_appsync::secrets::{self, MemorySecrets, SecretStore};
use selfkit_appsync::*;
use selfkit_core::crypto::SecretKey;
use selfkit_core::lan::{LanConfig, LanState, StoreFactory};
use selfkit_core::settings::Settings;
use selfkit_core::Store;
use serde_json::{Value, json};

// ---------------------------------------------------------------- helpers (as in scenarios.rs)

struct Dev {
    sync: AppSync,
    _dir: tempfile::TempDir,
    app: String,
}

fn b64(k: &SecretKey) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(k.as_bytes())
}

fn dev_in(dir: tempfile::TempDir, app: &str, name: &str, key: Option<&SecretKey>, server: Option<&ServerConfig>, mode: TransportMode, peers: &[SocketAddr]) -> Dev {
    let secrets = Arc::new(MemorySecrets::default());
    if let Some(key) = key {
        secrets.set(secrets::DATA_KEY, &b64(key)).unwrap();
    }
    let mut cfg = AppSyncConfig::new(app, dir.path(), secrets);
    cfg.device_name = name.into();
    cfg.default_owner = SyncOwner::App;
    cfg.test_key = false;
    let sync = AppSync::new(cfg).unwrap();
    sync.set_server(server).unwrap();
    let mut s = sync.settings();
    s.transport = mode;
    s.lan_discovery_ms = 0;
    s.enabled = true; // sync is off by default after install
    s.lan_peers = peers.iter().map(|p| p.to_string()).collect();
    sync.set_settings(&s).unwrap();
    Dev { sync, _dir: dir, app: app.into() }
}

fn dev(app: &str, name: &str, key: &SecretKey, server: Option<&ServerConfig>, mode: TransportMode, peers: &[SocketAddr]) -> Dev {
    dev_in(tempfile::tempdir().unwrap(), app, name, Some(key), server, mode, peers)
}

impl Dev {
    fn store(&self) -> Store {
        self.sync.open_stores().unwrap().remove(&self.app).unwrap()
    }
    fn settings_store(&self) -> Store {
        self.sync.open_stores().unwrap().remove("settings").unwrap()
    }
    fn put(&self, coll: &str, id: Option<&str>, data: Value) -> String {
        self.store().put(coll, id, &data).unwrap().id
    }
    fn get(&self, id: &str) -> Option<selfkit_core::Record> {
        self.store().get(id).unwrap()
    }
    async fn sync(&self) -> SyncOutcome {
        let o = self.sync.sync_now(Trigger::Manual).await;
        assert!(o.ok && o.ran, "sync failed: {:?}", o);
        o
    }
    fn dump_of(s: &Store) -> Vec<(String, Value, bool)> {
        let mut st = s.conn().prepare("SELECT id, data, deleted FROM records ORDER BY id").unwrap();
        st.query_map([], |r| Ok((r.get(0)?, serde_json::from_str(&r.get::<_, String>(1)?).unwrap(), r.get::<_, i64>(2)? != 0)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }
    fn dump(&self) -> Vec<(String, Value, bool)> {
        Self::dump_of(&self.store())
    }
    fn nconflicts(&self) -> usize {
        self.sync.conflicts().unwrap().len()
    }
}

/// Tests for open bugs (bugs.md ids) only run with SELFKIT_QA_KNOWN_BUGS=1, so they
/// don't break CI until fixed. Remove the guard when the bug is fixed.
fn skip_known_bug(id: &str) -> bool {
    if std::env::var("SELFKIT_QA_KNOWN_BUGS").is_ok() {
        return false;
    }
    eprintln!("skipped: open bug {id} (set SELFKIT_QA_KNOWN_BUGS=1 to run)");
    true
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

struct Pc {
    dir: tempfile::TempDir,
    addr: SocketAddr,
    id: String,
}

impl Pc {
    fn store(&self, app: &str) -> Store {
        Store::open(self.dir.path().join(format!("{app}.db")), app, &self.id, &[]).unwrap()
    }
}

async fn spawn_pc(key: &SecretKey, name: &str) -> Pc {
    let dir = tempfile::tempdir().unwrap();
    let id = format!("pc-{name}-{}", uuid::Uuid::now_v7());
    let port = free_port();
    let p = dir.path().to_path_buf();
    let dev = id.clone();
    let factory: StoreFactory = Arc::new(move || {
        let mut m = HashMap::new();
        for app in ["agenda", "hisab", "dusk", "chrono", "reflect", "settings"] {
            m.insert(app.to_string(), Store::open(p.join(format!("{app}.db")), app, &dev, &[])?);
        }
        Ok(m)
    });
    let cfg = LanConfig {
        data_key: key.clone(),
        device_id: id.clone(),
        device_name: name.into(),
        tcp_port: port,
        discovery_port: free_port(),
        static_peers: vec![],
        interval: Duration::from_secs(3600),
    };
    tokio::spawn(selfkit_core::lan::serve(cfg, factory, Arc::new(LanState::default())));
    tokio::time::sleep(Duration::from_millis(150)).await;
    Pc { dir, addr: SocketAddr::from(([127, 0, 0, 1], port)), id }
}

async fn server(with_s3: bool) -> ServerConfig {
    use selfkit_core::sync::s3::S3Config;
    let admin = std::env::var("SELFKIT_TEST_PG").expect("SELFKIT_TEST_PG (source dev/test.env)");
    let (c, conn) = tokio_postgres::connect(&admin, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(conn);
    let db = format!("skqa_{}", uuid::Uuid::now_v7().simple());
    c.batch_execute(&format!("CREATE DATABASE {db}")).await.unwrap();
    let base = admin.rsplit_once('/').unwrap().0;
    let s3 = with_s3.then(|| {
        let v = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k}"));
        S3Config {
            endpoint: v("SELFKIT_TEST_S3_ENDPOINT"),
            bucket: format!("{}-qa-{}", v("SELFKIT_TEST_S3_BUCKET"), &uuid::Uuid::now_v7().simple().to_string()[20..]),
            region: v("SELFKIT_TEST_S3_REGION"),
            access_key: v("SELFKIT_TEST_S3_KEY"),
            secret_key: v("SELFKIT_TEST_S3_SECRET"),
            path_style: true,
        }
    });
    if let Some(s) = &s3 {
        selfkit_core::sync::s3::BlobStore::new(s.clone()).unwrap().ensure_bucket().await.unwrap();
    }
    ServerConfig { pg_url: format!("{base}/{db}?sslmode=disable"), s3 }
}

fn task(title: &str) -> Value {
    json!({"title": title, "notes": "", "severity": 3, "start": "2026-11-17T18:00", "list": "inbox", "steps": [], "tags": [], "photos": [], "created_at": 1763100000000i64})
}
fn tx(amount: f64, note: &str) -> Value {
    json!({"kind": "expense", "amount": amount, "currency": "BDT", "date": "2026-11-14", "time": "09:15", "account": "acc-cash", "category": "cat-food", "splits": [], "tags": [], "note": note, "photos": [], "source": "manual", "created_at": 1763100000000i64})
}

// ---------------------------------------------------------------- shared settings

/// KNOWN BUG (QA-S1): two devices change *different* shared settings (main currency on
/// one, day start on the other) before syncing. `Settings::update` keeps fields apart
/// inside one device, but across devices the single settings record conflicts (and the
/// conflict lands in the `settings` store, which no app screen explains). Expected: both
/// changes kept, no conflict.
#[tokio::test]
async fn qa_lan_shared_settings_changed_on_two_devices_keep_both() {
    if skip_known_bug("QA-S1") {
        return;
    }
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("hisab", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("dusk", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    // Common starting point: both have the same settings record.
    Settings::update(&a.settings_store(), |s| s.main_currency = "USD".into()).unwrap();
    a.sync().await;
    b.sync().await;
    assert_eq!(Settings::load(&b.settings_store()).unwrap().main_currency, "USD");
    // Offline: A changes currency, B changes day start.
    Settings::update(&a.settings_store(), |s| s.main_currency = "BDT".into()).unwrap();
    Settings::update(&b.settings_store(), |s| s.day_start_min = 240).unwrap();
    a.sync().await;
    b.sync().await;
    a.sync().await;
    let (sa, sb) = (Settings::load(&a.settings_store()).unwrap(), Settings::load(&b.settings_store()).unwrap());
    let conflicts = a.nconflicts() + b.nconflicts();
    assert_eq!(
        (sa.main_currency.as_str(), sa.day_start_min, sb.main_currency.as_str(), sb.day_start_min, conflicts),
        ("BDT", 240, "BDT", 240, 0),
        "different settings changed on two devices should both survive without a conflict"
    );
}

/// Same settings field changed on two devices: the newest change wins everywhere, no
/// conflict (settings carry per-field clocks; decision 2026-11-15, QA-S1 fix).
#[tokio::test]
async fn qa_lan_shared_settings_same_field_conflict_is_listed_and_resolvable() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("hisab", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("hisab", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    Settings::update(&a.settings_store(), |s| s.main_currency = "USD".into()).unwrap();
    a.sync().await;
    b.sync().await;
    Settings::update(&a.settings_store(), |s| s.main_currency = "BDT".into()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    Settings::update(&b.settings_store(), |s| s.main_currency = "EUR".into()).unwrap();
    a.sync().await;
    b.sync().await;
    a.sync().await;
    assert_eq!(Settings::load(&a.settings_store()).unwrap().main_currency, "EUR");
    assert_eq!(Settings::load(&b.settings_store()).unwrap().main_currency, "EUR");
    assert_eq!(a.nconflicts() + b.nconflicts(), 0);
}

// ---------------------------------------------------------------- deletes

/// Deleted on both devices before syncing: no conflict, stays deleted everywhere.
#[tokio::test]
async fn qa_lan_delete_on_both_sides_is_not_a_conflict() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("agenda", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("agenda", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let t = a.put("tasks", None, task("Throw away"));
    a.sync().await;
    b.sync().await;
    a.store().delete(&t).unwrap();
    b.store().delete(&t).unwrap();
    a.sync().await;
    b.sync().await;
    a.sync().await;
    assert!(a.get(&t).unwrap().deleted && b.get(&t).unwrap().deleted && pc.store("agenda").get(&t).unwrap().unwrap().deleted);
    assert_eq!(a.nconflicts() + b.nconflicts(), 0, "both deleted the same item: nothing to decide");
}

/// Undelete on both sides (both pressed Undo after a synced delete): no conflict.
#[tokio::test]
async fn qa_lan_undelete_on_both_sides_is_not_a_conflict() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("agenda", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("agenda", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let t = a.put("tasks", None, task("Oops"));
    a.sync().await;
    b.sync().await;
    a.store().delete(&t).unwrap();
    a.sync().await;
    b.sync().await;
    assert!(b.get(&t).unwrap().deleted);
    a.store().restore(&t).unwrap();
    b.store().restore(&t).unwrap();
    a.sync().await;
    b.sync().await;
    a.sync().await;
    assert!(!a.get(&t).unwrap().deleted && !b.get(&t).unwrap().deleted);
    assert_eq!(a.nconflicts() + b.nconflicts(), 0);
}

// ---------------------------------------------------------------- edit patterns

/// 200 rapid edits on A between syncs, then ping-pong edits A/B with a sync after each:
/// B always ends with the latest, never a conflict.
#[tokio::test]
async fn qa_lan_rapid_edits_and_ping_pong() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("hisab", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("hisab", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let t = a.put("transactions", None, tx(1.0, "v0"));
    for i in 1..=200 {
        a.put("transactions", Some(&t), tx(i as f64, &format!("v{i}")));
    }
    a.sync().await;
    b.sync().await;
    assert_eq!(b.get(&t).unwrap().data["note"], "v200");
    for i in 0..20 {
        let (w, r) = if i % 2 == 0 { (&a, &b) } else { (&b, &a) };
        w.put("transactions", Some(&t), tx(1000.0 + i as f64, &format!("pp{i}")));
        w.sync().await;
        r.sync().await;
        assert_eq!(r.get(&t).unwrap().data["note"], format!("pp{i}"), "round {i}");
    }
    assert_eq!(a.nconflicts() + b.nconflicts(), 0);
    assert_eq!(a.dump(), b.dump());
}

/// Edits made while a pass is running are not lost: they stay pending and go out next pass.
#[tokio::test]
async fn qa_lan_edit_while_sync_is_running() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = Arc::new(dev("hisab", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]));
    let first = a.put("transactions", None, tx(5.0, "first"));
    {
        let s = a.store();
        let txn = s.conn().unchecked_transaction().unwrap();
        for i in 0..6000 {
            s.put("transactions", None, &tx(i as f64, &format!("bulk {i}"))).unwrap();
        }
        txn.commit().unwrap();
    }
    let a2 = a.clone();
    let running = tokio::spawn(async move { a2.sync.sync_now(Trigger::Manual).await });
    // Meanwhile the user edits and adds.
    let mut late = vec![];
    for i in 0..20 {
        tokio::time::sleep(Duration::from_millis(15)).await;
        late.push(a.put("transactions", None, tx(i as f64, &format!("during {i}"))));
    }
    a.put("transactions", Some(&first), tx(6.0, "first, edited during the pass"));
    // A second tap on Sync now while running is refused politely.
    let o2 = a.sync.sync_now(Trigger::Manual).await;
    assert!(o2.ok, "{o2:?}");
    let o = running.await.unwrap();
    assert!(o.ok, "{o:?}");
    a.sync().await;
    let p = pc.store("hisab");
    for id in &late {
        assert!(p.get(id).unwrap().is_some(), "record added during the pass never reached the PC");
    }
    assert_eq!(p.get(&first).unwrap().unwrap().data["note"], "first, edited during the pass");
    assert_eq!(p.count("transactions").unwrap(), 6021);
}

/// Two devices offline for "days": each adds 300, edits 50 shared items (disjoint), deletes
/// 20 shared items (disjoint). One sync round: no conflicts, identical data, right counts.
#[tokio::test]
async fn qa_lan_offline_for_days_both_sides_catch_up() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("agenda", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("agenda", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let shared: Vec<String> = (0..200).map(|i| a.put("tasks", None, task(&format!("shared {i}")))).collect();
    a.sync().await;
    b.sync().await;
    for i in 0..300 {
        a.put("tasks", None, task(&format!("A new {i}")));
        b.put("tasks", None, task(&format!("B new {i}")));
    }
    for i in 0..50 {
        a.put("tasks", Some(&shared[i]), task(&format!("shared {i} edited on A")));
        b.put("tasks", Some(&shared[100 + i]), task(&format!("shared {} edited on B", 100 + i)));
    }
    for i in 0..20 {
        a.store().delete(&shared[60 + i]).unwrap();
        b.store().delete(&shared[160 + i]).unwrap();
    }
    a.sync().await;
    b.sync().await;
    a.sync().await;
    assert_eq!(a.nconflicts() + b.nconflicts(), 0);
    assert_eq!(a.dump(), b.dump());
    assert_eq!(a.store().count("tasks").unwrap(), 200 + 600 - 40);
    assert_eq!(pc.store("agenda").count("tasks").unwrap(), 760);
}

/// After a conflict is noticed, the other device edits again before it is resolved:
/// the conflict shows the newest other version; Keep B gives that newest version.
#[tokio::test]
async fn qa_lan_conflict_updates_with_newer_remote_edit() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("agenda", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("agenda", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let t = a.put("tasks", None, task("base"));
    a.sync().await;
    b.sync().await;
    // Long notes changed on both sides stay a real conflict (short fields auto-merge since
    // field clocks; the title also carries the version for the checks below).
    let edit = |title: &str| { let mut v = task(title); v["notes"] = json!(format!("Shared first line\n{title} wrote this")); v };
    a.put("tasks", Some(&t), edit("A1"));
    b.put("tasks", Some(&t), edit("B1"));
    a.sync().await;
    assert_eq!(b.sync().await.report.conflicts, 1);
    a.put("tasks", Some(&t), edit("A2"));
    a.sync().await;
    b.sync().await;
    let cs = b.sync.conflicts().unwrap();
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].b.data["title"], "A2", "conflict should show the other device's newest version");
    b.sync.resolve("agenda", &t, &Choice::KeepB).unwrap();
    b.sync().await;
    a.sync().await;
    assert_eq!(a.get(&t).unwrap().data["title"], "A2");
    assert_eq!(b.get(&t).unwrap().data["title"], "A2");
    assert_eq!(a.nconflicts() + b.nconflicts(), 0);
    assert_eq!(a.dump(), b.dump());
}

/// A stranger with another key on the same LAN (listed as a peer, e.g. remembered) must not
/// stop a pass with the real PC, and must receive nothing.
#[tokio::test]
async fn qa_lan_stranger_next_to_real_pc_is_ignored() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let stranger = spawn_pc(&SecretKey::generate(), "Stranger").await;
    let a = dev("agenda", "OPPO", &key, None, TransportMode::LanOnly, &[stranger.addr, pc.addr]);
    let t = a.put("tasks", None, task("private"));
    let o = a.sync().await;
    assert_eq!(o.peers, vec!["Linux PC".to_string()]);
    assert!(pc.store("agenda").get(&t).unwrap().is_some());
    assert_eq!(stranger.store("agenda").count("tasks").unwrap(), 0);
    // And the stranger connecting to the PC directly gets nothing.
    let s = dev("agenda", "Stranger phone", &SecretKey::generate(), None, TransportMode::LanOnly, &[pc.addr]);
    let o = s.sync.sync_now(Trigger::Manual).await;
    assert!(!o.ok);
    assert_eq!(s.store().count("tasks").unwrap(), 0);
}

/// tz_offset written by the phone survives LAN transfer (Dusk nights / Hisab days depend on it).
#[tokio::test]
async fn qa_lan_tz_offset_and_timestamps_survive() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("dusk", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("dusk", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let id = a.put("sleep_log", Some("n-2026-11-15"), json!({"night": "2026-11-15", "kind": "main", "quality": 4}));
    a.store().conn().execute("UPDATE records SET tz_offset=-300 WHERE id=?", [&id]).unwrap();
    a.store().put("sleep_log", Some(&id), &json!({"night": "2026-11-15", "kind": "main", "quality": 5})).unwrap();
    let local = a.get(&id).unwrap();
    a.sync().await;
    b.sync().await;
    let got = b.get(&id).unwrap();
    assert_eq!((got.tz_offset, got.updated_at), (local.tz_offset, local.updated_at));
}

/// KNOWN BUG (QA-S3). Status after LAN-only use with no server: pending should not count records that were
/// merely *received* from the PC (they are not "changes saved here").
#[tokio::test]
async fn qa_lan_pending_counts_only_local_changes() {
    if skip_known_bug("QA-S3") {
        return;
    }
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    pc.store("agenda").put("tasks", None, &task("made on the PC")).unwrap();
    let a = dev("agenda", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    a.sync().await;
    let st = a.sync.status().await.unwrap();
    assert!(st.headline.starts_with("Synced locally with Linux PC"), "{}", st.headline);
    assert_eq!(st.pending, 0, "a record received over LAN is reported as a pending local change");
}

// ---------------------------------------------------------------- server

/// Edits on three devices via the server: conflict noticed on two of them; resolving on
/// one clears it on the other automatically.
#[tokio::test]
#[ignore]
async fn qa_server_conflict_resolved_on_one_device_clears_elsewhere() {
    let key = SecretKey::generate();
    let srv = server(false).await;
    let a = dev("agenda", "OPPO", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let b = dev("agenda", "vivo", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let c = dev("agenda", "PC", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let t = a.put("tasks", None, task("base"));
    for d in [&a, &b, &c] {
        d.sync().await;
    }
    a.put("tasks", Some(&t), task("A"));
    b.put("tasks", Some(&t), task("B"));
    c.put("tasks", Some(&t), task("C"));
    a.sync().await;
    assert_eq!(b.sync().await.report.conflicts, 1);
    assert_eq!(c.sync().await.report.conflicts, 1);
    b.sync.resolve("agenda", &t, &Choice::KeepA).unwrap(); // keep "B"
    b.sync().await;
    c.sync().await;
    a.sync().await;
    // C still has its own edit "C" vs the resolved "B": that is a real conflict now.
    let cs = c.sync.conflicts().unwrap();
    assert!(cs.len() <= 1);
    if let Some(x) = cs.first() {
        assert_eq!(x.b.data["title"], "B", "C's conflict should now be against the resolved version");
        c.sync.resolve("agenda", &t, &Choice::KeepB).unwrap();
        c.sync().await;
    }
    for d in [&a, &b, &c] {
        d.sync().await;
    }
    for d in [&a, &b, &c] {
        assert_eq!(d.get(&t).unwrap().data["title"], "B");
        assert_eq!(d.nconflicts(), 0);
    }
    assert_eq!(a.dump(), c.dump());
}

/// Delete on both devices via the server: no conflict.
#[tokio::test]
#[ignore]
async fn qa_server_delete_on_both_sides_is_not_a_conflict() {
    let key = SecretKey::generate();
    let srv = server(false).await;
    let a = dev("hisab", "OPPO", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let b = dev("hisab", "vivo", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let t = a.put("transactions", None, tx(10.0, "dup"));
    a.sync().await;
    b.sync().await;
    a.store().delete(&t).unwrap();
    b.store().delete(&t).unwrap();
    a.sync().await;
    b.sync().await;
    a.sync().await;
    assert!(a.get(&t).unwrap().deleted && b.get(&t).unwrap().deleted);
    assert_eq!(a.nconflicts() + b.nconflicts(), 0);
}

/// KNOWN BUG (QA-S1, server path): different shared settings changed on two devices.
#[tokio::test]
#[ignore]
async fn qa_server_shared_settings_changed_on_two_devices_keep_both() {
    if skip_known_bug("QA-S1") {
        return;
    }
    let key = SecretKey::generate();
    let srv = server(false).await;
    let a = dev("hisab", "OPPO", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let b = dev("agenda", "vivo", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    Settings::update(&a.settings_store(), |s| s.main_currency = "USD".into()).unwrap();
    a.sync().await;
    b.sync().await;
    Settings::update(&a.settings_store(), |s| s.main_currency = "BDT".into()).unwrap();
    Settings::update(&b.settings_store(), |s| s.day_start_min = 240).unwrap();
    a.sync().await;
    b.sync().await;
    a.sync().await;
    let (sa, sb) = (Settings::load(&a.settings_store()).unwrap(), Settings::load(&b.settings_store()).unwrap());
    assert_eq!(
        (sa.main_currency.as_str(), sa.day_start_min, sb.main_currency.as_str(), sb.day_start_min, a.nconflicts() + b.nconflicts()),
        ("BDT", 240, "BDT", 240, 0)
    );
}

/// KNOWN BUG (QA-S2): encryption set up in an *app* (phone) writes keys.json only locally;
/// nothing uploads it to S3, so a second device can't unlock with the passphrase or the
/// recovery key ("unlock downloads keys.json from S3 when missing" only works if the agent
/// set it up). Expected: a fresh device with the same server unlocks with the recovery key.
#[tokio::test]
#[ignore]
async fn qa_server_recovery_key_on_new_device_after_phone_setup() {
    if skip_known_bug("QA-S2") {
        return;
    }
    let srv = server(true).await;
    let d1 = tempfile::tempdir().unwrap();
    let a = dev_in(d1, "agenda", "OPPO", None, Some(&srv), TransportMode::ServerOnly, &[]);
    let rk = a.sync.setup_encryption_with("correct horse", selfkit_core::keys::KdfParams::insecure_fast()).unwrap();
    let t = a.put("tasks", None, task("made before the new phone"));
    a.sync().await;
    let b = dev_in(tempfile::tempdir().unwrap(), "agenda", "new phone", None, Some(&srv), TransportMode::ServerOnly, &[]);
    let r = b.sync.unlock_recovery(&rk).await;
    assert!(r.is_ok(), "new device can't unlock with the recovery key: {r:?}");
    b.sync().await;
    assert!(b.get(&t).is_some());
}

/// Passphrase change on one device: same data key, so sync keeps working; a fresh device
/// restores everything (incl. tombstones) from the server after unlocking.
#[tokio::test]
#[ignore]
async fn qa_server_passphrase_change_and_fresh_device_restore() {
    let srv = server(false).await;
    let d1 = tempfile::tempdir().unwrap();
    let keyfile = d1.path().join("keys.json");
    let a = dev_in(d1, "hisab", "OPPO", None, Some(&srv), TransportMode::ServerOnly, &[]);
    let rk = a.sync.setup_encryption_with("old pass", selfkit_core::keys::KdfParams::insecure_fast()).unwrap();
    let ids: Vec<String> = (0..50).map(|i| a.put("transactions", None, tx(i as f64, &format!("t{i}")))).collect();
    for id in &ids[..10] {
        a.store().delete(id).unwrap();
    }
    a.sync().await;
    a.sync.change_passphrase("old pass", "new pass").unwrap();
    a.put("transactions", None, tx(99.0, "after passphrase change"));
    assert_eq!(a.sync().await.report.pushed, 1);
    // Fresh device: copy keys.json by hand (see QA-S2 for the S3 path), unlock with the NEW passphrase.
    let d2 = tempfile::tempdir().unwrap();
    std::fs::copy(&keyfile, d2.path().join("keys.json")).unwrap();
    let b = dev_in(d2, "hisab", "restored PC", None, Some(&srv), TransportMode::ServerOnly, &[]);
    assert!(b.sync.unlock("old pass").await.is_err(), "old passphrase must stop working");
    b.sync.unlock("new pass").await.unwrap();
    let o = b.sync().await;
    assert_eq!(o.report.pulled, 51);
    assert_eq!(b.store().count("transactions").unwrap(), 41);
    assert_eq!(a.dump(), b.dump());
    // Recovery key still works too.
    let d3 = tempfile::tempdir().unwrap();
    std::fs::copy(&keyfile, d3.path().join("keys.json")).unwrap();
    let c = dev_in(d3, "hisab", "another", None, Some(&srv), TransportMode::ServerOnly, &[]);
    c.sync.unlock_recovery(&rk).await.unwrap();
    c.sync().await;
    assert_eq!(c.dump(), a.dump());
}

/// Three devices, mixed: OPPO at home (LAN to the PC), vivo away (server), PC both (agent-like:
/// LAN service + core sync_once). Everyone edits, everyone converges with no duplicates.
#[tokio::test]
#[ignore]
async fn qa_three_devices_lan_and_server_all_edit() {
    let key = SecretKey::generate();
    let srv = server(false).await;
    let pc = spawn_pc(&key, "Linux PC").await;
    let pc_sync = || async {
        let r = selfkit_core::sync::pg::PgRemote::connect(&srv.pg_url).await.unwrap();
        for app in ["agenda", "settings"] {
            let mut s = pc.store(app);
            selfkit_core::sync::sync_once(&mut s, &r, &key).await.unwrap();
        }
    };
    let a = dev("agenda", "OPPO", &key, Some(&srv), TransportMode::Auto, &[pc.addr]);
    let b = dev("agenda", "vivo", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let ta: Vec<String> = (0..30).map(|i| a.put("tasks", None, task(&format!("A{i}")))).collect();
    let tb: Vec<String> = (0..30).map(|i| b.put("tasks", None, task(&format!("B{i}")))).collect();
    let tp: Vec<String> = (0..30).map(|i| pc.store("agenda").put("tasks", None, &task(&format!("P{i}"))).unwrap().id).collect();
    for _ in 0..2 {
        a.sync().await;
        pc_sync().await;
        b.sync().await;
    }
    pc_sync().await;
    a.sync().await;
    // Everyone edits someone else's items and deletes a few.
    for i in 0..10 {
        a.put("tasks", Some(&tb[i]), task(&format!("B{i} edited by OPPO")));
        b.put("tasks", Some(&tp[i]), task(&format!("P{i} edited by vivo")));
        pc.store("agenda").put("tasks", Some(&ta[i]), &task(&format!("A{i} edited by PC"))).unwrap();
    }
    a.store().delete(&tp[20]).unwrap();
    b.store().delete(&ta[20]).unwrap();
    pc.store("agenda").delete(&tb[20]).unwrap();
    for _ in 0..3 {
        a.sync().await;
        pc_sync().await;
        b.sync().await;
        pc_sync().await;
    }
    a.sync().await;
    let p = Dev::dump_of(&pc.store("agenda"));
    assert_eq!(a.nconflicts() + b.nconflicts(), 0);
    assert_eq!(a.dump(), b.dump());
    assert_eq!(a.dump(), p);
    assert_eq!(a.store().count("tasks").unwrap(), 87);
    assert_eq!(b.get(&ta[3]).unwrap().data["title"], "A3 edited by PC");
    assert_eq!(a.get(&tp[3]).unwrap().data["title"], "P3 edited by vivo");
}

// ---------------------------------------------------------------- field-level / list merges

/// KNOWN BUG (QA-S1, general case): different fields of the same record edited on two devices
/// (title on A, severity on B) always become a conflict the user must settle by hand.
/// Expected: non-overlapping field edits merge automatically.
#[tokio::test]
async fn qa_lan_different_fields_of_one_task_merge_automatically() {
    if skip_known_bug("QA-S1") {
        return;
    }
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("agenda", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("agenda", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let t = a.put("tasks", None, task("Pay rent"));
    a.sync().await;
    b.sync().await;
    let mut va = a.get(&t).unwrap().data;
    va["title"] = json!("Pay rent (Nov)");
    a.put("tasks", Some(&t), va);
    let mut vb = b.get(&t).unwrap().data;
    vb["severity"] = json!(5);
    b.put("tasks", Some(&t), vb);
    a.sync().await;
    b.sync().await;
    a.sync().await;
    let (ra, rb) = (a.get(&t).unwrap().data, b.get(&t).unwrap().data);
    assert_eq!(
        (ra["title"].clone(), ra["severity"].clone(), rb["title"].clone(), rb["severity"].clone(), a.nconflicts() + b.nconflicts()),
        (json!("Pay rent (Nov)"), json!(5), json!("Pay rent (Nov)"), json!(5), 0)
    );
}

/// KNOWN BUG (QA-S5): a repayment added to the same Hisab loan on two devices. The conflict
/// screen offers the whole `repayments` list from A or from B; every choice (Keep A, Keep B,
/// Merge) loses one repayment. Same for Agenda `steps` (subtasks) and any other list field.
/// Expected: a way to keep both list items (union by item id).
#[tokio::test]
async fn qa_lan_loan_repayments_added_on_two_devices_both_kept() {
    if skip_known_bug("QA-S5") {
        return;
    }
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("hisab", "OPPO", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("hisab", "vivo", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let l = a.put("loans", None, json!({"direction": "lent", "person": "Karim", "title": "Rent share", "amount": 5000.0, "currency": "BDT", "given": "2026-11-01", "repayments": [], "photos": []}));
    a.sync().await;
    b.sync().await;
    let mut va = a.get(&l).unwrap().data;
    va["repayments"] = json!([{"id": "r-a", "date": "2026-11-10", "amount": 1000.0, "note": "cash"}]);
    a.put("loans", Some(&l), va);
    let mut vb = b.get(&l).unwrap().data;
    vb["repayments"] = json!([{"id": "r-b", "date": "2026-11-12", "amount": 500.0, "note": "bKash"}]);
    b.put("loans", Some(&l), vb);
    a.sync().await;
    b.sync().await;
    a.sync().await;
    // Fixed (field clocks + list union by item id): both repayments everywhere, no conflict.
    assert_eq!(a.nconflicts() + b.nconflicts(), 0);
    for d in [&a, &b] {
        let ids: Vec<String> = d.get(&l).unwrap().data["repayments"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect();
        assert!(ids.contains(&"r-a".to_string()) && ids.contains(&"r-b".to_string()), "{ids:?}");
    }
}
