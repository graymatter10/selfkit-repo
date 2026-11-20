//! OS keychain for secrets (data key, DB URL, S3 keys): Secret Service on
//! Linux, Credential Manager on Windows. Android uses the Keystore plugin.

use crate::{Error, Result};

const SERVICE: &str = "com.selfkit";

pub const DATA_KEY: &str = "data-key";
pub const PG_URL: &str = "postgres-url";
pub const S3_CONFIG: &str = "s3-config";

/// The keychain service itself isn't there, decided from the keyring error *kind*, not its
/// wording (C96): on Linux the Secret Service backend reports a locked keyring or a
/// dismissed prompt as `NoStorageAccess` and everything else (no Secret Service on D-Bus,
/// no D-Bus) as `PlatformFailure`.
pub use crate::error::KEYCHAIN_UNAVAILABLE as UNAVAILABLE;

fn err(e: keyring::Error) -> Error {
    if cfg!(target_os = "linux") && matches!(e, keyring::Error::PlatformFailure(_)) {
        Error::Keychain(format!("{UNAVAILABLE}{e}"))
    } else {
        Error::Keychain(e.to_string())
    }
}

fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).map_err(err)
}

pub fn set(name: &str, secret: &str) -> Result<()> {
    entry(name)?.set_password(secret).map_err(err)
}

pub fn get(name: &str) -> Result<Option<String>> {
    match entry(name)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(err(e)),
    }
}

pub fn delete(name: &str) -> Result<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(err(e)),
    }
}
