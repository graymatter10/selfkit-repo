//! Health screen data: the desktop agent's `<data_dir>/health.json` (schema 1,
//! docs/components/selfkit-agent.md § Health) plus the agent's IPC for
//! "Sync now" / "Back up now". Nothing here leaves the device.

use serde::Serialize;
use serde_json::Value;

use crate::model::Ms;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Health {
    /// health.json found.
    pub found: bool,
    /// The agent wrote it in the last 3 minutes (it rewrites every 60 s).
    pub running: bool,
    pub age_s: i64,
    /// The raw document (the UI picks fields).
    pub doc: Value,
    pub mobile: bool,
    /// Size of each app DB on this device (bytes), for phones without the agent.
    pub local_dbs: Vec<(String, u64)>,
    pub data_dir: String,
}

pub fn read(now: Ms) -> Health {
    let dir = selfkit_core::paths::data_dir();
    let mut h = Health { mobile: cfg!(mobile), data_dir: dir.display().to_string(), ..Default::default() };
    for app in selfkit_core::paths::APPS.iter().chain(["settings"].iter()) {
        let p = selfkit_core::paths::app_db(app);
        let mut n = 0;
        for suffix in ["", "-wal", "-shm"] {
            let mut s = p.clone().into_os_string();
            s.push(suffix);
            n += std::fs::metadata(&s).map(|m| m.len()).unwrap_or(0);
        }
        if n > 0 {
            h.local_dbs.push((app.to_string(), n));
        }
    }
    let Ok(text) = std::fs::read_to_string(dir.join("health.json")) else { return h };
    let Ok(doc) = serde_json::from_str::<Value>(&text) else { return h };
    h.found = true;
    let written = doc.get("written_at").and_then(Value::as_i64).unwrap_or(0);
    h.age_s = ((now - written) / 1000).max(0);
    h.running = h.age_s < 180;
    h.doc = doc;
    h
}

/// Send one IPC command to the running agent (`sync_now`, `backup_now`).
pub fn agent_cmd(cmd: &str) -> Result<Value, String> {
    let line = match cmd {
        "sync_now" => r#"{"cmd":"sync_now"}"#.to_string(),
        "backup_now" => r#"{"cmd":"backup_now","full":false}"#.to_string(),
        _ => return Err("unknown command".into()),
    };
    send(&line)
}

#[cfg(all(unix, not(target_os = "android")))]
fn send(line: &str) -> Result<Value, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let mut s = UnixStream::connect(std::path::Path::new(&dir).join("selfkit-agent.sock")).map_err(|_| "The background agent isn't running.".to_string())?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(120))).ok();
    s.write_all(format!("{line}\n").as_bytes()).map_err(|e| e.to_string())?;
    let mut out = String::new();
    BufReader::new(s).read_line(&mut out).map_err(|e| e.to_string())?;
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn send(line: &str) -> Result<Value, String> {
    use std::io::{BufRead, BufReader, Write};
    let mut f = std::fs::OpenOptions::new().read(true).write(true).open(r"\\.\pipe\selfkit-agent").map_err(|_| "The background agent isn't running.".to_string())?;
    f.write_all(format!("{line}\n").as_bytes()).map_err(|e| e.to_string())?;
    let mut out = String::new();
    BufReader::new(f).read_line(&mut out).map_err(|e| e.to_string())?;
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
fn send(_line: &str) -> Result<Value, String> {
    Err("The background agent runs on your PCs.".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn unknown_command_is_refused() {
        assert!(super::agent_cmd("rm").is_err());
    }
}
