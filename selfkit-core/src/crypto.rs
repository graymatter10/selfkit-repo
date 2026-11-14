//! XChaCha20-Poly1305 for records (`seal`/`open`) and a chunked STREAM
//! construction for files and bundles (`StreamWriter`/`StreamReader`).
//! See docs/security.md.

use std::io::{self, BufRead, BufReader, Read, Write};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{
        Aead, KeyInit, Payload,
        stream::{DecryptorBE32, EncryptorBE32},
    },
};
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{Error, Result};

pub const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
/// Plaintext bytes per STREAM chunk. 64 KiB keeps RAM flat on weak devices.
pub const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;
/// STREAM BE32 nonce prefix for XChaCha (24 - 5 bytes).
const STREAM_NONCE_LEN: usize = 19;
const STREAM_MAGIC: &[u8; 4] = b"SKS1";

/// 256-bit secret key, wiped from memory on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct SecretKey([u8; KEY_LEN]);

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(..)")
    }
}

impl SecretKey {
    pub fn generate() -> Self {
        let mut k = [0u8; KEY_LEN];
        rand::rngs::OsRng.fill_bytes(&mut k);
        Self(k)
    }
    pub fn from_bytes(b: [u8; KEY_LEN]) -> Self {
        Self(b)
    }
    pub fn from_slice(b: &[u8]) -> Result<Self> {
        let arr: [u8; KEY_LEN] = b.try_into().map_err(|_| Error::Format("key length".into()))?;
        Ok(Self(arr))
    }
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new((&self.0).into())
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b
}

/// Encrypt a small message. Output: `nonce(24) || ciphertext+tag`.
/// `aad` is authenticated but not encrypted (binds ciphertext to e.g. record id).
pub fn seal(key: &SecretKey, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    let nonce = random_bytes::<NONCE_LEN>();
    let ct = key
        .cipher()
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad })
        .expect("xchacha encrypt cannot fail");
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

pub fn open(key: &SecretKey, data: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LEN + TAG {
        return Err(Error::Decrypt);
    }
    let (nonce, ct) = data.split_at(NONCE_LEN);
    key.cipher()
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad })
        .map_err(|_| Error::Decrypt)
}

/// Streaming encryptor: `SKS1 || nonce(19) || chunk*` where every chunk but the
/// last holds exactly CHUNK plaintext bytes. Call `finish()` to write the last chunk.
pub struct StreamWriter<W: Write> {
    inner: Option<W>,
    enc: Option<EncryptorBE32<XChaCha20Poly1305>>,
    buf: Vec<u8>,
}

impl<W: Write> StreamWriter<W> {
    pub fn new(key: &SecretKey, mut inner: W) -> io::Result<Self> {
        let nonce = random_bytes::<STREAM_NONCE_LEN>();
        inner.write_all(STREAM_MAGIC)?;
        inner.write_all(&nonce)?;
        let enc = EncryptorBE32::from_aead(key.cipher(), nonce.as_slice().into());
        Ok(Self { inner: Some(inner), enc: Some(enc), buf: Vec::with_capacity(CHUNK * 2) })
    }

    fn flush_full_chunks(&mut self) -> io::Result<()> {
        // Keep at least one byte back so the final chunk is emitted by finish().
        while self.buf.len() > CHUNK {
            let rest = self.buf.split_off(CHUNK);
            let ct = self
                .enc
                .as_mut()
                .expect("not finished")
                .encrypt_next(self.buf.as_slice())
                .map_err(|_| io::Error::other("encrypt"))?;
            self.inner.as_mut().expect("not finished").write_all(&ct)?;
            self.buf = rest;
        }
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<W> {
        let enc = self.enc.take().expect("finish once");
        let ct = enc.encrypt_last(self.buf.as_slice()).map_err(|_| io::Error::other("encrypt"))?;
        let mut inner = self.inner.take().expect("finish once");
        inner.write_all(&ct)?;
        self.buf.zeroize();
        inner.flush()?;
        Ok(inner)
    }
}

impl<W: Write> Write for StreamWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        self.flush_full_chunks()?;
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        match self.inner.as_mut() {
            Some(w) => w.flush(),
            None => Ok(()),
        }
    }
}

/// Streaming decryptor for `StreamWriter` output. Fails on tampering or truncation.
pub struct StreamReader<R: Read> {
    inner: BufReader<R>,
    dec: Option<DecryptorBE32<XChaCha20Poly1305>>,
    out: Vec<u8>,
    pos: usize,
}

impl<R: Read> StreamReader<R> {
    pub fn new(key: &SecretKey, inner: R) -> Result<Self> {
        let mut inner = BufReader::with_capacity(CHUNK + TAG, inner);
        let mut head = [0u8; 4 + STREAM_NONCE_LEN];
        inner.read_exact(&mut head).map_err(|_| Error::Format("stream header".into()))?;
        if &head[..4] != STREAM_MAGIC {
            return Err(Error::Format("not a selfkit stream".into()));
        }
        let dec = DecryptorBE32::from_aead(key.cipher(), head[4..].into());
        Ok(Self { inner, dec: Some(dec), out: Vec::new(), pos: 0 })
    }

    fn next_chunk(&mut self) -> io::Result<()> {
        let Some(dec) = self.dec.as_mut() else {
            self.out.clear();
            self.pos = 0;
            return Ok(());
        };
        let mut chunk = vec![0u8; CHUNK + TAG];
        let mut n = 0;
        while n < chunk.len() {
            let r = self.inner.read(&mut chunk[n..])?;
            if r == 0 {
                break;
            }
            n += r;
        }
        chunk.truncate(n);
        let last = n < CHUNK + TAG || self.inner.fill_buf()?.is_empty();
        let pt = if last {
            let dec = self.dec.take().unwrap();
            dec.decrypt_last(chunk.as_slice())
        } else {
            dec.decrypt_next(chunk.as_slice())
        }
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "decryption failed"))?;
        self.out.zeroize();
        self.out = pt;
        self.pos = 0;
        Ok(())
    }
}

impl<R: Read> Read for StreamReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.pos >= self.out.len() {
            if self.dec.is_none() {
                return Ok(0);
            }
            self.next_chunk()?;
        }
        let n = buf.len().min(self.out.len() - self.pos);
        buf[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Encrypt everything from `r` into `w`.
pub fn encrypt_stream<R: Read, W: Write>(key: &SecretKey, mut r: R, w: W) -> io::Result<W> {
    let mut sw = StreamWriter::new(key, w)?;
    io::copy(&mut r, &mut sw)?;
    sw.finish()
}

/// Decrypt everything from `r` into `w`.
pub fn decrypt_stream<R: Read, W: Write>(key: &SecretKey, r: R, mut w: W) -> Result<W> {
    let mut sr = StreamReader::new(key, r)?;
    io::copy(&mut sr, &mut w).map_err(|e| {
        if e.kind() == io::ErrorKind::InvalidData { Error::Decrypt } else { Error::Io(e) }
    })?;
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_roundtrip_and_aad_binding() {
        let k = SecretKey::generate();
        let ct = seal(&k, b"hello", b"id1");
        assert_eq!(open(&k, &ct, b"id1").unwrap(), b"hello");
        assert!(open(&k, &ct, b"id2").is_err());
        assert!(open(&SecretKey::generate(), &ct, b"id1").is_err());
    }

    #[test]
    fn stream_roundtrip_sizes() {
        let k = SecretKey::generate();
        for size in [0, 1, CHUNK - 1, CHUNK, CHUNK + 1, 3 * CHUNK, 3 * CHUNK + 17] {
            let data: Vec<u8> = (0..size).map(|i| (i * 31 % 251) as u8).collect();
            let ct = encrypt_stream(&k, data.as_slice(), Vec::new()).unwrap();
            let pt = decrypt_stream(&k, ct.as_slice(), Vec::new()).unwrap();
            assert_eq!(pt, data, "size {size}");
        }
    }

    #[test]
    fn stream_detects_truncation_and_tamper() {
        let k = SecretKey::generate();
        let data = vec![7u8; 2 * CHUNK + 5];
        let ct = encrypt_stream(&k, data.as_slice(), Vec::new()).unwrap();
        // drop the last chunk entirely
        let cut = &ct[..4 + STREAM_NONCE_LEN + 2 * (CHUNK + TAG)];
        assert!(decrypt_stream(&k, cut, Vec::new()).is_err());
        let mut bad = ct.clone();
        bad[100] ^= 1;
        assert!(decrypt_stream(&k, bad.as_slice(), Vec::new()).is_err());
    }
}
