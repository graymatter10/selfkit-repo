use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("decryption failed (wrong key or corrupted data)")]
    Decrypt,
    #[error("wrong passphrase or recovery key")]
    BadKey,
    #[error("invalid format: {0}")]
    Format(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("remote: {0}")]
    Remote(String),
    #[error("keychain: {0}")]
    Keychain(String),
}

pub type Result<T> = std::result::Result<T, Error>;
