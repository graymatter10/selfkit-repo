//! Key management: one random data key, wrapped twice — by the passphrase
//! (Argon2id) and by the recovery key (HKDF). See docs/security.md.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use data_encoding::BASE32_NOPAD;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LEN, SecretKey};
use crate::{Error, Result};

const WRAP_AAD: &[u8] = b"selfkit-datakey-v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfParams {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl Default for KdfParams {
    /// 64 MiB / 3 passes: ~0.5 s on a weak phone, stays inside the RAM budget.
    fn default() -> Self {
        Self { m_kib: 64 * 1024, t: 3, p: 1 }
    }
}

impl KdfParams {
    /// Cheap params for tests only.
    pub fn insecure_fast() -> Self {
        Self { m_kib: 64, t: 1, p: 1 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kdf", rename_all = "snake_case")]
pub enum Envelope {
    Argon2id { params: KdfParams, salt: String, wrapped: String },
    HkdfSha256 { salt: String, wrapped: String },
}

/// Safe to store anywhere (holds only wrapped keys). Written to `keys.json`
/// and synced so a new device can unlock with the passphrase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyFile {
    pub version: u32,
    pub passphrase: Envelope,
    pub recovery: Envelope,
    /// When the passphrase or recovery key last changed (ms). Only a newer file replaces
    /// the published one, so a device with an old keys.json can't put it back (R-2).
    /// 0 = written before this field existed.
    #[serde(default)]
    pub changed_at: i64,
}

/// Human-readable recovery key: 52 base32 chars in 13 groups of 4.
pub struct RecoveryKey(Zeroizing<String>);

impl RecoveryKey {
    fn generate() -> (Self, Zeroizing<[u8; KEY_LEN]>) {
        let raw = Zeroizing::new(crypto::random_bytes::<KEY_LEN>());
        let s = BASE32_NOPAD.encode(raw.as_slice());
        let grouped = s.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("-");
        (Self(Zeroizing::new(grouped)), raw)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    fn parse(s: &str) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        let clean: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
        let bytes = BASE32_NOPAD.decode(clean.as_bytes()).map_err(|_| Error::BadKey)?;
        let arr: [u8; KEY_LEN] = bytes.as_slice().try_into().map_err(|_| Error::BadKey)?;
        Ok(Zeroizing::new(arr))
    }
}

/// keys.json can come from the server/S3, so its KDF parameters are untrusted input:
/// a tampered file must not make a phone allocate gigabytes or spin for hours.
const MAX_M_KIB: u32 = 256 * 1024;
const MAX_T: u32 = 16;
const MAX_P: u32 = 8;

fn argon_kek(pass: &str, salt: &[u8], p: KdfParams) -> Result<SecretKey> {
    if p.m_kib > MAX_M_KIB || p.t > MAX_T || p.p > MAX_P || p.p == 0 || p.t == 0 {
        return Err(Error::Format("key file: KDF parameters out of range".into()));
    }
    let params = Params::new(p.m_kib, p.t, p.p, Some(KEY_LEN)).map_err(|e| Error::Format(e.to_string()))?;
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pass.as_bytes(), salt, out.as_mut_slice())
        .map_err(|e| Error::Format(e.to_string()))?;
    Ok(SecretKey::from_bytes(*out))
}

fn hkdf_kek(raw: &[u8], salt: &[u8]) -> SecretKey {
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    Hkdf::<Sha256>::new(Some(salt), raw).expand(b"selfkit-recovery-kek", out.as_mut_slice()).expect("len ok");
    SecretKey::from_bytes(*out)
}

fn wrap_passphrase(data: &SecretKey, pass: &str, params: KdfParams) -> Result<Envelope> {
    let salt = crypto::random_bytes::<16>();
    let kek = argon_kek(pass, &salt, params)?;
    Ok(Envelope::Argon2id {
        params,
        salt: B64.encode(salt),
        wrapped: B64.encode(crypto::seal(&kek, data.as_bytes(), WRAP_AAD)),
    })
}

fn wrap_recovery(data: &SecretKey, raw: &[u8]) -> Envelope {
    let salt = crypto::random_bytes::<16>();
    let kek = hkdf_kek(raw, &salt);
    Envelope::HkdfSha256 { salt: B64.encode(salt), wrapped: B64.encode(crypto::seal(&kek, data.as_bytes(), WRAP_AAD)) }
}

fn unwrap_with(kek: &SecretKey, wrapped: &str) -> Result<SecretKey> {
    let ct = B64.decode(wrapped).map_err(|_| Error::Format("base64".into()))?;
    let pt = Zeroizing::new(crypto::open(kek, &ct, WRAP_AAD).map_err(|_| Error::BadKey)?);
    SecretKey::from_slice(&pt)
}

fn b64(s: &str) -> Result<Vec<u8>> {
    B64.decode(s).map_err(|_| Error::Format("base64".into()))
}

impl KeyFile {
    /// First-time setup. Returns the key file, the unlocked data key and the
    /// recovery key (show once, printable).
    pub fn create(passphrase: &str, params: KdfParams) -> Result<(KeyFile, SecretKey, RecoveryKey)> {
        let data = SecretKey::generate();
        let (rk, raw) = RecoveryKey::generate();
        let kf = KeyFile { version: 1, passphrase: wrap_passphrase(&data, passphrase, params)?, recovery: wrap_recovery(&data, raw.as_slice()), changed_at: crate::time::now_ms() };
        Ok((kf, data, rk))
    }

    pub fn unlock_passphrase(&self, passphrase: &str) -> Result<SecretKey> {
        match &self.passphrase {
            Envelope::Argon2id { params, salt, wrapped } => unwrap_with(&argon_kek(passphrase, &b64(salt)?, *params)?, wrapped),
            _ => Err(Error::Format("passphrase envelope kdf".into())),
        }
    }

    pub fn unlock_recovery(&self, recovery_key: &str) -> Result<SecretKey> {
        let raw = RecoveryKey::parse(recovery_key)?;
        match &self.recovery {
            Envelope::HkdfSha256 { salt, wrapped } => unwrap_with(&hkdf_kek(raw.as_slice(), &b64(salt)?), wrapped),
            _ => Err(Error::Format("recovery envelope kdf".into())),
        }
    }

    /// Instant: only re-wraps the data key.
    pub fn change_passphrase(&mut self, data: &SecretKey, new_passphrase: &str, params: KdfParams) -> Result<()> {
        self.passphrase = wrap_passphrase(data, new_passphrase, params)?;
        self.changed_at = crate::time::now_ms();
        Ok(())
    }

    /// Issue a new recovery key (old one stops working).
    pub fn rotate_recovery(&mut self, data: &SecretKey) -> RecoveryKey {
        let (rk, raw) = RecoveryKey::generate();
        self.recovery = wrap_recovery(data, raw.as_slice());
        self.changed_at = crate::time::now_ms();
        rk
    }

    pub fn load(path: &std::path::Path) -> Result<Self> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_unlock_change_recover() {
        let p = KdfParams::insecure_fast();
        let (mut kf, data, rk) = KeyFile::create("correct horse", p).unwrap();
        assert_eq!(kf.unlock_passphrase("correct horse").unwrap(), data);
        assert!(matches!(kf.unlock_passphrase("wrong"), Err(Error::BadKey)));
        // recovery key works with dashes, spaces and lowercase
        assert_eq!(rk.as_str().len(), 52 + 12);
        assert_eq!(kf.unlock_recovery(&rk.as_str().to_lowercase().replace('-', " ")).unwrap(), data);
        kf.change_passphrase(&data, "new pass", p).unwrap();
        assert!(kf.unlock_passphrase("correct horse").is_err());
        assert_eq!(kf.unlock_passphrase("new pass").unwrap(), data);
        // json roundtrip
        let kf2: KeyFile = serde_json::from_str(&serde_json::to_string(&kf).unwrap()).unwrap();
        assert_eq!(kf2.unlock_recovery(rk.as_str()).unwrap(), data);
        let rk2 = kf.rotate_recovery(&data);
        assert!(kf.unlock_recovery(rk.as_str()).is_err());
        assert_eq!(kf.unlock_recovery(rk2.as_str()).unwrap(), data);
    }

    #[test]
    fn tampered_kdf_params_are_refused() {
        let (mut kf, _, _) = KeyFile::create("pw", KdfParams::insecure_fast()).unwrap();
        if let Envelope::Argon2id { params, .. } = &mut kf.passphrase {
            params.m_kib = 4 * 1024 * 1024; // 4 GiB
        }
        assert!(matches!(kf.unlock_passphrase("pw"), Err(Error::Format(_))));
    }

    #[test]
    fn default_params_work() {
        let (kf, data, _) = KeyFile::create("pw", KdfParams::default()).unwrap();
        assert_eq!(kf.unlock_passphrase("pw").unwrap(), data);
    }
}
