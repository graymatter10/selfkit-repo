//! Server-less sync between the user's own devices on the same network
//! (KDE Connect style). Everything on the wire is encrypted: discovery beacons
//! with a beacon key, sessions with per-session keys — both derived from the
//! data key, so only the user's devices can take part. No pairing step.
//! See docs/lan-sync.md.

pub mod channel;
pub mod discovery;
pub mod session;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, mpsc};

pub use channel::LanKeys;
pub use session::{Role, SessionReport};

use crate::crypto::SecretKey;
use crate::{Result, Store, time};

pub const DEFAULT_TCP_PORT: u16 = 47617;

/// Opens this device's app stores for one session (fresh connections each time).
pub type StoreFactory = Arc<dyn Fn() -> Result<HashMap<String, Store>> + Send + Sync>;

pub struct LanConfig {
    pub data_key: SecretKey,
    pub device_id: String,
    pub device_name: String,
    /// 0 = try DEFAULT_TCP_PORT, then any free port.
    pub tcp_port: u16,
    pub discovery_port: u16,
    /// Extra peers by address, for networks where broadcast doesn't reach
    /// (VPNs, emulators, some Wi-Fi). Authenticated like any other peer.
    pub static_peers: Vec<SocketAddr>,
    /// Auto-sync interval with known peers.
    pub interval: Duration,
}

#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    pub device_id: String,
    pub name: String,
    pub addr: SocketAddr,
    pub last_seen: i64,
    pub last_sync: Option<i64>,
    pub last_report: Option<SessionReport>,
    pub last_error: Option<String>,
}

/// Live state for status screens.
#[derive(Default)]
pub struct LanState {
    pub peers: Mutex<HashMap<String, Peer>>,
    pub port: Mutex<Option<u16>>,
    pub sync_now: Notify,
}

impl LanState {
    pub fn snapshot(&self) -> Vec<Peer> {
        let mut v: Vec<Peer> = self.peers.lock().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

/// Peers seen within this window are considered online.
const ONLINE_MS: i64 = 5 * 60 * 1000;
const BEACON_EVERY: Duration = Duration::from_secs(30);

async fn bind_tcp(port: u16) -> Result<TcpListener> {
    if port != 0 {
        return Ok(TcpListener::bind(("0.0.0.0", port)).await?);
    }
    match TcpListener::bind(("0.0.0.0", DEFAULT_TCP_PORT)).await {
        Ok(l) => Ok(l),
        Err(_) => Ok(TcpListener::bind(("0.0.0.0", 0)).await?),
    }
}

struct Ctx {
    cfg: LanConfig,
    keys: LanKeys,
    stores: StoreFactory,
    state: Arc<LanState>,
    busy: Mutex<std::collections::HashSet<String>>,
}

fn record(ctx: &Ctx, key: &str, res: &Result<SessionReport>) {
    let mut peers = ctx.state.peers.lock().unwrap();
    match res {
        Ok(rep) => {
            if let Some(p) = peers.get_mut(&rep.peer_id) {
                p.last_sync = Some(time::now_ms());
                p.last_report = Some(rep.clone());
                p.last_error = None;
            }
            tracing::info!(peer = rep.peer_name, received = rep.received, sent = rep.sent, conflicts = rep.conflicts, "lan sync");
        }
        Err(e) => {
            if let Some(p) = peers.get_mut(key) {
                p.last_error = Some(e.to_string());
            }
            tracing::warn!(peer = key, "lan sync failed: {e}");
        }
    }
}

async fn sync_with(ctx: Arc<Ctx>, key: String, addr: SocketAddr) {
    if !ctx.busy.lock().unwrap().insert(key.clone()) {
        return; // already syncing with this peer
    }
    let res: Result<SessionReport> = async {
        let stream = tokio::time::timeout(channel::TIMEOUT, TcpStream::connect(addr))
            .await
            .map_err(|_| crate::Error::Remote("lan connect timeout".into()))??;
        stream.set_nodelay(true)?;
        let ch = channel::Channel::client(stream, &ctx.keys).await?;
        let mut stores = (ctx.stores)()?;
        session::run(ch, Role::Client, &ctx.cfg.device_id, &ctx.cfg.device_name, &mut stores).await
    }
    .await;
    record(&ctx, &key, &res);
    ctx.busy.lock().unwrap().remove(&key);
}

/// Inbound sessions handled at once. More connections (a flood from a stranger) are
/// closed right away instead of each holding a task, a socket and store handles.
pub const MAX_INBOUND: usize = 8;

async fn accept_loop(ctx: Arc<Ctx>, listener: TcpListener) {
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_INBOUND));
    loop {
        let Ok((stream, addr)) = listener.accept().await else { continue };
        let Ok(permit) = slots.clone().try_acquire_owned() else {
            tracing::debug!(%addr, "lan: too many inbound connections, dropped");
            drop(stream);
            continue;
        };
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let _ = stream.set_nodelay(true);
            let res: Result<SessionReport> = async {
                let ch = channel::Channel::server(stream, &ctx.keys).await?;
                let mut stores = (ctx.stores)()?;
                session::run(ch, Role::Server, &ctx.cfg.device_id, &ctx.cfg.device_name, &mut stores).await
            }
            .await;
            // Wrong-key connections are just dropped; log quietly.
            match &res {
                Err(crate::Error::BadKey) | Err(crate::Error::Format(_)) => {
                    tracing::debug!(%addr, "rejected lan connection: {:?}", res.as_ref().err())
                }
                _ => record(&ctx, &addr.to_string(), &res),
            }
        });
    }
}

/// Beacon replay guard: a beacon is only accepted if it is newer than the last one
/// from the same device. A captured beacon replayed from another machine (to redirect
/// our sessions to the attacker's address) is ignored once the original was seen.
fn fresh_beacon(last_ts: &mut HashMap<String, i64>, b: &discovery::Beacon) -> bool {
    match last_ts.get(&b.device_id) {
        Some(&t) if b.ts <= t => false,
        _ => {
            last_ts.insert(b.device_id.clone(), b.ts);
            true
        }
    }
}

/// Run LAN sync forever: listen, announce, discover, and sync with peers.
pub async fn serve(cfg: LanConfig, stores: StoreFactory, state: Arc<LanState>) -> Result<()> {
    let keys = LanKeys::derive(&cfg.data_key);
    let listener = bind_tcp(cfg.tcp_port).await?;
    let port = listener.local_addr()?.port();
    *state.port.lock().unwrap() = Some(port);
    let udp = Arc::new(discovery::bind(cfg.discovery_port)?);
    tracing::info!(tcp = port, udp = cfg.discovery_port, "lan sync listening");

    let ctx = Arc::new(Ctx { cfg, keys, stores, state: state.clone(), busy: Mutex::new(Default::default()) });
    tokio::spawn(accept_loop(ctx.clone(), listener));

    let beacon = |ctx: &Ctx| {
        discovery::encode(
            &ctx.keys,
            &discovery::Beacon { device_id: ctx.cfg.device_id.clone(), name: ctx.cfg.device_name.clone(), port, ts: time::now_ms() },
        )
    };

    // Receive beacons.
    let (seen_tx, mut seen_rx) = mpsc::channel::<(discovery::Beacon, SocketAddr)>(32);
    {
        let (udp, ctx) = (udp.clone(), ctx.clone());
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                let Ok((n, from)) = udp.recv_from(&mut buf).await else { continue };
                if let Some(b) = discovery::decode(&ctx.keys, &buf[..n])
                    && b.device_id != ctx.cfg.device_id
                {
                    let _ = seen_tx.send((b, from)).await;
                }
            }
        });
    }

    let mut tick = tokio::time::interval(BEACON_EVERY);
    let mut last_auto = tokio::time::Instant::now() - ctx.cfg.interval;
    let mut last_ts: HashMap<String, i64> = HashMap::new();
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let pkt = beacon(&ctx);
                if let Err(e) = udp.send_to(&pkt, discovery::broadcast_addr(ctx.cfg.discovery_port)).await {
                    tracing::debug!("beacon broadcast: {e}");
                }
            }
            Some((b, from)) = seen_rx.recv() => {
                if !fresh_beacon(&mut last_ts, &b) {
                    tracing::debug!(%from, "lan: replayed/stale beacon ignored");
                    continue;
                }
                let addr = SocketAddr::new(from.ip(), b.port);
                let is_new = {
                    let mut peers = state.peers.lock().unwrap();
                    let is_new = !peers.contains_key(&b.device_id)
                        || time::now_ms() - peers[&b.device_id].last_seen > ONLINE_MS;
                    let e = peers.entry(b.device_id.clone()).or_insert_with(|| Peer {
                        device_id: b.device_id.clone(), name: b.name.clone(), addr, last_seen: 0,
                        last_sync: None, last_report: None, last_error: None,
                    });
                    e.name = b.name.clone();
                    e.addr = addr;
                    e.last_seen = time::now_ms();
                    is_new
                };
                if is_new {
                    tracing::info!(peer = b.name, %addr, "lan peer found");
                    // Answer directly so they learn about us without waiting.
                    let _ = udp.send_to(&beacon(&ctx), from).await;
                    // The lower device id starts the session (avoids doubles).
                    if ctx.cfg.device_id < b.device_id {
                        tokio::spawn(sync_with(ctx.clone(), b.device_id.clone(), addr));
                    }
                }
            }
            _ = state.sync_now.notified() => { last_auto = tokio::time::Instant::now() - ctx.cfg.interval; }
        }
        if last_auto.elapsed() >= ctx.cfg.interval {
            last_auto = tokio::time::Instant::now();
            let targets: Vec<(String, SocketAddr)> = {
                let peers = state.peers.lock().unwrap();
                peers.values()
                    .filter(|p| time::now_ms() - p.last_seen <= ONLINE_MS && ctx.cfg.device_id < p.device_id)
                    .map(|p| (p.device_id.clone(), p.addr))
                    .collect()
            };
            for (id, addr) in targets {
                tokio::spawn(sync_with(ctx.clone(), id, addr));
            }
            for addr in ctx.cfg.static_peers.clone() {
                tokio::spawn(sync_with(ctx.clone(), addr.to_string(), addr));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::channel::Channel;
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn stores(dev: &str) -> HashMap<String, Store> {
        ["agenda", "hisab"].iter().map(|a| (a.to_string(), Store::open_in_memory(a, dev, &[]).unwrap())).collect()
    }

    /// Byte pipe that records everything flowing through it (a LAN sniffer).
    fn sniffed_pair() -> (tokio::io::DuplexStream, tokio::io::DuplexStream, Arc<Mutex<Vec<u8>>>) {
        let (a, a_relay) = tokio::io::duplex(1 << 20);
        let (b_relay, b) = tokio::io::duplex(1 << 20);
        let log = Arc::new(Mutex::new(Vec::new()));
        let (mut ar, mut aw) = tokio::io::split(a_relay);
        let (mut br, mut bw) = tokio::io::split(b_relay);
        let l1 = log.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            while let Ok(n) = ar.read(&mut buf).await {
                if n == 0 { break; }
                l1.lock().unwrap().extend_from_slice(&buf[..n]);
                if bw.write_all(&buf[..n]).await.is_err() { break; }
            }
        });
        let l2 = log.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            while let Ok(n) = br.read(&mut buf).await {
                if n == 0 { break; }
                l2.lock().unwrap().extend_from_slice(&buf[..n]);
                if aw.write_all(&buf[..n]).await.is_err() { break; }
            }
        });
        (a, b, log)
    }

    async fn sync_pair(keys: &LanKeys, a: &mut HashMap<String, Store>, b: &mut HashMap<String, Store>) -> (SessionReport, SessionReport, Vec<u8>) {
        let (sa, sb, log) = sniffed_pair();
        let kb = keys.clone();
        let mut b_owned = std::mem::take(b);
        let srv = tokio::spawn(async move {
            let ch = Channel::server(sb, &kb).await.unwrap();
            let r = session::run(ch, Role::Server, "B", "desk", &mut b_owned).await.unwrap();
            (r, b_owned)
        });
        let ch = Channel::client(sa, keys).await.unwrap();
        let ra = session::run(ch, Role::Client, "A", "phone", a).await.unwrap();
        let (rb, b_back) = srv.await.unwrap();
        *b = b_back;
        let bytes = log.lock().unwrap().clone();
        (ra, rb, bytes)
    }

    #[tokio::test]
    async fn two_way_lan_sync_is_encrypted_and_incremental() {
        let keys = LanKeys::derive(&SecretKey::generate());
        let mut a = stores("A");
        let mut b = stores("B");
        for i in 0..1200 {
            a["agenda"].put("tasks", None, &json!({"title": format!("SECRET-TASK-{i}")})).unwrap();
        }
        let t = b["hisab"].put("transactions", None, &json!({"note": "SECRET-COFFEE", "amount": 3})).unwrap();

        let (ra, rb, wire) = sync_pair(&keys, &mut a, &mut b).await;
        assert_eq!((ra.received, rb.received), (1, 1200), "multi-page pull both ways");
        assert_eq!(b["agenda"].count("tasks").unwrap(), 1200);
        assert_eq!(a["hisab"].get(&t.id).unwrap().unwrap().data["note"], "SECRET-COFFEE");
        // Nothing readable on the wire.
        let s = String::from_utf8_lossy(&wire);
        assert!(!s.contains("SECRET") && !s.contains("phone") && !s.contains("tasks"), "plaintext leaked to the network");
        assert!(wire.len() > 100_000);

        // Second session only moves what changed; records each side got from the other
        // are not sent back (S-4).
        b["agenda"].put("tasks", None, &json!({"title": "new"})).unwrap();
        let (ra, rb, _) = sync_pair(&keys, &mut a, &mut b).await;
        assert_eq!(ra.received, 1);
        assert_eq!(rb.received, 0);
        assert_eq!((ra.sent, rb.sent), (0, 1), "no echo of the peer's own records");

        // Concurrent edit of long text -> conflict on both, nothing lost (short fields auto-merge).
        let id = t.id.clone();
        std::thread::sleep(std::time::Duration::from_millis(20)); // both edits newer than the shared version
        a["hisab"].put("transactions", Some(&id), &json!({"note": "n\nfour"})).unwrap();
        b["hisab"].put("transactions", Some(&id), &json!({"note": "n\nfive"})).unwrap();
        let (ra, rb, _) = sync_pair(&keys, &mut a, &mut b).await;
        assert_eq!((ra.conflicts, rb.conflicts), (1, 1));

        // C85: A loses its database (same device id); a fresh pull from B restores A's own
        // records, which the S-4 echo filter must not hold back.
        let mut a_fresh = stores("A");
        let (ra2, _, _) = sync_pair(&keys, &mut a_fresh, &mut b).await;
        assert_eq!(a_fresh["agenda"].count("tasks").unwrap(), 1201, "own tasks come back ({ra2:?})");
    }

    #[tokio::test]
    async fn service_discovers_and_syncs_over_real_sockets() {
        // Two services on localhost; B is reached as a static peer (broadcast
        // loopback isn't guaranteed in CI), discovery port is random per test.
        let data_key = SecretKey::generate();
        let state_a = Arc::new(LanState::default());
        let state_b = Arc::new(LanState::default());
        let path = tempfile::tempdir().unwrap();
        let (pa, pb) = (path.path().join("a.db"), path.path().join("b.db"));
        Store::open(&pa, "agenda", "dev-a", &[]).unwrap().put("tasks", None, &json!({"t": "from A"})).unwrap();
        Store::open(&pb, "agenda", "dev-b", &[]).unwrap().put("tasks", None, &json!({"t": "from B"})).unwrap();
        let fa: StoreFactory = { let p = pa.clone(); Arc::new(move || Ok(HashMap::from([("agenda".into(), Store::open(&p, "agenda", "dev-a", &[])?)]))) };
        let fb: StoreFactory = { let p = pb.clone(); Arc::new(move || Ok(HashMap::from([("agenda".into(), Store::open(&p, "agenda", "dev-b", &[])?)]))) };
        let free = |_: ()| std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let (tcp_b, udp_a, udp_b) = (free(()), free(()), free(()));
        let mk = |id: &str, tcp: u16, udp: u16, peers: Vec<SocketAddr>| LanConfig {
            data_key: data_key.clone(), device_id: id.into(), device_name: id.into(), tcp_port: tcp, discovery_port: udp,
            static_peers: peers, interval: Duration::from_secs(3600),
        };
        tokio::spawn(serve(mk("dev-b", tcp_b, udp_b, vec![]), fb, state_b.clone()));
        tokio::time::sleep(Duration::from_millis(200)).await;
        tokio::spawn(serve(mk("dev-a", 0, udp_a, vec![SocketAddr::from(([127, 0, 0, 1], tcp_b))]), fa, state_a.clone()));
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if Store::open(&pa, "agenda", "dev-a", &[]).unwrap().count("tasks").unwrap() == 2
                && Store::open(&pb, "agenda", "dev-b", &[]).unwrap().count("tasks").unwrap() == 2 { return; }
        }
        panic!("services did not sync");
    }

    /// Attack: a device on the same Wi-Fi without the data key (or with another one)
    /// connects to a real service and tries to push a record and pull data. It must
    /// get nothing and change nothing.
    #[tokio::test]
    async fn rogue_peer_with_wrong_key_gets_nothing_and_changes_nothing() {
        let data_key = SecretKey::generate();
        let dir = tempfile::tempdir().unwrap();
        let pb = dir.path().join("b.db");
        Store::open(&pb, "agenda", "dev-b", &[]).unwrap().put("tasks", None, &json!({"t": "SECRET"})).unwrap();
        let fb: StoreFactory = { let p = pb.clone(); Arc::new(move || Ok(HashMap::from([("agenda".into(), Store::open(&p, "agenda", "dev-b", &[])?)]))) };
        let free = || std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let (tcp_b, udp_b) = (free(), free());
        let cfg = LanConfig {
            data_key, device_id: "dev-b".into(), device_name: "desk".into(), tcp_port: tcp_b, discovery_port: udp_b,
            static_peers: vec![], interval: Duration::from_secs(3600),
        };
        tokio::spawn(serve(cfg, fb, Arc::new(LanState::default())));
        tokio::time::sleep(Duration::from_millis(200)).await;

        for rogue_key in [SecretKey::generate(), SecretKey::from_bytes([0u8; 32])] {
            let mut rogue = HashMap::from([("agenda".to_string(), Store::open_in_memory("agenda", "dev-0", &[]).unwrap())]);
            rogue["agenda"].put("tasks", None, &json!({"t": "INJECTED"})).unwrap();
            let stream = TcpStream::connect(("127.0.0.1", tcp_b)).await.unwrap();
            let ch = Channel::client(stream, &LanKeys::derive(&rogue_key)).await.unwrap();
            let r = session::run(ch, Role::Client, "dev-0", "rogue", &mut rogue).await;
            assert!(r.is_err(), "rogue session must fail");
            assert_eq!(rogue["agenda"].count("tasks").unwrap(), 1, "rogue received data");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let b = Store::open(&pb, "agenda", "dev-b", &[]).unwrap();
        assert_eq!(b.count("tasks").unwrap(), 1, "rogue changed our data");
        assert!(b.search(None, "INJECTED", 5).unwrap().is_empty());
    }

    /// Attack: a flood of idle connections must not lock out the real devices forever;
    /// extra connections are closed at once (MAX_INBOUND), idle ones time out.
    #[tokio::test]
    async fn connection_flood_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let pb = dir.path().join("b.db");
        Store::open(&pb, "agenda", "dev-b", &[]).unwrap();
        let fb: StoreFactory = { let p = pb.clone(); Arc::new(move || Ok(HashMap::from([("agenda".into(), Store::open(&p, "agenda", "dev-b", &[])?)]))) };
        let free = || std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let (tcp_b, udp_b) = (free(), free());
        let cfg = LanConfig {
            data_key: SecretKey::generate(), device_id: "dev-b".into(), device_name: "desk".into(), tcp_port: tcp_b, discovery_port: udp_b,
            static_peers: vec![], interval: Duration::from_secs(3600),
        };
        tokio::spawn(serve(cfg, fb, Arc::new(LanState::default())));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let mut idle = Vec::new();
        for _ in 0..MAX_INBOUND {
            idle.push(TcpStream::connect(("127.0.0.1", tcp_b)).await.unwrap());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        // One more: closed by the service right away (read returns EOF/reset, not a hang).
        let mut extra = TcpStream::connect(("127.0.0.1", tcp_b)).await.unwrap();
        let mut buf = [0u8; 1];
        let r = tokio::time::timeout(Duration::from_secs(2), extra.read(&mut buf)).await.expect("extra connection must be closed");
        assert!(matches!(r, Ok(0) | Err(_)));
    }

    #[test]
    fn replayed_beacons_are_ignored() {
        let mut last = HashMap::new();
        let b = discovery::Beacon { device_id: "dev-a".into(), name: "a".into(), port: 1, ts: 1000 };
        assert!(fresh_beacon(&mut last, &b));
        assert!(!fresh_beacon(&mut last, &b), "same beacon again (replay)");
        assert!(!fresh_beacon(&mut last, &discovery::Beacon { ts: 999, ..b.clone() }), "older beacon");
        assert!(fresh_beacon(&mut last, &discovery::Beacon { ts: 31_000, ..b.clone() }));
        assert!(fresh_beacon(&mut last, &discovery::Beacon { device_id: "dev-c".into(), ..b }));
    }
}
