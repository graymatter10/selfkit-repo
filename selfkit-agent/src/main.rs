//! selfkit-agent: small desktop background service (no UI). Tracks the active
//! app for Chrono, hosts the browser extension (native messaging), syncs every
//! app DB, makes backups, shows Agenda reminders, keeps local diagnostics and
//! answers local IPC. Docs: docs/components/selfkit-agent.md.

mod backup;
mod chrono_settings;
mod config;
mod health;
mod install;
mod ipc;
mod native_host;
mod reminders;
mod sessions;
mod state;
mod sync_loop;
mod tracker;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use selfkit_core::{Store, paths, time};

use crate::state::Shared;

const USAGE: &str = "selfkit-agent — Selfkit desktop background service

USAGE:
  selfkit-agent [run]              start the agent (tracking, sync, backups, reminders, IPC)
  selfkit-agent status             show agent status (JSON)
  selfkit-agent health             diagnostics (from the running agent, else health.json)
  selfkit-agent pause|resume       pause/resume activity tracking (this agent only)
  selfkit-agent sync               trigger a sync now
  selfkit-agent today              per-app time today
  selfkit-agent backup [--full]    run due backups now (--full: also a full export now)
  selfkit-agent restore --list     list backups on S3
  selfkit-agent restore [--app A]  restore from S3 into the data folder (newest weekly + all monthly)
  selfkit-agent restore --file F   import one .skb file
  selfkit-agent native-host        browser native-messaging host (stdin/stdout)
  selfkit-agent install-native-host [--path EXE] [--extension-id ID] [--chrome-profile DIR] [--uninstall]
  selfkit-agent install-service [--enable] [--uninstall]   systemd user unit / Windows HKCU Run entry
  selfkit-agent set-secret pg|s3   store the Postgres URL / S3 JSON (stdin) in the OS keychain
  selfkit-agent init               first-time key setup (passphrase on stdin)
  selfkit-agent unlock [--recovery] unlock with passphrase or recovery key (stdin), store data key

ENV: SELFKIT_DATA_DIR, SELFKIT_PG_URL, SELFKIT_DATA_KEY (base64; dev/test only), RUST_LOG,
     SELFKIT_S3_ENDPOINT/_BUCKET/_REGION/_KEY/_SECRET (dev/test; else keychain s3-config),
     SELFKIT_BACKUP=0, SELFKIT_BACKUP_KEEP_LOCAL=1, SELFKIT_REMINDERS=0,
     SELFKIT_LAN=0 (disable LAN sync), SELFKIT_LAN_PEERS=host:port,..., SELFKIT_LAN_PORT, SELFKIT_DEVICE_NAME";

fn platform() -> &'static str {
    if cfg!(windows) { "windows" } else if cfg!(target_os = "linux") { "linux" } else { "other" }
}

/// Local midnight (respecting day start = 00:00 for now) in UTC ms.
fn local_midnight_ms() -> i64 {
    let off = time::local_offset_min() as i64 * 60_000;
    let now = time::now_ms();
    ((now + off) / 86_400_000) * 86_400_000 - off
}

pub fn today() -> Result<serde_json::Value> {
    let p = paths::app_db("chrono");
    if !p.exists() {
        return Ok(serde_json::json!([]));
    }
    let store = Store::open_read_only(p, "chrono")?;
    let v = sessions::totals_since(&store, local_midnight_ms())?;
    Ok(serde_json::json!(v.into_iter().map(|(a, s)| serde_json::json!({"app": a, "secs": s})).collect::<Vec<_>>()))
}

fn tracker_thread(shared: Arc<Shared>) -> Result<()> {
    let device = paths::device_id()?;
    let store = Store::open(paths::app_db("chrono"), "chrono", &device, &[])?;
    let mut sess = sessions::Sessions::new(store, platform());
    // The X display may not be there yet (service started before X): retry.
    let mut probe = loop {
        match tracker::platform_probe() {
            Ok(p) => break p,
            Err(e) => {
                let msg = format!("{e:#}");
                let first = shared.inner.lock().unwrap().tracking_error.replace(msg.clone()).is_none();
                if first {
                    tracing::warn!("activity probe unavailable, retrying every 10 s: {msg}");
                }
                for _ in 0..10 {
                    if SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
    };
    {
        let mut g = shared.inner.lock().unwrap();
        g.tracking = true;
        g.tracking_error = None;
    }
    let mut errors = 0u32;
    let mut cfg = chrono_settings::ChronoSettings::default();
    for n in 0u64.. {
        let now = time::now_ms();
        if n % 10 == 0 {
            // Chrono's pause / exclude list (cheap single-row read).
            cfg = chrono_settings::ChronoSettings::load(sess.store());
            sess.exclude = cfg.exclude_apps.clone();
            let mut g = shared.inner.lock().unwrap();
            g.paused_until = cfg.paused_until;
            g.excluded_apps = cfg.exclude_apps.len();
        }
        let paused = shared.inner.lock().unwrap().paused || cfg.paused_at(now);
        sess.paused = paused;
        match probe.sample() {
            Ok(s) => {
                errors = 0;
                if let Err(e) = sess.tick(&s, now) {
                    tracing::warn!("record session: {e:#}");
                }
                shared.inner.lock().unwrap().current_app = sess.current_app().map(str::to_owned);
            }
            Err(e) => {
                errors += 1;
                tracing::debug!("probe: {e:#}");
                if errors > 30 {
                    // Probe connection is likely dead (X restarted); rebuild it.
                    match tracker::platform_probe() {
                        Ok(p) => probe = p,
                        Err(e) => tracing::warn!("activity probe: {e:#}"),
                    }
                    errors = 0;
                }
            }
        }
        if SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
            sess.shutdown(time::now_ms())?;
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

/// Backups: first check shortly after start, then hourly, or on `backup_now`.
async fn backup_loop(shared: Arc<Shared>) {
    let delay = std::env::var("SELFKIT_BACKUP_DELAY_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(delay)) => {}
        _ = shared.backup_now.notified() => {}
    }
    loop {
        let cfg = config::Config::load();
        let full = shared.backup_full.swap(false, std::sync::atomic::Ordering::Relaxed);
        if cfg.backup.enabled || full {
            tracing::debug!(full, "backup check");
            match sync_loop::data_key() {
                Ok(Some(key)) => {
                    // SQLite handles are not Send: run on a blocking thread with its own runtime.
                    let bcfg = cfg.backup.clone();
                    let r = tokio::task::spawn_blocking(move || -> Result<backup::BackupState> {
                        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
                        let st = rt.block_on(backup::run_due(&bcfg, &key, full));
                        // Don't wait for lingering HTTP/DNS tasks on drop.
                        rt.shutdown_background();
                        Ok(st)
                    })
                    .await;
                    match r {
                        Ok(Ok(st)) => tracing::debug!(bundles = st.last_bundles.len(), error = ?st.last_error, "backup check done"),
                        Ok(Err(e)) => tracing::warn!("backup: {e:#}"),
                        Err(e) => tracing::warn!("backup task: {e}"),
                    }
                }
                Ok(None) => tracing::debug!("backup skipped: locked (no data key)"),
                Err(e) => tracing::warn!("backup: data key: {e:#}"),
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(3600)) => {}
            _ = shared.backup_now.notified() => {}
        }
    }
}

static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

async fn run() -> Result<()> {
    let shared = Arc::new(Shared::default());
    shared.inner.lock().unwrap().started_at = time::now_ms();
    tracing::info!(data_dir = %paths::data_dir().display(), "selfkit-agent starting");

    let sh = shared.clone();
    let tracker = std::thread::Builder::new().name("tracker".into()).spawn(move || {
        if let Err(e) = tracker_thread(sh.clone()) {
            tracing::error!("tracker stopped: {e:#}");
            let mut g = sh.inner.lock().unwrap();
            g.tracking = false;
            g.tracking_error = Some(format!("{e:#}"));
        }
    })?;

    tokio::spawn(sync_loop::run(shared.clone()));
    tokio::spawn(sync_loop::run_lan(shared.clone()));
    tokio::spawn(backup_loop(shared.clone()));
    tokio::spawn(reminders::run(shared.clone()));
    tokio::spawn(health::run(shared.clone()));
    let ipc = tokio::spawn(ipc::serve(shared.clone()));

    tokio::select! {
        r = ipc => { r??; }
        _ = shutdown_signal() => { tracing::info!("shutting down"); }
    }
    SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = tokio::task::spawn_blocking(move || tracker.join()).await;
    #[cfg(unix)]
    let _ = std::fs::remove_file(ipc::socket_path());
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("sigterm");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

fn read_stdin_line() -> Result<String> {
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(s.trim_end_matches(['\r', '\n']).to_string())
}

fn init_keys() -> Result<()> {
    use selfkit_core::keys::{KdfParams, KeyFile};
    let kp = paths::key_file();
    anyhow::ensure!(!kp.exists(), "{} already exists", kp.display());
    eprintln!("Enter a new master passphrase:");
    let pass = read_stdin_line()?;
    anyhow::ensure!(pass.chars().count() >= 12, "passphrase must be at least 12 characters"); // same rule as the apps (C14)
    let (kf, data, rk) = KeyFile::create(&pass, KdfParams::default())?;
    kf.save(&kp)?;
    store_data_key(&data)?;
    println!("Recovery key (shown once — print or write it down):\n\n  {}\n", rk.as_str());
    Ok(())
}

fn unlock(recovery: bool) -> Result<()> {
    let kf = selfkit_core::keys::KeyFile::load(&paths::key_file())
        .context("no keys.json; run `init` first (or `restore`, which fetches it from S3)")?;
    let data = if recovery {
        eprintln!("Recovery key:");
        kf.unlock_recovery(&read_stdin_line()?)?
    } else {
        eprintln!("Passphrase:");
        kf.unlock_passphrase(&read_stdin_line()?)?
    };
    store_data_key(&data)?;
    eprintln!("Unlocked; data key stored in the OS keychain.");
    Ok(())
}

fn store_data_key(k: &selfkit_core::crypto::SecretKey) -> Result<()> {
    use base64::Engine;
    selfkit_core::keychain::set(selfkit_core::keychain::DATA_KEY, &base64::engine::general_purpose::STANDARD.encode(k.as_bytes()))?;
    Ok(())
}

fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn opt(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn opts(args: &[String], flag: &str) -> Vec<String> {
    args.windows(2).filter(|w| w[0] == flag).map(|w| w[1].clone()).collect()
}

fn print_json(v: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

fn need_key() -> Result<selfkit_core::crypto::SecretKey> {
    sync_loop::data_key()?.context("locked: no data key (run `selfkit-agent unlock`, or set SELFKIT_DATA_KEY for tests)")
}

fn main() -> Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let args: Vec<String> = std::env::args().collect();
    let browser = native_host::started_by_browser(&args);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    // stdout is the native-messaging channel in host mode: log to stderr only.
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_ansi(!browser))
        .with(health::ErrorLogLayer)
        .init();
    let cmd = if browser { "native-host".to_string() } else { args.get(1).cloned().unwrap_or_else(|| "run".into()) };
    let rest: Vec<String> = args.iter().skip(2).cloned().collect();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let client = |req: ipc::Request| -> Result<()> {
        let v = rt.block_on(ipc::request(&req))?;
        print_json(&v)
    };
    match cmd.as_str() {
        "run" => rt.block_on(run()),
        "status" => client(ipc::Request::Status),
        "pause" => client(ipc::Request::Pause),
        "resume" => client(ipc::Request::Resume),
        "sync" => client(ipc::Request::SyncNow),
        "today" => client(ipc::Request::Today),
        "health" => match rt.block_on(ipc::request(&ipc::Request::Health)) {
            Ok(v) => print_json(&v["health"]),
            Err(_) => {
                let b = std::fs::read(health::health_path()).context("agent not running and no health.json yet")?;
                print_json(&serde_json::from_slice(&b)?)
            }
        },
        "native-host" => native_host::run(args, &rt),
        "backup" => {
            let cfg = config::Config::load();
            let key = need_key()?;
            let st = rt.block_on(backup::run_due(&cfg.backup, &key, has(&rest, "--full")));
            print_json(&serde_json::to_value(&st)?)?;
            anyhow::ensure!(st.last_error.is_none(), "backup failed");
            Ok(())
        }
        "restore" => {
            if has(&rest, "--list") {
                let v = rt.block_on(backup::list_remote())?;
                for r in v {
                    println!("{:<8} {:<8} {:<10} {:>10}  {}", r.app, r.kind, r.label, r.size, r.path);
                }
                return Ok(());
            }
            if let Some(f) = opt(&rest, "--file") {
                return print_json(&backup::restore_file(&need_key()?, std::path::Path::new(&f))?);
            }
            if rt.block_on(backup::fetch_key_file())? {
                eprintln!("Fetched keys.json from S3 into {}", paths::data_dir().display());
            }
            let key = need_key()?;
            let v = rt.block_on(backup::restore_s3(&key, opt(&rest, "--app").as_deref()))?;
            print_json(&v)
        }
        "install-native-host" => {
            let exe = match opt(&rest, "--path") {
                Some(p) => std::path::PathBuf::from(p),
                None => std::env::current_exe()?,
            };
            let o = install::NativeHostOpts {
                exe,
                extra_ids: opts(&rest, "--extension-id"),
                profile_dirs: opts(&rest, "--chrome-profile").into_iter().map(Into::into).collect(),
                uninstall: has(&rest, "--uninstall"),
            };
            for l in install::install_native_host(&o)? {
                println!("{l}");
            }
            Ok(())
        }
        "install-service" => {
            let exe = match opt(&rest, "--path") {
                Some(p) => std::path::PathBuf::from(p),
                None => std::env::current_exe()?,
            };
            for l in install::install_service(&exe, has(&rest, "--enable"), has(&rest, "--uninstall"))? {
                println!("{l}");
            }
            Ok(())
        }
        "set-secret" => {
            let which = rest.first().map(String::as_str);
            eprintln!("Paste the value and press Enter:");
            let v = read_stdin_line()?;
            match which {
                Some("pg") => selfkit_core::keychain::set(selfkit_core::keychain::PG_URL, v.trim())?,
                Some("s3") => {
                    let _: selfkit_core::sync::s3::S3Config = serde_json::from_str(&v).context(
                        "expected JSON {endpoint, bucket, region, access_key, secret_key, path_style}",
                    )?;
                    selfkit_core::keychain::set(selfkit_core::keychain::S3_CONFIG, v.trim())?
                }
                _ => anyhow::bail!("usage: selfkit-agent set-secret pg|s3"),
            }
            eprintln!("Stored in the OS keychain.");
            Ok(())
        }
        "init" => init_keys(),
        "unlock" => unlock(has(&rest, "--recovery")),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        other => anyhow::bail!("unknown command {other:?}\n\n{USAGE}"),
    }
}
