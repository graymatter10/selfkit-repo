//! UDP broadcast discovery. Beacons are encrypted with a key derived from the
//! data key, so only the user's own devices can read (or forge) them; other
//! machines on the network see random bytes.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use serde::{Deserialize, Serialize};

use super::channel::LanKeys;
use crate::{Result, crypto, time};

pub const DISCOVERY_PORT: u16 = 47616;
const MAGIC: &[u8; 4] = b"SKD1";
const AAD: &[u8] = b"selfkit-beacon-v1";
/// Ignore beacons whose clock differs by more than this (stale replays).
const MAX_SKEW_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Beacon {
    pub device_id: String,
    pub name: String,
    pub port: u16,
    pub ts: i64,
}

pub fn encode(keys: &LanKeys, b: &Beacon) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend(crypto::seal(&keys.beacon, &serde_json::to_vec(b).expect("json"), AAD));
    out
}

/// `None` for anything that isn't a fresh beacon from one of our devices.
pub fn decode(keys: &LanKeys, pkt: &[u8]) -> Option<Beacon> {
    let body = pkt.strip_prefix(MAGIC)?;
    let pt = crypto::open(&keys.beacon, body, AAD).ok()?;
    let b: Beacon = serde_json::from_slice(&pt).ok()?;
    ((b.ts - time::now_ms()).abs() <= MAX_SKEW_MS).then_some(b)
}

/// UDP socket on the discovery port, shareable by several local processes.
pub fn bind(port: u16) -> Result<tokio::net::UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    #[cfg(all(unix, not(target_os = "solaris")))]
    s.set_reuse_port(true)?;
    s.set_broadcast(true)?;
    s.set_nonblocking(true)?;
    s.bind(&SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).into())?;
    Ok(tokio::net::UdpSocket::from_std(s.into())?)
}

pub fn broadcast_addr(port: u16) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::SecretKey;

    #[test]
    fn beacons_are_private() {
        let k = LanKeys::derive(&SecretKey::generate());
        let b = Beacon { device_id: "dev-a".into(), name: "rick-laptop".into(), port: 47617, ts: time::now_ms() };
        let pkt = encode(&k, &b);
        assert!(!pkt.windows(4).any(|w| w == b"rick"), "name must not be visible");
        assert_eq!(decode(&k, &pkt), Some(b.clone()));
        assert_eq!(decode(&LanKeys::derive(&SecretKey::generate()), &pkt), None);
        let old = Beacon { ts: 0, ..b };
        assert_eq!(decode(&k, &encode(&k, &old)), None);
    }
}
