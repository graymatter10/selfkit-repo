//! OS keychain for secrets (data key, DB URL, S3 keys): Secret Service on
//! Linux, Credential Manager on Windows. Android uses the Keystore plugin.

use crate::{Error, Result};

const SERVICE: &str = "com.selfkit";

pub const DATA_KEY: &str = "data-key";
pub const PG_URL: &str = "postgres-url";
pub const S3_CONFIG: &str = "s3-config";

fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).map_err(|e| Error::Keychain(e.to_string()))
}

pub fn set(name: &str, secret: &str) -> Result<()> {
    entry(name)?.set_password(secret).map_err(|e| Error::Keychain(e.to_string()))
}

pub fn get(name: &str) -> Result<Option<String>> {
    match entry(name)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Keychain(e.to_string())),
    }
}

pub fn delete(name: &str) -> Result<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Keychain(e.to_string())),
    }
}
