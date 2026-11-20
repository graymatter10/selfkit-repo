//! App lock commands shared by every app (feature `tauri`). Logic: selfkit_core::applock.
//! Docs: docs/components/app-lock.md. UI: ui/src/lock/.
//!
//! ```ignore
//! app.manage(selfkit_appsync::lock_cmds::LockHandle::new("agenda", &data_dir));
//! .invoke_handler(tauri::generate_handler![..., selfkit_appsync::lock_cmds::lock_status, ...])
//! ```
//! Argon2id runs on a blocking thread so the UI stays responsive.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use selfkit_core::applock::{AppLock, Attempt, LockStatus, SecretKind};
use selfkit_core::keys::KeyFile;
use serde::Serialize;
use tauri::State;

#[derive(Clone)]
pub struct LockHandle(pub Arc<Mutex<AppLock>>, PathBuf);

impl LockHandle {
    /// `<data_dir>/lock/<app>.json`; locked on start when a lock is set.
    pub fn new(app: &str, data_dir: &Path) -> Self {
        Self(Arc::new(Mutex::new(AppLock::open(AppLock::path_for(data_dir, app)))), data_dir.to_path_buf())
    }
}

/// Test builds (feature `lock-test`, on by default in the apps): the lock screen opens
/// by itself after a few seconds unless someone starts typing, so automated phone tests
/// never get stuck. Release workflows (`SELFKIT_RELEASE=1`) always turn it off.
pub fn test_unlock_allowed() -> bool {
    cfg!(feature = "lock-test") && !crate::release_build()
}

fn now_ms() -> i64 {
    selfkit_core::time::now_ms()
}

type Res<T> = Result<T, String>;

#[derive(Serialize)]
pub struct LockView {
    #[serde(flatten)]
    pub status: LockStatus,
    pub platform: &'static str,
    /// Encryption is set up here (keys.json), so "Forgot PIN?" can use the passphrase.
    pub can_reset: bool,
    /// Test build: the lock screen auto-opens (see `test_unlock_allowed`).
    pub test_build: bool,
    /// Where the lock file is, for the desktop "Forgot PIN?" hint (L6).
    pub lock_file: String,
    /// Increases with every view, taken under the lock mutex, so the UI can keep the reply
    /// that describes the latest state even when commands finished in another order than
    /// they were sent (C97).
    pub seq: u64,
}

static VIEW_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn view(l: &AppLock, data_dir: &Path) -> LockView {
    LockView {
        status: l.status(now_ms()),
        platform: if cfg!(target_os = "android") { "android" } else { "desktop" },
        can_reset: data_dir.join("keys.json").exists(),
        test_build: test_unlock_allowed(),
        lock_file: l.path().display().to_string(),
        seq: VIEW_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
    }
}

fn wait_text(ms: u64) -> String {
    let s = ms.div_ceil(1000);
    if s < 60 { format!("{s} s") } else { format!("{} min", s.div_ceil(60)) }
}

/// Answer to a secret check, ready for the screen.
#[derive(Serialize)]
pub struct AttemptView {
    #[serde(flatten)]
    pub attempt: Attempt,
    pub lock: LockView,
}

fn run<T: Send + 'static>(h: &LockHandle, f: impl FnOnce(&mut AppLock, &Path) -> selfkit_core::Result<T> + Send + 'static) -> impl std::future::Future<Output = Res<T>> {
    let h = h.clone();
    async move {
        tauri::async_runtime::spawn_blocking(move || {
            let mut l = h.0.lock().unwrap_or_else(|p| p.into_inner());
            f(&mut *l, h.1.as_path()).map_err(|e| match e {
                selfkit_core::Error::Format(m) => m,
                other => other.to_string(),
            })
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

fn with<T>(h: &LockHandle, f: impl FnOnce(&mut AppLock, &Path) -> T) -> T {
    let mut l = h.0.lock().unwrap_or_else(|p| p.into_inner());
    f(&mut *l, h.1.as_path())
}

// Every command runs off the UI thread (`async`): they share the mutex that
// `lock_unlock`/`change`/`disable`/`reset` hold during Argon2id, which froze the window
// on a blur/focus during a hash (C13).
#[tauri::command(async)]
pub fn lock_status(h: State<'_, LockHandle>) -> Res<LockView> {
    Ok(with(&h, |l, d| view(l, d)))
}

#[tauri::command]
pub async fn lock_unlock(h: State<'_, LockHandle>, secret: String) -> Res<AttemptView> {
    run(&h, move |l, d| {
        let attempt = l.unlock(&secret, now_ms())?;
        Ok(AttemptView { attempt, lock: view(l, d) })
    })
    .await
}

/// Called after Android's BiometricPrompt succeeded (Kotlin → page → here). Refused on
/// desktop, where there is no prompt and it would skip the PIN (C12).
#[tauri::command(async)]
pub fn lock_unlock_biometric(h: State<'_, LockHandle>) -> Res<LockView> {
    if !cfg!(target_os = "android") {
        return Err("Fingerprint unlock is only on Android.".into());
    }
    with(&h, |l, d| {
        l.unlock_biometric().map_err(|e| e.to_string())?;
        Ok(view(l, d))
    })
}

/// Test builds only: open without a secret. Refused in other builds.
#[tauri::command(async)]
pub fn lock_test_unlock(h: State<'_, LockHandle>) -> Res<LockView> {
    if !test_unlock_allowed() {
        return Err("Not a test build.".into());
    }
    Ok(with(&h, |l, d| {
        l.unlock_unchecked();
        view(l, d)
    }))
}

#[tauri::command]
pub async fn lock_enable(h: State<'_, LockHandle>, kind: SecretKind, secret: String) -> Res<LockView> {
    run(&h, move |l, d| {
        l.enable(kind, &secret)?;
        Ok(view(l, d))
    })
    .await
}

#[tauri::command]
pub async fn lock_change(h: State<'_, LockHandle>, current: String, kind: SecretKind, secret: String) -> Res<AttemptView> {
    run(&h, move |l, d| {
        let attempt = l.change(&current, kind, &secret, now_ms())?;
        Ok(AttemptView { attempt, lock: view(l, d) })
    })
    .await
}

#[tauri::command]
pub async fn lock_disable(h: State<'_, LockHandle>, current: String) -> Res<AttemptView> {
    run(&h, move |l, d| {
        let attempt = l.disable(&current, now_ms())?;
        Ok(AttemptView { attempt, lock: view(l, d) })
    })
    .await
}

#[tauri::command(async)]
pub fn lock_set_options(h: State<'_, LockHandle>, timeout_secs: u32, biometric: bool, hide_in_switcher: bool) -> Res<LockView> {
    with(&h, |l, d| {
        l.set_options(timeout_secs, biometric, hide_in_switcher).map_err(|e| match e {
            selfkit_core::Error::Format(m) => m,
            other => other.to_string(),
        })?;
        Ok(view(l, d))
    })
}

/// The app is back after `ms` away (Android: stopped → started, measured in Kotlin;
/// desktop: unfocused or idle, measured in the page).
#[tauri::command(async)]
pub fn lock_away(h: State<'_, LockHandle>, ms: i64) -> Res<LockView> {
    Ok(with(&h, |l, d| {
        l.away(ms);
        view(l, d)
    }))
}

#[tauri::command(async)]
pub fn lock_now(h: State<'_, LockHandle>) -> Res<LockView> {
    Ok(with(&h, |l, d| {
        l.lock_now();
        view(l, d)
    }))
}

/// "Forgot PIN?": turn the lock off with the Selfkit encryption passphrase or recovery
/// key, checked against this device's keys.json (no network). Only where encryption is
/// set up; otherwise the docs explain the manual reset.
#[tauri::command]
pub async fn lock_reset(h: State<'_, LockHandle>, secret: String, recovery: bool) -> Res<LockView> {
    run(&h, move |l, d| {
        let kf = KeyFile::load(&d.join("keys.json")).map_err(|_| selfkit_core::Error::Format("Encryption isn't set up on this device.".into()))?;
        // Same counter and waits as the PIN, so this isn't an unlimited guessing oracle
        // for the encryption passphrase (C6).
        let wait = l.retry_in_ms(now_ms());
        if wait > 0 {
            return Err(selfkit_core::Error::Format(format!("Too many tries. Try again in {}.", wait_text(wait))));
        }
        let ok = if recovery { kf.unlock_recovery(secret.trim()) } else { kf.unlock_passphrase(&secret) };
        if ok.is_err() {
            let wrong = if recovery { "That recovery key doesn't match." } else { "Wrong passphrase." };
            return Err(selfkit_core::Error::Format(match l.record_failure(now_ms())? {
                Attempt::Wrong { retry_in_ms, .. } if retry_in_ms > 0 => format!("{wrong} Try again in {}.", wait_text(retry_in_ms)),
                _ => wrong.to_string(),
            }));
        }
        l.reset()?;
        Ok(view(l, d))
    })
    .await
}
