//! Local diagnostics (never sent anywhere): an error log, the agent's CPU/RAM,
//! storage used, sync and backup status. Exposed as the IPC `health` command and
//! written every minute to `<data_dir>/health.json` for the Reflect health screen.
//! Shape: docs/components/selfkit-agent.md § Health.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use selfkit_core::{Store, paths, time};
use serde_json::{Value, json};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

use crate::state::Shared;

pub const SCHEMA: u32 = 1;
const LOG_MAX: u64 = 512 * 1024;

pub fn log_path() -> PathBuf {
    paths::data_dir().join("logs").join("agent-errors.log")
}

pub fn health_path() -> PathBuf {
    paths::data_dir().join("health.json")
}

// ---- error log --------------------------------------------------------------

/// tracing layer: appends WARN/ERROR events as JSON lines to the error log
/// (rotated to `.1` at 512 KiB). Used by every agent mode, incl. the native host.
pub struct ErrorLogLayer;

#[derive(Default)]
struct MsgVisitor(String);

impl Visit for MsgVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if field.name() == "message" {
            self.0.push_str(&format!("{value:?}"));
        } else {
            self.0.push_str(&format!("{}={value:?}", field.name()));
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(value);
        } else {
            self.record_debug(field, &value);
        }
    }
}

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub fn append_log(level: &str, source: &str, message: &str) {
    let _g = LOG_LOCK.lock();
    let p = log_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if std::fs::metadata(&p).is_ok_and(|m| m.len() > LOG_MAX) {
        let _ = std::fs::rename(&p, p.with_extension("log.1"));
    }
    let line = json!({"at": time::now_ms(), "level": level, "source": source, "message": message, "pid": std::process::id()});
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{line}");
    }
}

impl<S: Subscriber> Layer<S> for ErrorLogLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let lvl = *event.metadata().level();
        if lvl > Level::WARN {
            return;
        }
        let mut v = MsgVisitor::default();
        event.record(&mut v);
        append_log(lvl.as_str(), event.metadata().target(), &v.0);
    }
}

/// Last `n` entries of the error log (reads only the file's tail).
pub fn recent_errors(n: usize) -> Vec<Value> {
    let Ok(mut f) = std::fs::File::open(log_path()) else { return vec![] };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(32 * 1024);
    let _ = f.seek(SeekFrom::Start(start));
    let mut s = String::new();
    let _ = f.read_to_string(&mut s);
    let mut lines: Vec<Value> = s.lines().skip(usize::from(start > 0)).filter_map(|l| serde_json::from_str(l).ok()).collect();
    let k = lines.len().saturating_sub(n);
    lines.drain(..k);
    lines.reverse();
    lines
}

// ---- process stats ----------------------------------------------------------

/// Resident memory of this process in KiB.
pub fn rss_kib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = s.split_whitespace().nth(1)?.parse().ok()?;
        return Some(pages * 4);
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
        use windows::Win32::System::Threading::GetCurrentProcess;
        let mut c = PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb).is_ok() {
            return Some(c.WorkingSetSize as u64 / 1024);
        }
        return None;
    }
    #[allow(unreachable_code)]
    None
}

/// Total CPU time (user + system) used by this process, in ms.
pub fn cpu_ms() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string("/proc/self/stat").ok()?;
        // Fields after the ")" of the command name; utime/stime are fields 14/15.
        let rest = &s[s.rfind(')')? + 2..];
        let f: Vec<&str> = rest.split_whitespace().collect();
        let ticks: u64 = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
        return Some(ticks * 10); // USER_HZ = 100 on Linux
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::FILETIME;
        use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).is_ok() {
            let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) / 10_000;
            return Some(t(k) + t(u));
        }
        return None;
    }
    #[allow(unreachable_code)]
    None
}

/// CPU use since the previous call, in percent of one core.
pub struct CpuMeter(Mutex<Option<(Instant, u64)>>);

impl CpuMeter {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }
    pub fn percent(&self) -> Option<f64> {
        let now = (Instant::now(), cpu_ms()?);
        let mut g = self.0.lock().unwrap();
        let prev = g.replace(now);
        let (t0, c0) = prev?;
        let wall = now.0.duration_since(t0).as_millis() as f64;
        (wall >= 500.0).then(|| ((now.1.saturating_sub(c0)) as f64 / wall * 1000.0).round() / 10.0)
    }
}

pub static CPU: CpuMeter = CpuMeter::new();

// ---- storage ------------------------------------------------------------------

fn dir_size(p: &Path, depth: u32) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() && depth > 0 => dir_size(&e.path(), depth - 1),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

fn db_size(app: &str) -> u64 {
    let p = paths::app_db(app);
    ["", "-wal", "-shm"].iter().map(|s| std::fs::metadata(format!("{}{s}", p.display())).map(|m| m.len()).unwrap_or(0)).sum()
}

pub fn storage() -> Value {
    let dir = paths::data_dir();
    let mut apps = serde_json::Map::new();
    for app in crate::backup::backup_apps() {
        apps.insert(app.into(), json!(db_size(app)));
    }
    json!({
        "data_dir": dir.display().to_string(),
        "total_bytes": dir_size(&dir, 6),
        "apps": apps,
        "files_bytes": dir_size(&dir.join("files"), 4),
        "backups_bytes": dir_size(&dir.join("backups"), 4),
        "logs_bytes": dir_size(&dir.join("logs"), 1),
    })
}

/// Records not yet pushed to the server, over all app DBs.
fn pending_records() -> i64 {
    crate::backup::backup_apps()
        .filter_map(|app| {
            let p = paths::app_db(app);
            p.exists().then(|| Store::open_read_only(p, app).ok()?.dirty_count().ok()).flatten()
        })
        .sum()
}

/// Newest browser session end per browser (from chrono.db).
fn web_status() -> Value {
    let p = paths::app_db("chrono");
    let q = || -> Option<Value> {
        let st = Store::open_read_only(p, "chrono").ok()?;
        let mut stmt = st
            .conn()
            .prepare(
                "SELECT json_extract(data,'$.browser'), MAX(json_extract(data,'$.end')) FROM records
                 WHERE collection='web_sessions' AND deleted=0 GROUP BY 1",
            )
            .ok()?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))).ok()?;
        let m: serde_json::Map<String, Value> = rows.flatten().map(|(b, e)| (b, json!(e))).collect();
        Some(json!({"last_session_end": m}))
    };
    q().unwrap_or_else(|| json!({"last_session_end": {}}))
}

/// Build the full health document.
pub fn snapshot(shared: &Shared) -> Value {
    let now = time::now_ms();
    let s = shared.inner.lock().unwrap().clone();
    let cfg = crate::config::Config::load();
    let limit = cfg.server_db_limit_bytes();
    let backup = crate::backup::BackupState::load();
    let peers = shared.lan.snapshot();
    let paused_until = s.paused_until.filter(|u| *u > now);
    json!({
        "schema": SCHEMA,
        "written_at": now,
        "version": env!("CARGO_PKG_VERSION"),
        "device_id": paths::device_id().ok(),
        "agent": {
            "pid": std::process::id(),
            "started_at": s.started_at,
            "uptime_secs": (now - s.started_at) / 1000,
            "rss_kib": rss_kib(),
            "cpu_percent": CPU.percent(),
            "cpu_secs_total": cpu_ms().map(|m| m as f64 / 1000.0),
        },
        "tracking": {
            "on": s.tracking,
            "paused": s.paused || paused_until.is_some(),
            "paused_until": paused_until,
            "current_app": s.current_app,
            "excluded_apps": s.excluded_apps,
            "error": s.tracking_error,
        },
        "web": web_status(),
        "sync": {
            "configured": s.sync_configured,
            "last_at": s.last_sync_at,
            "last_ok_at": s.last_sync_ok_at,
            "last_error": s.last_sync_error,
            "last_report": s.last_sync,
            "pending_records": pending_records(),
            "server_db_bytes": s.server_db_bytes,
            "server_db_limit_bytes": limit,
            "server_db_warn": s.server_db_bytes.is_some_and(|b| b as u64 * 10 >= limit * 8),
        },
        "lan": {"enabled": s.lan_enabled, "error": s.lan_error, "peers": peers.len()},
        "backup": {
            "enabled": cfg.backup.enabled,
            "keep_local": cfg.backup.keep_local,
            "target": backup.target,
            "last_run_at": backup.last_run_at,
            "last_ok_at": backup.last_ok_at,
            "last_error": backup.last_error,
            "last_weekly": backup.weekly,
            "last_monthly": backup.monthly,
        },
        "reminders": {
            "enabled": cfg.reminders.enabled,
            "last_fired_at": s.reminders_last_fired_at,
            "fired_since_start": s.reminders_fired,
        },
        "storage": storage(),
        "errors": recent_errors(20),
    })
}

/// Write `health.json` atomically.
pub fn write_file(v: &Value) -> std::io::Result<()> {
    let p = health_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(tmp, p)
}

/// Every 60 s: refresh `health.json`.
pub async fn run(shared: std::sync::Arc<Shared>) {
    loop {
        let sh = shared.clone();
        let v = tokio::task::spawn_blocking(move || snapshot(&sh)).await;
        if let Ok(v) = v
            && let Err(e) = write_file(&v)
        {
            tracing::warn!("health.json: {e}");
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_stats_work() {
        assert!(rss_kib().unwrap() > 100);
        let m = CpuMeter::new();
        assert!(m.percent().is_none());
        let t = Instant::now();
        while t.elapsed().as_millis() < 600 {
            std::hint::black_box((0..1000).sum::<u64>());
        }
        let p = m.percent().unwrap();
        assert!(p > 10.0, "busy loop should use CPU, got {p}");
    }
}
