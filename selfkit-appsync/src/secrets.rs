//! Where secrets live (data key, Postgres URL, S3 keys). One store per device,
//! shared by every Selfkit app on it where the OS allows:
//! - Linux/Windows: the OS keychain, service `com.selfkit` (same entries the agent uses).
//! - Android: [`FileSecrets`] in the app's private folder, sealed with a wrapping
//!   key that the Kotlin plugin keeps in the Android Keystore (`SelfkitSecrets.wrapKey()`).
//! - Tests: [`MemorySecrets`] or an unsealed [`FileSecrets`].

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use selfkit_core::crypto::{self, SecretKey};
use selfkit_core::{Error, Result};

/// Entry names (identical to `selfkit_core::keychain`, so apps and agent share them).
pub const DATA_KEY: &str = "data-key";
pub const PG_URL: &str = "postgres-url";
pub const S3_CONFIG: &str = "s3-config";

pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<String>>;
    fn set(&self, name: &str, value: &str) -> Result<()>;
    fn delete(&self, name: &str) -> Result<()>;
}

#[derive(Default)]
pub struct MemorySecrets(Mutex<BTreeMap<String, String>>);

impl SecretStore for MemorySecrets {
    fn get(&self, name: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }
    fn set(&self, name: &str, value: &str) -> Result<()> {
        self.0.lock().unwrap().insert(name.into(), value.into());
        Ok(())
    }
    fn delete(&self, name: &str) -> Result<()> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}

/// JSON file of secrets, optionally sealed (XChaCha20-Poly1305) with a wrapping key.
/// Written atomically with mode 0600.
pub struct FileSecrets {
    path: PathBuf,
    wrap: Option<SecretKey>,
    lock: Mutex<()>,
}

const AAD: &[u8] = b"selfkit-secrets-v1";

impl FileSecrets {
    pub fn new(path: impl Into<PathBuf>, wrap: Option<SecretKey>) -> Self {
        Self { path: path.into(), wrap, lock: Mutex::new(()) }
    }

    fn read(&self) -> Result<BTreeMap<String, String>> {
        let raw = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(e.into()),
        };
        let plain = match &self.wrap {
            Some(k) => crypto::open(k, &B64.decode(raw.trim_ascii()).map_err(|_| Error::Format("secrets base64".into()))?, AAD)?,
            None => raw,
        };
        Ok(serde_json::from_slice(&plain)?)
    }

    fn write(&self, m: &BTreeMap<String, String>) -> Result<()> {
        let plain = serde_json::to_vec(m)?;
        let out = match &self.wrap {
            Some(k) => B64.encode(crypto::seal(k, &plain, AAD)).into_bytes(),
            None => plain,
        };
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, out)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

impl SecretStore for FileSecrets {
    fn get(&self, name: &str) -> Result<Option<String>> {
        let _g = self.lock.lock().unwrap();
        Ok(self.read()?.get(name).cloned())
    }
    fn set(&self, name: &str, value: &str) -> Result<()> {
        let _g = self.lock.lock().unwrap();
        let mut m = self.read()?;
        m.insert(name.into(), value.into());
        self.write(&m)
    }
    fn delete(&self, name: &str) -> Result<()> {
        let _g = self.lock.lock().unwrap();
        let mut m = self.read()?;
        if m.remove(name).is_some() {
            self.write(&m)?;
        }
        Ok(())
    }
}

/// The OS keychain (desktop). Same service/entries as `selfkit-agent set-secret` / `unlock`.
#[cfg(not(target_os = "android"))]
pub struct KeychainSecrets;

#[cfg(not(target_os = "android"))]
impl SecretStore for KeychainSecrets {
    fn get(&self, name: &str) -> Result<Option<String>> {
        selfkit_core::keychain::get(name)
    }
    fn set(&self, name: &str, value: &str) -> Result<()> {
        selfkit_core::keychain::set(name, value)
    }
    fn delete(&self, name: &str) -> Result<()> {
        selfkit_core::keychain::delete(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_secrets_sealed_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.json");
        let k = SecretKey::generate();
        let s = FileSecrets::new(&p, Some(k.clone()));
        s.set(PG_URL, "postgres://u:hunter2@h/db").unwrap();
        assert_eq!(s.get(PG_URL).unwrap().as_deref(), Some("postgres://u:hunter2@h/db"));
        assert!(!String::from_utf8_lossy(&std::fs::read(&p).unwrap()).contains("hunter2"));
        assert!(FileSecrets::new(&p, Some(SecretKey::generate())).get(PG_URL).is_err());
        s.delete(PG_URL).unwrap();
        assert_eq!(s.get(PG_URL).unwrap(), None);
    }
}
