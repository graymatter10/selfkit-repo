//! One-shot LAN sync for apps (no always-on service): announce ourselves, listen
//! briefly for beacons and inbound sessions, then connect to every reachable
//! peer (discovered, remembered or configured) and run one encrypted session
//! each. Wire format, keys and merge rules are selfkit-core's (`lan::*`), so the
//! peer is usually the desktop agent's `lan::serve`.
//! Android: hold a `WifiManager.MulticastLock` around the pass (Kotlin plugin).

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use selfkit_core::crypto::SecretKey;
use selfkit_core::lan::{LanKeys, Role, SessionReport, channel::Channel, discovery, session};
use selfkit_core::{Error, Result, Store, time};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

pub struct LanPassConfig {
    pub device_id: String,
    pub device_name: String,
    /// Addresses to try directly (remembered peers + settings).
    pub candidates: Vec<SocketAddr>,
    /// Listen for beacons on this port (None = no discovery).
    pub discovery_port: Option<u16>,
    pub discovery_wait: Duration,
    pub connect_timeout: Duration,
}

#[derive(Debug, Default)]
pub struct LanPassResult {
    pub sessions: Vec<(SocketAddr, SessionReport)>,
    /// Peers heard from (beacons or sessions): id -> (name, addr).
    pub seen: HashMap<String, (String, SocketAddr)>,
    /// Last error per address (connect refused, wrong key, protocol).
    pub errors: Vec<(String, Error)>,
}

impl LanPassResult {
    pub fn synced(&self) -> bool {
        !self.sessions.is_empty()
    }
}

async fn serve_inbound(stream: TcpStream, keys: &LanKeys, cfg: &LanPassConfig, stores: &mut HashMap<String, Store>) -> Result<SessionReport> {
    stream.set_nodelay(true)?;
    let ch = Channel::server(stream, keys).await?;
    session::run(ch, Role::Server, &cfg.device_id, &cfg.device_name, stores).await
}

async fn connect_out(addr: SocketAddr, keys: &LanKeys, cfg: &LanPassConfig, stores: &mut HashMap<String, Store>) -> Result<SessionReport> {
    let stream = tokio::time::timeout(cfg.connect_timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| Error::Remote(format!("{}: lan connect to {addr}", crate::explain::TIMEOUT)))??;
    stream.set_nodelay(true)?;
    let ch = Channel::client(stream, keys).await?;
    session::run(ch, Role::Client, &cfg.device_id, &cfg.device_name, stores).await
}

pub async fn pass(key: &SecretKey, cfg: &LanPassConfig, stores: &mut HashMap<String, Store>) -> LanPassResult {
    let keys = LanKeys::derive(key);
    let mut res = LanPassResult::default();
    let mut done_ids: HashSet<String> = HashSet::new();
    let mut discovered: Vec<(String, SocketAddr)> = vec![];

    if let Some(dport) = cfg.discovery_port {
        // Inbound listener: a peer with a lower device id connects to us when it hears our beacon.
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await.ok();
        let my_port = listener.as_ref().and_then(|l| l.local_addr().ok()).map(|a| a.port()).unwrap_or(0);
        // Ephemeral socket: our broadcast goes out from it and direct replies come back to it.
        let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await.ok();
        // Shared discovery port: periodic beacons (SO_REUSEPORT, so the agent on this machine keeps working).
        let shared = discovery::bind(dport).ok();
        if let Some(p) = &probe {
            let _ = p.set_broadcast(true);
        }
        // Beacon now and every 700 ms while we listen: a peer whose own short pass starts a
        // moment later still hears us while our listener is open (S-6: two "Sync now" passes
        // 1-2 s apart never met with a single beacon at the start).
        let beacon = || async {
            if let Some(p) = &probe {
                let pkt = discovery::encode(
                    &keys,
                    &discovery::Beacon { device_id: cfg.device_id.clone(), name: cfg.device_name.clone(), port: my_port, ts: time::now_ms() },
                );
                if let Err(e) = p.send_to(&pkt, discovery::broadcast_addr(dport)).await {
                    tracing::debug!("lan beacon broadcast: {e}");
                }
            }
        };
        let mut tick = tokio::time::interval(Duration::from_millis(700));
        let deadline = tokio::time::Instant::now() + cfg.discovery_wait;
        let (mut b1, mut b2) = ([0u8; 2048], [0u8; 2048]);
        loop {
            let recv_probe = async {
                match &probe {
                    Some(s) => s.recv_from(&mut b1).await.map(|(n, a)| (b1[..n].to_vec(), a)),
                    None => std::future::pending().await,
                }
            };
            let recv_shared = async {
                match &shared {
                    Some(s) => s.recv_from(&mut b2).await.map(|(n, a)| (b2[..n].to_vec(), a)),
                    None => std::future::pending().await,
                }
            };
            let accept = async {
                match &listener {
                    Some(l) => l.accept().await,
                    None => std::future::pending().await,
                }
            };
            let pkt = tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                _ = tick.tick() => { beacon().await; None }
                r = recv_probe => r.ok(),
                r = recv_shared => r.ok(),
                r = accept => {
                    if let Ok((stream, addr)) = r {
                        match serve_inbound(stream, &keys, cfg, stores).await {
                            Ok(rep) => {
                                done_ids.insert(rep.peer_id.clone());
                                res.seen.insert(rep.peer_id.clone(), (rep.peer_name.clone(), addr));
                                res.sessions.push((addr, rep));
                            }
                            Err(e) => res.errors.push((addr.to_string(), e)),
                        }
                    }
                    None
                }
            };
            if let Some((data, from)) = pkt
                && let Some(b) = discovery::decode(&keys, &data)
                && b.device_id != cfg.device_id
                && b.port != 0
            {
                let addr = SocketAddr::new(from.ip(), b.port);
                if !discovered.iter().any(|(id, _)| *id == b.device_id) {
                    discovered.push((b.device_id.clone(), addr));
                    res.seen.insert(b.device_id.clone(), (b.name.clone(), addr));
                    // Its listener is open right now (its pass may end before ours): connect
                    // at once instead of after our window (S-6).
                    if !done_ids.contains(&b.device_id) {
                        match connect_out(addr, &keys, cfg, stores).await {
                            Ok(rep) => {
                                done_ids.insert(rep.peer_id.clone());
                                res.sessions.push((addr, rep));
                            }
                            Err(e) => res.errors.push((addr.to_string(), e)),
                        }
                    }
                }
            }
        }
    }

    // Discovered first (fresh addresses), then remembered/configured ones.
    let mut tried: HashSet<SocketAddr> = HashSet::new();
    for (id, addr) in discovered {
        if done_ids.contains(&id) || !tried.insert(addr) {
            continue;
        }
        match connect_out(addr, &keys, cfg, stores).await {
            Ok(rep) => {
                done_ids.insert(rep.peer_id.clone());
                res.sessions.push((addr, rep));
            }
            Err(e) => res.errors.push((addr.to_string(), e)),
        }
    }
    for addr in cfg.candidates.iter().copied() {
        if !tried.insert(addr) {
            continue;
        }
        match connect_out(addr, &keys, cfg, stores).await {
            Ok(rep) => {
                res.seen.insert(rep.peer_id.clone(), (rep.peer_name.clone(), addr));
                // A remembered address that turned out to be a peer we already synced: fine, it was a no-op.
                done_ids.insert(rep.peer_id.clone());
                res.sessions.push((addr, rep));
            }
            Err(e) => res.errors.push((addr.to_string(), e)),
        }
    }
    res
}
