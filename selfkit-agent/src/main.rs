//! selfkit-agent: small desktop background service (no UI). Tracks the active
//! app for Chrono, syncs every app DB, answers local IPC.
//! Docs: docs/components/selfkit-agent.md.

mod ipc;
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
  selfkit-agent [run]        start the agent (tracking + sync + IPC)
  selfkit-agent status       show agent status (JSON)
  selfkit-agent pause|resume pause/resume activity tracking
  selfkit-agent sync         trigger a sync now
  selfkit-agent today        per-app time today
  selfkit-agent init         first-time key setup (passphrase on stdin)
  selfkit-agent unlock       unlock with passphrase (stdin) and store data key in keychain

ENV: SELFKIT_DATA_DIR, SELFKIT_PG_URL, SELFKIT_DATA_KEY (base64; dev/test only), RUST_LOG,
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
    let mut probe = tracker::platform_probe()?;
    let mut sess = sessions::Sessions::new(store, platform());
    shared.inner.lock().unwrap().tracking = true;
    let mut errors = 0u32;
    loop {
        let paused = shared.inner.lock().unwrap().paused;
        sess.paused = paused;
        match probe.sample() {
            Ok(s) => {
                errors = 0;
                if let Err(e) = sess.tick(&s, time::now_ms()) {
                    tracing::warn!("record session: {e:#}");
                }
                shared.inner.lock().unwrap().current_app = sess.current_app().map(str::to_owned);
            }
            Err(e) => {
                errors += 1;
                tracing::warn!("probe: {e:#}");
                if errors > 30 {
                    // Probe connection is likely dead (X restarted); rebuild it.
                    probe = tracker::platform_probe()?;
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
    anyhow::ensure!(pass.chars().count() >= 8, "passphrase must be at least 8 characters");
    let (kf, data, rk) = KeyFile::create(&pass, KdfParams::default())?;
    kf.save(&kp)?;
    store_data_key(&data)?;
    println!("Recovery key (shown once — print or write it down):\n\n  {}\n", rk.as_str());
    Ok(())
}

fn unlock() -> Result<()> {
    let kf = selfkit_core::keys::KeyFile::load(&paths::key_file()).context("no keys.json; run `init` first")?;
    eprintln!("Passphrase:");
    let data = kf.unlock_passphrase(&read_stdin_line()?)?;
    store_data_key(&data)?;
    eprintln!("Unlocked; data key stored in the OS keychain.");
    Ok(())
}

fn store_data_key(k: &selfkit_core::crypto::SecretKey) -> Result<()> {
    use base64::Engine;
    selfkit_core::keychain::set(selfkit_core::keychain::DATA_KEY, &base64::engine::general_purpose::STANDARD.encode(k.as_bytes()))?;
    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .init();
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "run".into());
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let client = |req: ipc::Request| -> Result<()> {
        let v = rt.block_on(ipc::request(&req))?;
        println!("{}", serde_json::to_string_pretty(&v)?);
        Ok(())
    };
    match cmd.as_str() {
        "run" => rt.block_on(run()),
        "status" => client(ipc::Request::Status),
        "pause" => client(ipc::Request::Pause),
        "resume" => client(ipc::Request::Resume),
        "sync" => client(ipc::Request::SyncNow),
        "today" => client(ipc::Request::Today),
        "init" => init_keys(),
        "unlock" => unlock(),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        other => anyhow::bail!("unknown command {other:?}\n\n{USAGE}"),
    }
}
