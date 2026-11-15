//! Sync scenarios with simulated devices (one data folder each) and real app
//! record shapes (Agenda tasks, Hisab transactions/loans, Dusk sleep_log).
//! LAN tests run everywhere; server tests need Postgres + S3:
//!   source dev/test.env && cargo test -p selfkit-appsync -- --include-ignored
//! CI: .github/workflows/sync-e2e.yml. Process-level cases (kill -9, clock skew
//! with faketime, real selfkit-agent processes) are in dev/sync-e2e.sh.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use selfkit_appsync::secrets::{self, MemorySecrets, SecretStore};
use selfkit_appsync::*;
use selfkit_core::crypto::SecretKey;
use selfkit_core::lan::{LanConfig, LanState, StoreFactory};
use selfkit_core::sync::s3::S3Config;
use selfkit_core::Store;
use serde_json::{Value, json};

// ---------------------------------------------------------------- helpers

struct Dev {
    sync: AppSync,
    _dir: tempfile::TempDir,
    app: String,
    secrets: Arc<MemorySecrets>,
}

fn b64(k: &SecretKey) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(k.as_bytes())
}

fn dev(app: &str, name: &str, key: &SecretKey, server: Option<&ServerConfig>, mode: TransportMode, peers: &[SocketAddr]) -> Dev {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemorySecrets::default());
    secrets.set(secrets::DATA_KEY, &b64(key)).unwrap();
    let mut cfg = AppSyncConfig::new(app, dir.path(), secrets.clone());
    cfg.device_name = name.into();
    cfg.default_owner = SyncOwner::App;
    cfg.test_key = false;
    let sync = AppSync::new(cfg).unwrap();
    sync.set_server(server).unwrap();
    let mut s = sync.settings();
    s.transport = mode;
    s.lan_discovery_ms = 0; // CI runners: no broadcast; peers by address
    s.enabled = true; // sync is off by default after install
    s.lan_peers = peers.iter().map(|p| p.to_string()).collect();
    sync.set_settings(&s).unwrap();
    Dev { sync, _dir: dir, app: app.into(), secrets }
}

impl Dev {
    fn store(&self) -> Store {
        self.sync.open_stores().unwrap().remove(&self.app).unwrap()
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
    /// Content of every record (no device/time fields), for convergence checks.
    fn dump(&self) -> Vec<(String, Value, bool)> {
        let s = self.store();
        let mut st = s.conn().prepare("SELECT id, data, deleted FROM records ORDER BY id").unwrap();
        st.query_map([], |r| Ok((r.get(0)?, serde_json::from_str(&r.get::<_, String>(1)?).unwrap(), r.get::<_, i64>(2)? != 0)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }
    fn pending(&self) -> i64 {
        self.store().dirty_count().unwrap()
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A desktop agent's LAN service (selfkit-core `lan::serve`, exactly what selfkit-agent runs)
/// over its own data folder with every app DB.
struct Pc {
    dir: tempfile::TempDir,
    addr: SocketAddr,
    id: String,
}

fn pc_store(dir: &std::path::Path, app: &str, id: &str) -> Store {
    Store::open(dir.join(format!("{app}.db")), app, id, &[]).unwrap()
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

/// Fresh Postgres database per test (tests run in parallel) + shared S3 bucket.
async fn server(with_s3: bool) -> ServerConfig {
    let admin = std::env::var("SELFKIT_TEST_PG").expect("SELFKIT_TEST_PG (source dev/test.env)");
    let (c, conn) = tokio_postgres::connect(&admin, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(conn);
    let db = format!("sk_{}", uuid::Uuid::now_v7().simple());
    c.batch_execute(&format!("CREATE DATABASE {db}")).await.unwrap();
    let base = admin.rsplit_once('/').unwrap().0;
    let s3 = with_s3.then(|| {
        let v = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k}"));
        S3Config {
            endpoint: v("SELFKIT_TEST_S3_ENDPOINT"),
            bucket: v("SELFKIT_TEST_S3_BUCKET"),
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

async fn pg(url: &str) -> tokio_postgres::Client {
    let (c, conn) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(conn);
    c
}

fn task(title: &str) -> Value {
    json!({"title": title, "notes": "", "severity": 3, "start": "2026-11-17T18:00", "list": "inbox", "steps": [], "tags": ["home"], "photos": [], "created_at": 1763100000000i64})
}
fn tx(amount: f64, note: &str) -> Value {
    json!({"kind": "expense", "amount": amount, "currency": "BDT", "date": "2026-11-14", "time": "09:15", "account": "acc-cash", "category": "cat-food", "splits": [], "tags": [], "note": note, "photos": [], "source": "manual", "created_at": 1763100000000i64})
}
fn loan(person: &str, amount: f64) -> Value {
    json!({"direction": "lent", "person": person, "title": "Rent share", "description": "", "amount": amount, "currency": "BDT", "given": "2026-11-01", "due": "2026-12-01", "photos": [], "repayments": [], "remind": false})
}
fn night(q: Option<i64>, before: Option<&str>, after: Option<&str>) -> Value {
    json!({"night": "2026-11-15", "kind": "main", "bed": null, "wake": null, "tz_offset": 360, "quality": q, "note_before": before, "note_after": after, "created_at": 1763180000000i64})
}

// ---------------------------------------------------------------- LAN only (no server)

#[tokio::test]
async fn lan_two_phones_through_pc_create_edit_delete_conflict() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("agenda", "OPPO phone", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let b = dev("agenda", "vivo phone", &key, None, TransportMode::LanOnly, &[pc.addr]);

    // Create on each side.
    let t1 = a.put("tasks", None, task("Pay electricity bill"));
    let t2 = b.put("tasks", None, task("Call plumber"));
    let o = a.sync().await;
    assert_eq!(o.transport, Some(Transport::Lan));
    assert_eq!(o.peers, vec!["Linux PC".to_string()]);
    b.sync().await;
    a.sync().await;
    assert!(a.get(&t2).is_some() && b.get(&t1).is_some());
    assert_eq!(pc_store(pc.dir.path(), "agenda", &pc.id).count("tasks").unwrap(), 2);
    let st = a.sync.status().await.unwrap();
    assert!(st.headline.starts_with("Synced locally with Linux PC"), "{}", st.headline);
    assert_eq!(st.peers.len(), 1);

    // Edit on A -> B.
    a.put("tasks", Some(&t1), json!({"title": "Pay electricity + water bill", "severity": 4}));
    a.sync().await;
    b.sync().await;
    assert_eq!(b.get(&t1).unwrap().data["title"], "Pay electricity + water bill");

    // Delete on B, undelete on A.
    b.store().delete(&t2).unwrap();
    b.sync().await;
    a.sync().await;
    assert!(a.get(&t2).unwrap().deleted);
    a.store().restore(&t2).unwrap();
    a.sync().await;
    b.sync().await;
    assert!(!b.get(&t2).unwrap().deleted);

    // Edit-edit conflict: notes changed on both (long text -> line diff), merged field by field.
    a.put("tasks", Some(&t1), json!({"title": "A title", "notes": "Account 2231\nDue 20th, pay online\nBring receipt", "severity": 4}));
    b.put("tasks", Some(&t1), json!({"title": "B title", "notes": "Account 2231\nDue 20th, pay at the counter\nBring receipt\nAsk about meter", "severity": 2}));
    a.sync().await;
    let ob = b.sync().await;
    assert_eq!(ob.report.conflicts, 1);
    let cs = b.sync.conflicts().unwrap();
    assert_eq!(cs.len(), 1);
    let c = &cs[0];
    assert_eq!((c.store.as_str(), c.source.as_str()), ("agenda", "peer"));
    // A = this device (B phone), B = the other version (came from phone A via the PC).
    assert_eq!(c.a.data["title"], "B title");
    assert_eq!(c.b.data["title"], "A title");
    let notes = c.fields.iter().find(|f| f.key == "notes").unwrap();
    let lines = notes.lines.as_ref().expect("long text gets a line diff");
    // Keep the "pay online" line from the other side and the meter line from ours.
    let keep: std::collections::BTreeSet<usize> =
        lines.iter().enumerate().filter(|(_, l)| l.text.contains("online") || l.text.contains("meter")).map(|(i, _)| i).collect();
    let choice = Choice::Merge {
        fields: HashMap::from([("title".into(), "b".into()), ("severity".into(), "a".into())]),
        lines: HashMap::from([("notes".into(), keep)]),
    };
    b.sync.resolve("agenda", &t1, &choice).unwrap();
    b.sync().await;
    a.sync().await;
    for d in [&a, &b] {
        let r = d.get(&t1).unwrap().data;
        assert_eq!(r["title"], "A title");
        assert_eq!(r["severity"], 2);
        assert_eq!(r["notes"], "Account 2231\nDue 20th, pay online\nBring receipt\nAsk about meter");
    }
    assert!(a.sync.conflicts().unwrap().is_empty());
    assert_eq!(a.dump(), b.dump());
}

#[tokio::test]
async fn lan_edit_vs_delete_keep_edit_brings_it_back() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("hisab", "phone", &key, None, TransportMode::LanOnly, &[pc.addr]);
    let t = a.put("transactions", None, tx(1240.0, "Groceries"));
    a.sync().await;
    // PC deletes it (Hisab desktop writes the same DB the agent serves).
    pc_store(pc.dir.path(), "hisab", &pc.id).delete(&t).unwrap();
    a.put("transactions", Some(&t), tx(1180.0, "Groceries + oil"));
    let o = a.sync().await;
    assert_eq!(o.report.conflicts, 1);
    let c = &a.sync.conflicts().unwrap()[0];
    assert!(c.delete_vs_edit && c.b.deleted && !c.a.deleted);
    a.sync.resolve("hisab", &t, &Choice::KeepA).unwrap();
    a.sync().await;
    let r = pc_store(pc.dir.path(), "hisab", &pc.id).get(&t).unwrap().unwrap();
    assert!(!r.deleted, "keeping the edit restores the item on the PC");
    assert!(pc_store(pc.dir.path(), "hisab", &pc.id).conflicts().unwrap().is_empty(), "PC's side of the conflict is settled too");
    assert_eq!(r.data["note"], "Groceries + oil");
}

#[tokio::test]
async fn lan_ten_thousand_records() {
    let key = SecretKey::generate();
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("hisab", "phone", &key, None, TransportMode::LanOnly, &[pc.addr]);
    {
        let s = a.store();
        let txn = s.conn().unchecked_transaction().unwrap();
        for i in 0..10_000 {
            s.put("transactions", None, &tx(i as f64, &format!("bulk {i}"))).unwrap();
        }
        txn.commit().unwrap();
    }
    let o = a.sync().await;
    assert_eq!(o.report.pushed, 10_000);
    assert_eq!(pc_store(pc.dir.path(), "hisab", &pc.id).count("transactions").unwrap(), 10_000);
    // Second pass moves nothing.
    let o = a.sync().await;
    assert_eq!(o.report.pulled + o.report.pushed, 0);
}

#[tokio::test]
async fn lan_wrong_key_is_rejected_and_nothing_moves() {
    let pc = spawn_pc(&SecretKey::generate(), "Linux PC").await;
    let a = dev("agenda", "phone", &SecretKey::generate(), None, TransportMode::LanOnly, &[pc.addr]);
    a.put("tasks", None, task("secret"));
    let o = a.sync.sync_now(Trigger::Manual).await;
    assert!(!o.ok);
    assert!(o.error.as_deref().unwrap().contains("encryption key"), "{o:?}");
    assert_eq!(pc_store(pc.dir.path(), "agenda", &pc.id).count("tasks").unwrap(), 0);
    assert_eq!(a.pending(), 1);
}

#[tokio::test]
async fn lan_protocol_mismatch_is_explained() {
    // A peer that speaks a future protocol version.
    let key = SecretKey::generate();
    let keys = selfkit_core::lan::LanKeys::derive(&key);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        let (s, _) = l.accept().await.unwrap();
        let mut ch = selfkit_core::lan::channel::Channel::server(s, &keys).await.unwrap();
        let _: Value = ch.recv().await.unwrap();
        ch.send(&json!({"t": "hello", "device_id": "future", "name": "New PC", "apps": ["agenda"], "proto": 99})).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let a = dev("agenda", "phone", &key, None, TransportMode::LanOnly, &[addr]);
    let o = a.sync.sync_now(Trigger::Manual).await;
    assert!(!o.ok);
    assert!(o.error.as_deref().unwrap().contains("different Selfkit version"), "{o:?}");
}

#[tokio::test]
async fn nothing_configured_and_owner_rules() {
    let key = SecretKey::generate();
    let dead = SocketAddr::from(([127, 0, 0, 1], free_port()));
    let a = dev("dusk", "phone", &key, None, TransportMode::Auto, &[dead]);
    a.put("sleep_log", Some("n-2026-11-15"), night(Some(4), None, Some("woke once")));
    let o = a.sync.sync_now(Trigger::Manual).await;
    assert!(!o.ok);
    assert!(o.error.as_deref().unwrap().starts_with("Nothing to sync with yet"), "{o:?}");
    let st = a.sync.status().await.unwrap();
    // No server: local changes aren't "waiting for the server", just no connection.
    assert_eq!(st.headline, "Waiting to sync · no connection");

    // Scheduler hook: open only after a minute, background after ~15 min.
    assert!(a.sync.due(Trigger::Manual));
    assert!(!a.sync.due(Trigger::Open));
    assert!(!a.sync.due(Trigger::Background));
    assert_eq!(a.sync.schedule(), Schedule { enabled: true, every_minutes: 15, run_on_open: true });

    // A companion takes over: the app steps back.
    let mut s = a.sync.settings();
    s.owner = SyncOwner::Companion;
    a.sync.set_settings(&s).unwrap();
    let o = a.sync.sync_now(Trigger::Manual).await;
    assert!(!o.ran);
    assert!(!a.sync.schedule().enabled);
    assert_eq!(a.sync.status().await.unwrap().headline, "Synced by the Selfkit companion");
}

#[tokio::test]
async fn test_key_unlocks_without_prompt_and_locked_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(MemorySecrets::default());
    let mut cfg = AppSyncConfig::new("agenda", dir.path(), secrets.clone());
    cfg.test_key = false;
    cfg.default_owner = SyncOwner::App;
    let s = AppSync::new(cfg.clone()).unwrap();
    // Off by default: nothing runs until the user turns sync on.
    let o = s.sync_now(Trigger::Manual).await;
    assert!(!o.ran && o.note.as_deref() == Some("Sync is off. Turn it on in Sync settings."));
    assert!(!s.schedule().enabled);
    let mut st = s.settings();
    st.enabled = true;
    s.set_settings(&st).unwrap();
    assert!(s.data_key().unwrap().is_none());
    let o = s.sync_now(Trigger::Manual).await;
    assert!(o.error.unwrap().starts_with("Sync is locked"));
    cfg.test_key = true;
    let s = AppSync::new(cfg).unwrap();
    let st = s.status().await.unwrap();
    assert!(st.unlocked && st.test_key);
    // Real setup replaces the test key; recovery key unlocks a second device.
    let rk = s.setup_encryption_with("correct horse", selfkit_core::keys::KdfParams::insecure_fast()).unwrap();
    assert!(!s.status().await.unwrap().test_key);
    let k1 = s.data_key().unwrap().unwrap().0;
    let dir2 = tempfile::tempdir().unwrap();
    std::fs::copy(dir.path().join("keys.json"), dir2.path().join("keys.json")).unwrap();
    let s2 = AppSync::new(AppSyncConfig::new("agenda", dir2.path(), Arc::new(MemorySecrets::default()))).unwrap();
    assert!(s2.unlock("wrong").await.is_err());
    s2.unlock_recovery(&rk).await.unwrap();
    assert_eq!(s2.data_key().unwrap().unwrap().0.as_bytes(), k1.as_bytes());
    let _ = secrets;
}

// ---------------------------------------------------------------- server (Postgres + S3)

#[tokio::test]
#[ignore]
async fn server_three_devices_real_records() {
    let key = SecretKey::generate();
    let srv = server(false).await;
    let a = dev("hisab", "OPPO", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let b = dev("hisab", "vivo", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let c = dev("hisab", "Linux PC", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let t = a.put("transactions", None, tx(250.0, "Tea"));
    let l = b.put("loans", None, loan("Karim", 5000.0));
    c.put("transactions", None, tx(90.0, "Bus"));
    for d in [&a, &b, &c, &a, &b] {
        let o = d.sync().await;
        assert_eq!(o.transport, Some(Transport::Server));
    }
    assert_eq!(a.dump(), b.dump());
    assert_eq!(b.dump(), c.dump());
    // Repayment added on C reaches A.
    let mut lv = c.get(&l).unwrap().data;
    lv["repayments"] = json!([{"id": "r1", "date": "2026-11-10", "amount": 1000.0, "note": "first"}]);
    c.put("loans", Some(&l), lv);
    c.sync().await;
    a.sync().await;
    assert_eq!(a.get(&l).unwrap().data["repayments"][0]["amount"], 1000.0);
    // Delete / undelete.
    b.sync().await;
    b.store().delete(&t).unwrap();
    b.sync().await;
    a.sync().await;
    assert!(a.get(&t).unwrap().deleted);
    a.store().restore(&t).unwrap();
    a.sync().await;
    c.sync().await;
    assert!(!c.get(&t).unwrap().deleted);
    let st = a.sync.status().await.unwrap();
    assert!(st.headline.starts_with("Synced via server"), "{}", st.headline);
    assert_eq!(st.pending, 0);
}

#[tokio::test]
#[ignore]
async fn server_edit_edit_and_edit_delete_conflicts_dusk_and_agenda() {
    let key = SecretKey::generate();
    let srv = server(false).await;
    // Dusk: both phones log the same night (deterministic id n-<night>) before syncing.
    let a = dev("dusk", "OPPO", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let b = dev("dusk", "vivo", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    a.put("sleep_log", Some("n-2026-11-15"), night(Some(4), Some("tea late"), None));
    b.put("sleep_log", Some("n-2026-11-15"), night(None, None, Some("woke at 3")));
    a.sync().await;
    assert_eq!(b.sync().await.report.conflicts, 1);
    let c = &b.sync.conflicts().unwrap()[0];
    assert_eq!(c.source, "server");
    let picks = HashMap::from([("quality".to_string(), "b".to_string()), ("note_before".into(), "b".into())]);
    b.sync.resolve("dusk", "n-2026-11-15", &Choice::Merge { fields: picks, lines: HashMap::new() }).unwrap();
    b.sync().await;
    a.sync().await;
    let r = a.get("n-2026-11-15").unwrap().data;
    assert_eq!((r["quality"].clone(), r["note_before"].clone(), r["note_after"].clone()), (json!(4), json!("tea late"), json!("woke at 3")));

    // Agenda: edit on A, delete on B -> keep B (delete wins everywhere).
    let a2 = dev("agenda", "OPPO", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let b2 = dev("agenda", "PC", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let t = a2.put("tasks", None, task("Renew passport"));
    a2.sync().await;
    b2.sync().await;
    a2.put("tasks", Some(&t), json!({"title": "Renew passport (photos!)"}));
    b2.store().delete(&t).unwrap();
    b2.sync().await;
    assert_eq!(a2.sync().await.report.conflicts, 1);
    assert!(a2.sync.conflicts().unwrap()[0].delete_vs_edit);
    a2.sync.resolve("agenda", &t, &Choice::KeepB).unwrap();
    a2.sync().await;
    b2.sync().await;
    assert!(a2.get(&t).unwrap().deleted && b2.get(&t).unwrap().deleted);
    assert_eq!(a2.dump(), b2.dump());
}

#[tokio::test]
#[ignore]
async fn server_offline_queue_then_reconnect() {
    let key = SecretKey::generate();
    let srv = server(false).await;
    let dead = ServerConfig { pg_url: format!("postgres://x:y@127.0.0.1:{}/none?sslmode=disable", free_port()), s3: None };
    let a = dev("agenda", "phone", &key, Some(&dead), TransportMode::Auto, &[]);
    for i in 0..12 {
        a.put("tasks", None, task(&format!("offline {i}")));
    }
    let o = a.sync.sync_now(Trigger::Manual).await;
    assert!(!o.ok);
    assert!(o.error.as_deref().unwrap().starts_with("Can't reach the server"), "{o:?}");
    let st = a.sync.status().await.unwrap();
    assert_eq!(st.pending, 12);
    assert_eq!(st.headline, "Waiting to sync · 12 changes saved here");
    // Back online.
    a.sync.set_server(Some(&srv)).unwrap();
    assert_eq!(a.sync().await.report.pushed, 12);
    let b = dev("agenda", "PC", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    assert_eq!(b.sync().await.report.pulled, 12);
}

#[tokio::test]
#[ignore]
async fn server_switch_transports_midway() {
    // Phone A at home (LAN to the PC), phone B away (server). The PC's agent
    // syncs its DBs with the server (simulated with core sync_once, as the agent does).
    let key = SecretKey::generate();
    let srv = server(false).await;
    let pc = spawn_pc(&key, "Linux PC").await;
    let pc_sync = |k: SecretKey, url: String, dir: std::path::PathBuf, id: String| async move {
        let r = selfkit_core::sync::pg::PgRemote::connect(&url).await.unwrap();
        let mut s = pc_store(&dir, "agenda", &id);
        selfkit_core::sync::sync_once(&mut s, &r, &k).await.unwrap()
    };
    let a = dev("agenda", "OPPO", &key, Some(&srv), TransportMode::Auto, &[pc.addr]);
    let b = dev("agenda", "vivo", &key, Some(&srv), TransportMode::Auto, &[]);
    let t = a.put("tasks", None, task("Buy rice"));
    assert_eq!(a.sync().await.transport, Some(Transport::Lan));
    pc_sync(key.clone(), srv.pg_url.clone(), pc.dir.path().into(), pc.id.clone()).await;
    assert_eq!(b.sync().await.transport, Some(Transport::Server));
    assert!(b.get(&t).is_some());
    // B edits away from home; A comes home (LAN) after the PC pulled it.
    b.put("tasks", Some(&t), json!({"title": "Buy rice 5 kg"}));
    b.sync().await;
    pc_sync(key.clone(), srv.pg_url.clone(), pc.dir.path().into(), pc.id.clone()).await;
    a.sync().await;
    assert_eq!(a.get(&t).unwrap().data["title"], "Buy rice 5 kg");
    // A leaves home: no LAN peer reachable (here: local sync off; dev/sync-e2e.sh E4 kills a
    // real agent instead), Auto falls back to the server. Its LAN-received copy must not
    // conflict with the server copy (history travels with the record).
    let mut s = a.sync.settings();
    s.lan_enabled = false;
    a.sync.set_settings(&s).unwrap();
    let o = a.sync().await;
    assert_eq!((o.transport, o.report.conflicts), (Some(Transport::Server), 0));
    a.put("tasks", Some(&t), json!({"title": "Buy rice 5 kg + lentils"}));
    a.sync().await;
    b.sync().await;
    pc_sync(key.clone(), srv.pg_url.clone(), pc.dir.path().into(), pc.id.clone()).await;
    assert_eq!(b.get(&t).unwrap().data["title"], "Buy rice 5 kg + lentils");
    assert_eq!(pc_store(pc.dir.path(), "agenda", &pc.id).get(&t).unwrap().unwrap().data["title"], "Buy rice 5 kg + lentils");
    assert!(a.sync.conflicts().unwrap().is_empty() && b.sync.conflicts().unwrap().is_empty());
}

#[tokio::test]
#[ignore]
async fn server_ten_thousand_and_interrupted_push_resumes() {
    let key = SecretKey::generate();
    let srv = server(false).await;
    let a = dev("hisab", "phone", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    {
        let s = a.store();
        let txn = s.conn().unchecked_transaction().unwrap();
        for i in 0..10_000 {
            s.put("transactions", None, &tx(i as f64, &format!("bulk {i}"))).unwrap();
        }
        txn.commit().unwrap();
    }
    // Interrupt mid-push (the future is dropped, like the OS killing the job)
    // as soon as the server has some of the rows.
    let c = pg(&srv.pg_url).await;
    let watch = async {
        loop {
            tokio::time::sleep(Duration::from_millis(20)).await;
            if let Ok(r) = c.query_one("SELECT count(*) FROM sk_records", &[]).await
                && r.get::<_, i64>(0) >= 300
            {
                return;
            }
        }
    };
    tokio::select! {
        o = a.sync.sync_now(Trigger::Manual) => panic!("pass finished before it could be interrupted: {o:?}"),
        _ = watch => {}
    }
    let n: i64 = c.query_one("SELECT count(*) FROM sk_records", &[]).await.unwrap().get(0);
    assert!(n > 0 && n < 10_000, "partial push expected, got {n}");
    assert!(a.pending() > 0);
    // Resume: everything arrives exactly once, no conflicts.
    let o = a.sync().await;
    assert_eq!(o.report.conflicts, 0);
    assert_eq!(a.pending(), 0);
    let n: i64 = c.query_one("SELECT count(*) FROM sk_records", &[]).await.unwrap().get(0);
    assert_eq!(n, 10_000);
    let b = dev("hisab", "PC", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    assert_eq!(b.sync().await.report.pulled, 10_000);
    assert_eq!(b.store().count("transactions").unwrap(), 10_000);
}

#[tokio::test]
#[ignore]
async fn server_photos_upload_and_download() {
    let key = SecretKey::generate();
    let srv = server(true).await;
    let pc = spawn_pc(&key, "Linux PC").await;
    let a = dev("agenda", "phone", &key, Some(&srv), TransportMode::Auto, &[pc.addr]);
    let b = dev("agenda", "PC", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    let photo: Vec<u8> = (0..700_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let pdir = a.sync.config().data_dir.join("agenda-photos");
    std::fs::create_dir_all(&pdir).unwrap();
    let pid = a.put("photos", None, json!({"file": "p1.webp", "mime": "image/webp", "w": 2400, "h": 1800, "bytes": photo.len()}));
    std::fs::write(pdir.join("p1.webp"), &photo).unwrap();
    a.put("tasks", None, json!({"title": "Receipt", "photos": [pid]}));
    // Over LAN: records go to the PC, the photo still goes to S3.
    let o = a.sync().await;
    assert_eq!((o.transport, o.report.photos_up), (Some(Transport::Lan), 1));
    // B (away) pulls records from the server... the PC hasn't pushed yet, so nothing yet.
    let o = b.sync().await;
    assert_eq!(o.report.photos_down, 0);
    // A syncs via server too (left home); then B gets record + photo.
    let mut s = a.sync.settings();
    s.transport = TransportMode::ServerOnly;
    a.sync.set_settings(&s).unwrap();
    assert_eq!(a.sync().await.report.photos_up, 0, "already uploaded");
    let o = b.sync().await;
    assert_eq!(o.report.photos_down, 1);
    assert_eq!(std::fs::read(b.sync.config().data_dir.join("agenda-photos/p1.webp")).unwrap(), photo);
    // Nothing readable on S3.
    let bs = selfkit_core::sync::s3::BlobStore::new(srv.s3.clone().unwrap()).unwrap();
    assert!(bs.get_blob(&SecretKey::generate(), &pid).await.is_err());
}

#[tokio::test]
#[ignore]
async fn server_wrong_key_rejected_before_any_push() {
    let srv = server(false).await;
    let a = dev("agenda", "phone", &SecretKey::generate(), Some(&srv), TransportMode::ServerOnly, &[]);
    a.put("tasks", None, task("mine"));
    a.sync().await;
    let intruder = dev("agenda", "old phone", &SecretKey::generate(), Some(&srv), TransportMode::ServerOnly, &[]);
    intruder.put("tasks", None, task("would pollute the server"));
    let o = intruder.sync.sync_now(Trigger::Manual).await;
    assert!(!o.ok);
    assert!(o.error.as_deref().unwrap().contains("encryption key"), "{o:?}");
    let n: i64 = pg(&srv.pg_url).await.query_one("SELECT count(*) FROM sk_records", &[]).await.unwrap().get(0);
    assert_eq!(n, 1, "nothing pushed with the wrong key");
    assert!(intruder.sync.test_server(&srv).await.unwrap_err().contains("encryption key"));
    // Unlocking with the right key fixes it.
    intruder.secrets.set(secrets::DATA_KEY, &a.secrets.get(secrets::DATA_KEY).unwrap().unwrap()).unwrap();
    intruder.sync().await;
}

#[tokio::test]
#[ignore]
async fn server_schema_newer_is_refused() {
    let key = SecretKey::generate();
    let srv = server(false).await;
    let a = dev("agenda", "phone", &key, Some(&srv), TransportMode::ServerOnly, &[]);
    a.sync().await; // creates the schema
    let c = pg(&srv.pg_url).await;
    c.execute("INSERT INTO sk_schema(version, applied_at) VALUES (99, 0)", &[]).await.unwrap();
    a.put("tasks", None, task("x"));
    let o = a.sync.sync_now(Trigger::Manual).await;
    assert!(!o.ok);
    assert!(o.error.as_deref().unwrap().contains("newer Selfkit version"), "{o:?}");
    let n: i64 = c.query_one("SELECT count(*) FROM sk_records", &[]).await.unwrap().get(0);
    assert_eq!(n, 0);
    c.execute("DELETE FROM sk_schema WHERE version=99", &[]).await.unwrap();
    assert_eq!(a.sync().await.report.pushed, 1);
}

// ---------------------------------------------------------------- year-sized data (app seeds)

/// Year of real app data from the apps' own seeds (`hisab seed --year`, `agenda seed --year`,
/// `chrono seed` (default size)) in SELFKIT_YEAR_DIR. Full sync (server and LAN) then an
/// incremental one; prints times and sizes. CI runs it after building the seeds.
#[tokio::test]
#[ignore]
async fn year_data_full_and_incremental_sync() {
    let Ok(src) = std::env::var("SELFKIT_YEAR_DIR") else {
        eprintln!("SELFKIT_YEAR_DIR not set; skipped");
        return;
    };
    let key = SecretKey::generate();
    let srv = server(false).await;
    let pc = spawn_pc(&key, "Linux PC").await;
    let mb = |b: u64| b as f64 / 1_048_576.0;
    for app in ["hisab", "agenda", "chrono"] {
        let db = std::path::Path::new(&src).join(format!("{app}.db"));
        if !db.exists() {
            println!("YEAR {app}: skipped ({} not seeded)", db.display());
            continue;
        }
        let a = dev(app, "OPPO", &key, Some(&srv), TransportMode::ServerOnly, &[]);
        let ad = a.sync.config().data_dir.clone();
        // Copy the seeded DB (with WAL checkpointed) into device A.
        {
            let s = Store::open(&db, app, "seed", &[]).unwrap();
            s.conn().execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
        }
        std::fs::copy(&db, ad.join(format!("{app}.db"))).unwrap();
        let size = std::fs::metadata(ad.join(format!("{app}.db"))).unwrap().len();
        let n: i64 = a.store().conn().query_row("SELECT count(*) FROM records", [], |r| r.get(0)).unwrap();

        let t = std::time::Instant::now();
        let o = a.sync().await;
        let full_push = t.elapsed();
        assert_eq!(o.report.pushed as i64, n);

        let b = dev(app, "PC", &key, Some(&srv), TransportMode::ServerOnly, &[]);
        let t = std::time::Instant::now();
        let o = b.sync().await;
        let full_pull = t.elapsed();
        assert_eq!(o.report.pulled as i64, n);
        assert_eq!(a.dump(), b.dump());

        // LAN full sync of the same year to a PC agent.
        let mut s = a.sync.settings();
        s.transport = TransportMode::LanOnly;
        s.lan_peers = vec![pc.addr.to_string()];
        a.sync.set_settings(&s).unwrap();
        let t = std::time::Instant::now();
        let o = a.sync().await;
        let lan_full = t.elapsed();
        assert_eq!(o.report.pushed as i64, n);

        // Incremental: a day of use (20 new records, 5 edits) on A, then server + LAN.
        let ids: Vec<String> = a.store().conn().prepare("SELECT id FROM records WHERE deleted=0 LIMIT 5").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        for i in 0..20 {
            a.put("transactions", None, tx(i as f64, "today"));
        }
        for id in &ids {
            let mut d = a.get(id).unwrap().data;
            d["edited_today"] = json!(true);
            a.put(&a.get(id).unwrap().collection, Some(id), d);
        }
        let t = std::time::Instant::now();
        let o = a.sync().await; // LAN
        let lan_inc = t.elapsed();
        assert_eq!(o.report.pushed, 25);
        s.transport = TransportMode::ServerOnly;
        a.sync.set_settings(&s).unwrap();
        let t = std::time::Instant::now();
        let o = a.sync().await;
        let inc_push = t.elapsed();
        assert_eq!(o.report.pushed, 25);
        let t = std::time::Instant::now();
        let o = b.sync().await;
        let inc_pull = t.elapsed();
        assert_eq!(o.report.pulled, 25);
        assert_eq!(a.dump(), b.dump());
        println!(
            "YEAR {app}: {n} records, db {:.1} MB | server full push {:.1}s, full pull {:.1}s | LAN full {:.1}s | incremental (25 changes): LAN {:.2}s, push {:.2}s, pull {:.2}s",
            mb(size), full_push.as_secs_f64(), full_pull.as_secs_f64(), lan_full.as_secs_f64(), lan_inc.as_secs_f64(), inc_push.as_secs_f64(), inc_pull.as_secs_f64()
        );
    }
    let srv_bytes: i64 = pg(&srv.pg_url).await.query_one("SELECT pg_total_relation_size('sk_records')", &[]).await.unwrap().get(0);
    println!("YEAR server table sk_records: {:.1} MB (all three apps, encrypted)", mb(srv_bytes as u64));
}
