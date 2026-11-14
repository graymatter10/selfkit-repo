//! Encrypted, mutually-authenticated channel between two of the user's devices.
//!
//! Both devices hold the same data key, so no pairing is needed: possession of
//! the key *is* the authentication. Handshake:
//!   C → S: "SKL1" || client_nonce(32)
//!   S → C: "SKL1" || server_nonce(32)
//!   c2s, s2c = HKDF-SHA256(ikm = lan_key, salt = cn || sn, info = "c2s"/"s2c")
//! Every frame: u32 BE length || XChaCha20-Poly1305(dir key, msg, aad = frame counter).
//! Fresh nonces per session → fresh keys (forward-separated sessions, no replay
//! across sessions); the counter in the AAD blocks replay/reorder/drop within one.
//! A device without the key fails on the very first frame.

use hkdf::Hkdf;
use serde::{Serialize, de::DeserializeOwned};
use sha2::Sha256;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};

use crate::crypto::{self, SecretKey};
use crate::{Error, Result};

const MAGIC: &[u8; 4] = b"SKL1";
const NONCE: usize = 32;
/// Largest accepted frame (a batch of 500 records stays far below this).
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Keys derived from the data key for LAN use only.
#[derive(Clone)]
pub struct LanKeys {
    /// Session key derivation root.
    pub(crate) session_root: SecretKey,
    /// Encrypts discovery beacons.
    pub(crate) beacon: SecretKey,
}

impl LanKeys {
    pub fn derive(data_key: &SecretKey) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(b"selfkit-lan-v1"), data_key.as_bytes());
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        hk.expand(b"session-root", &mut a).expect("len");
        hk.expand(b"beacon", &mut b).expect("len");
        Self { session_root: SecretKey::from_bytes(a), beacon: SecretKey::from_bytes(b) }
    }
}

pub struct Channel<S> {
    r: ReadHalf<S>,
    w: WriteHalf<S>,
    send_key: SecretKey,
    recv_key: SecretKey,
    send_ctr: u64,
    recv_ctr: u64,
}

fn io_err(e: std::io::Error) -> Error {
    Error::Remote(format!("lan io: {e}"))
}

async fn with_timeout<T>(f: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(TIMEOUT, f).await.map_err(|_| Error::Remote("lan timeout".into()))?
}

fn session_keys(root: &SecretKey, cn: &[u8], sn: &[u8]) -> (SecretKey, SecretKey) {
    let mut salt = Vec::with_capacity(2 * NONCE);
    salt.extend_from_slice(cn);
    salt.extend_from_slice(sn);
    let hk = Hkdf::<Sha256>::new(Some(&salt), root.as_bytes());
    let (mut c2s, mut s2c) = ([0u8; 32], [0u8; 32]);
    hk.expand(b"c2s", &mut c2s).expect("len");
    hk.expand(b"s2c", &mut s2c).expect("len");
    (SecretKey::from_bytes(c2s), SecretKey::from_bytes(s2c))
}

impl<S: AsyncRead + AsyncWrite + Unpin + Send> Channel<S> {
    /// Connecting side.
    pub async fn client(mut stream: S, keys: &LanKeys) -> Result<Self> {
        with_timeout(async {
            let cn = crypto::random_bytes::<NONCE>();
            stream.write_all(MAGIC).await.map_err(io_err)?;
            stream.write_all(&cn).await.map_err(io_err)?;
            let mut head = [0u8; 4 + NONCE];
            stream.read_exact(&mut head).await.map_err(io_err)?;
            if &head[..4] != MAGIC {
                return Err(Error::Format("not a selfkit peer".into()));
            }
            let (c2s, s2c) = session_keys(&keys.session_root, &cn, &head[4..]);
            let (r, w) = tokio::io::split(stream);
            Ok(Self { r, w, send_key: c2s, recv_key: s2c, send_ctr: 0, recv_ctr: 0 })
        })
        .await
    }

    /// Accepting side.
    pub async fn server(mut stream: S, keys: &LanKeys) -> Result<Self> {
        with_timeout(async {
            let mut head = [0u8; 4 + NONCE];
            stream.read_exact(&mut head).await.map_err(io_err)?;
            if &head[..4] != MAGIC {
                return Err(Error::Format("not a selfkit peer".into()));
            }
            let sn = crypto::random_bytes::<NONCE>();
            stream.write_all(MAGIC).await.map_err(io_err)?;
            stream.write_all(&sn).await.map_err(io_err)?;
            let (c2s, s2c) = session_keys(&keys.session_root, &head[4..], &sn);
            let (r, w) = tokio::io::split(stream);
            Ok(Self { r, w, send_key: s2c, recv_key: c2s, send_ctr: 0, recv_ctr: 0 })
        })
        .await
    }

    pub async fn send<T: Serialize>(&mut self, msg: &T) -> Result<()> {
        let pt = serde_json::to_vec(msg)?;
        let ct = crypto::seal(&self.send_key, &pt, &self.send_ctr.to_be_bytes());
        self.send_ctr += 1;
        if ct.len() > MAX_FRAME {
            return Err(Error::Format("lan frame too large".into()));
        }
        with_timeout(async {
            self.w.write_all(&(ct.len() as u32).to_be_bytes()).await.map_err(io_err)?;
            self.w.write_all(&ct).await.map_err(io_err)?;
            self.w.flush().await.map_err(io_err)
        })
        .await
    }

    pub async fn recv<T: DeserializeOwned>(&mut self) -> Result<T> {
        let ct = with_timeout(async {
            let mut len = [0u8; 4];
            self.r.read_exact(&mut len).await.map_err(io_err)?;
            let len = u32::from_be_bytes(len) as usize;
            if len > MAX_FRAME {
                return Err(Error::Format("lan frame too large".into()));
            }
            let mut ct = vec![0u8; len];
            self.r.read_exact(&mut ct).await.map_err(io_err)?;
            Ok(ct)
        })
        .await?;
        // Wrong key, tampering, replay or reordering all fail here.
        let pt = crypto::open(&self.recv_key, &ct, &self.recv_ctr.to_be_bytes()).map_err(|_| Error::BadKey)?;
        self.recv_ctr += 1;
        Ok(serde_json::from_slice(&pt)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip_and_wrong_key() {
        let k = LanKeys::derive(&SecretKey::generate());
        let (a, b) = tokio::io::duplex(1 << 16);
        let k2 = k.clone();
        let srv = tokio::spawn(async move {
            let mut ch = Channel::server(b, &k2).await.unwrap();
            let m: String = ch.recv().await.unwrap();
            ch.send(&format!("echo {m}")).await.unwrap();
        });
        let mut ch = Channel::client(a, &k).await.unwrap();
        ch.send(&"hi").await.unwrap();
        assert_eq!(ch.recv::<String>().await.unwrap(), "echo hi");
        srv.await.unwrap();

        // A device with a different data key cannot talk to us.
        let other = LanKeys::derive(&SecretKey::generate());
        let (a, b) = tokio::io::duplex(1 << 16);
        let srv = tokio::spawn(async move {
            let mut ch = Channel::server(b, &k).await.unwrap();
            ch.recv::<String>().await
        });
        let mut ch = Channel::client(a, &other).await.unwrap();
        ch.send(&"intruder").await.unwrap();
        assert!(matches!(srv.await.unwrap(), Err(Error::BadKey)));
    }
}
