//! Shared Selfkit data folder: `<data_dir>/selfkit/` (override with `SELFKIT_DATA_DIR`).
use std::path::PathBuf;

pub const APPS: [&str; 5] = ["chrono", "hisab", "tickd", "reflect", "dusk"];

pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("SELFKIT_DATA_DIR") {
        return PathBuf::from(d);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("selfkit")
}

/// SQLite file for an app (or `settings`).
pub fn app_db(app: &str) -> PathBuf {
    data_dir().join(format!("{app}.db"))
}

pub fn key_file() -> PathBuf {
    data_dir().join("keys.json")
}

/// Stable per-device id shared by all apps on this device (created on first call).
pub fn device_id() -> crate::Result<String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let p = dir.join("device_id");
    if let Ok(s) = std::fs::read_to_string(&p) {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return Ok(s);
        }
    }
    let id = uuid::Uuid::now_v7().to_string();
    std::fs::write(&p, &id)?;
    Ok(id)
}
