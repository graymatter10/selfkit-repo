//! Shared Selfkit data folder: `<data_dir>/selfkit/` (override with `SELFKIT_DATA_DIR`).
use std::path::PathBuf;

pub const APPS: [&str; 5] = ["chrono", "hisab", "agenda", "reflect", "dusk"];

static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Override the data folder (Android: the app's private data dir). Call once at startup.
pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

pub fn data_dir() -> PathBuf {
    if let Some(d) = DATA_DIR.get() {
        return d.clone();
    }
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
    let read = || std::fs::read_to_string(&p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(s) = read() {
        return Ok(s);
    }
    // Exclusive create: when two processes start together on a fresh install (agent +
    // app), only one id wins and the other process reads it (C27).
    let id = uuid::Uuid::now_v7().to_string();
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
        Ok(mut f) => {
            use std::io::Write;
            f.write_all(id.as_bytes())?;
            f.sync_all()?;
            Ok(id)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // The other process may not have written it yet.
            for _ in 0..50 {
                if let Some(s) = read() {
                    return Ok(s);
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            // An empty file left by a crash: take it over.
            std::fs::write(&p, &id)?;
            Ok(id)
        }
        Err(e) => Err(e.into()),
    }
}
