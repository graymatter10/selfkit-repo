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

/// Desktop: the OS keychain, and a 0600 file in the data folder only while the keychain
/// can't be used (no Secret Service, e.g. bare i3) — C80/S-5, decided 2026-11-21. The file
/// is within the threat model (docs/security.md: same-user malware can read the keychain
/// and the plaintext DB anyway); a working keychain always wins.
pub struct KeychainOrFile<K: SecretStore> {
    pub keychain: K,
    pub file: FileSecrets,
}

/// The keychain *service* isn't there at all (no Secret Service on D-Bus, no D-Bus).
/// Temporary failures — keyring still locked, unlock prompt dismissed, a timeout — are
/// not this: they stay errors and never move a secret into the file (C86).
pub fn keychain_absent(msg: &str) -> bool {
    // Tagged by selfkit-core from the error kind (C96); the D-Bus names below are stable
    // identifiers (not prose) kept for messages that come without the tag.
    if msg.starts_with(selfkit_core::error::KEYCHAIN_UNAVAILABLE) {
        return true;
    }
    let m = msg.to_ascii_lowercase();
    ["org.freedesktop.dbus.error.serviceunknown", "org.freedesktop.dbus.error.nameHasNoOwner", "org.freedesktop.secrets"]
        .iter()
        .any(|k| m.contains(&k.to_ascii_lowercase()))
}

impl<K: SecretStore> SecretStore for KeychainOrFile<K> {
    fn get(&self, name: &str) -> Result<Option<String>> {
        match self.keychain.get(name) {
            Ok(Some(v)) => Ok(Some(v)),
            // Nothing in the keychain: a value saved while it was unavailable.
            Ok(None) => self.file.get(name).or(Ok(None)),
            Err(Error::Keychain(e)) if keychain_absent(&e) => self.file.get(name),
            Err(e) => Err(e),
        }
    }
    fn set(&self, name: &str, value: &str) -> Result<()> {
        match self.keychain.set(name, value) {
            Ok(()) => {
                let _ = self.file.delete(name); // the keychain copy is the one that counts
                Ok(())
            }
            Err(Error::Keychain(e)) if keychain_absent(&e) => {
                tracing::warn!("keychain unavailable ({e}); storing {name} in the data folder (0600)");
                self.file.set(name, value)
            }
            Err(e) => Err(e),
        }
    }
    fn delete(&self, name: &str) -> Result<()> {
        let k = self.keychain.delete(name);
        self.file.delete(name)?;
        match k {
            Ok(()) => Ok(()),
            Err(Error::Keychain(e)) if keychain_absent(&e) => Ok(()),
            Err(e) => Err(e),
        }
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
mod fallback_tests {
    use super::*;

    struct Broken;
    impl SecretStore for Broken {
        fn get(&self, _: &str) -> Result<Option<String>> {
            Err(Error::Keychain(ABSENT.into()))
        }
        fn set(&self, _: &str, _: &str) -> Result<()> {
            Err(Error::Keychain(ABSENT.into()))
        }
        fn delete(&self, _: &str) -> Result<()> {
            Err(Error::Keychain(ABSENT.into()))
        }
    }
    const ABSENT: &str = "Platform secure storage failure: DBus error: The name org.freedesktop.secrets was not provided by any .service files";

    struct Locked;
    impl SecretStore for Locked {
        fn get(&self, _: &str) -> Result<Option<String>> {
            Err(Error::Keychain("Couldn't access platform secure storage: the collection is locked".into()))
        }
        fn set(&self, _: &str, _: &str) -> Result<()> {
            Err(Error::Keychain("Prompt dismissed".into()))
        }
        fn delete(&self, _: &str) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn absence_is_read_from_the_tag_not_the_wording() {
        // C96: the tag set from keyring's PlatformFailure counts, whatever the text after it.
        assert!(keychain_absent(&format!("{}some new backend wording", selfkit_core::error::KEYCHAIN_UNAVAILABLE)));
        assert!(keychain_absent(ABSENT));
        assert!(!keychain_absent("Couldn't access platform secure storage: the collection is locked"));
        assert!(!keychain_absent("failed to connect: timed out"));
    }

    #[test]
    fn a_locked_keychain_never_moves_secrets_into_the_file() {
        // C86: only a missing keychain service falls back; a locked one is an error.
        let dir = tempfile::tempdir().unwrap();
        let s = KeychainOrFile { keychain: Locked, file: FileSecrets::new(dir.path().join("secrets.json"), None) };
        assert!(s.set("data-key", "k").is_err());
        assert!(!dir.path().join("secrets.json").exists());
        assert!(s.get("data-key").is_err());
    }

    #[test]
    fn file_is_used_only_while_the_keychain_is_unavailable() {
        // C80/S-5: a desktop without a keyring can still save the server and the key.
        let dir = tempfile::tempdir().unwrap();
        let file = || FileSecrets::new(dir.path().join("secrets.json"), None);
        let s = KeychainOrFile { keychain: Broken, file: file() };
        s.set("pg-url", "postgres://x").unwrap();
        assert_eq!(s.get("pg-url").unwrap().as_deref(), Some("postgres://x"));
        // A working keychain wins and the file copy goes away.
        let ok = KeychainOrFile { keychain: MemorySecrets::default(), file: file() };
        assert_eq!(ok.get("pg-url").unwrap().as_deref(), Some("postgres://x"), "saved earlier in the file");
        ok.set("pg-url", "postgres://y").unwrap();
        assert_eq!(file().get("pg-url").unwrap(), None);
        assert_eq!(ok.get("pg-url").unwrap().as_deref(), Some("postgres://y"));
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
