//! App lock: a PIN or passphrase in front of an app's UI (docs/components/app-lock.md).
//!
//! - The secret is stored only as an Argon2id hash with a random salt, in a small
//!   local-only file (`<data_dir>/lock/<app>.json`). It is never synced or exported.
//! - Lock on start when enabled, and after the app was away (background / unfocused /
//!   idle) for at least the chosen time: Immediately, 1, 5 or 15 minutes.
//! - Wrong attempts: 4 free tries, then escalating waits (30 s … 1 h). Nothing is wiped.
//! - Changing or turning off the lock needs the current secret (and counts as an attempt).
//!
//! The lock protects the screen, not the data: the SQLite files stay readable to anyone
//! with file access (see docs/security.md). Time comes in from the caller (`now` in ms
//! since the epoch) so the rules are testable.

use std::path::{Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{Error, Result, crypto};

/// "Lock after" choices offered in the settings (seconds; 0 = immediately).
pub const TIMEOUTS: [u32; 4] = [0, 60, 300, 900];
/// Wrong attempts allowed before the first wait.
pub const FREE_ATTEMPTS: u32 = 4;
/// PIN length limits (digits).
pub const PIN_MIN: usize = 4;
pub const PIN_MAX: usize = 12;
/// Passphrase minimum length (characters).
pub const PASS_MIN: usize = 6;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SecretKind {
    #[default]
    Pin,
    Passphrase,
}

/// Argon2id hash of the secret. Parameters are stored so they can be raised later.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SecretHash {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
    pub salt: String,
    pub hash: String,
}

/// Argon2id cost: ~19 MiB, 2 passes (OWASP minimum). About 0.1–0.4 s on a low-end phone.
pub const KDF_M_KIB: u32 = 19 * 1024;
pub const KDF_T: u32 = 2;
pub const KDF_P: u32 = 1;
const HASH_LEN: usize = 32;
// The lock file is local, but treat its parameters as untrusted anyway.
const MAX_M_KIB: u32 = 256 * 1024;
const MAX_T: u32 = 16;
const MAX_P: u32 = 8;

fn argon(secret: &str, salt: &[u8], m_kib: u32, t: u32, p: u32) -> Result<Zeroizing<[u8; HASH_LEN]>> {
    if m_kib > MAX_M_KIB || t == 0 || t > MAX_T || p == 0 || p > MAX_P {
        return Err(Error::Format("app lock: KDF parameters out of range".into()));
    }
    let params = Params::new(m_kib, t, p, Some(HASH_LEN)).map_err(|e| Error::Format(e.to_string()))?;
    let mut out = Zeroizing::new([0u8; HASH_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(secret.as_bytes(), salt, out.as_mut_slice())
        .map_err(|e| Error::Format(e.to_string()))?;
    Ok(out)
}

/// Hash a secret with a fresh random salt (default cost).
pub fn hash_secret(secret: &str) -> Result<SecretHash> {
    hash_secret_with(secret, KDF_M_KIB, KDF_T, KDF_P)
}

pub fn hash_secret_with(secret: &str, m_kib: u32, t: u32, p: u32) -> Result<SecretHash> {
    let salt = crypto::random_bytes::<16>();
    let h = argon(secret, &salt, m_kib, t, p)?;
    Ok(SecretHash { m_kib, t, p, salt: B64.encode(salt), hash: B64.encode(h.as_slice()) })
}

/// Constant-time check of `secret` against a stored hash. Malformed hashes never match.
pub fn verify_secret(stored: &SecretHash, secret: &str) -> bool {
    let (Ok(salt), Ok(want)) = (B64.decode(&stored.salt), B64.decode(&stored.hash)) else { return false };
    let Ok(got) = argon(secret, &salt, stored.m_kib, stored.t, stored.p) else { return false };
    if want.len() != got.len() {
        return false;
    }
    want.iter().zip(got.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

/// Wait (seconds) after `failed` wrong attempts in a row. Escalates, never wipes.
pub fn backoff_secs(failed: u32) -> u64 {
    match failed {
        0..=FREE_ATTEMPTS => 0,
        5 => 30,
        6 => 60,
        7 => 5 * 60,
        8 => 15 * 60,
        9 => 30 * 60,
        _ => 60 * 60,
    }
}

/// Should the app lock after being away for `away_ms`? (`timeout_secs` 0 = immediately.)
pub fn should_lock(timeout_secs: u32, away_ms: i64) -> bool {
    // A negative duration means the clock moved backwards: lock to be safe.
    away_ms < 0 || away_ms >= i64::from(timeout_secs) * 1000
}

/// Is this a valid new secret of that kind? Returns a short reason when not.
pub fn check_new_secret(kind: SecretKind, secret: &str) -> std::result::Result<(), String> {
    match kind {
        SecretKind::Pin => {
            let n = secret.chars().count();
            if !secret.chars().all(|c| c.is_ascii_digit()) {
                return Err("A PIN has digits only.".into());
            }
            if !(PIN_MIN..=PIN_MAX).contains(&n) {
                return Err(format!("Use {PIN_MIN} to {PIN_MAX} digits."));
            }
            Ok(())
        }
        SecretKind::Passphrase => {
            let n = secret.chars().count();
            if n < PASS_MIN {
                return Err(format!("Use at least {PASS_MIN} characters."));
            }
            if n > 256 {
                return Err("That passphrase is too long.".into());
            }
            Ok(())
        }
    }
}

/// What is stored on disk (local only).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct LockFile {
    pub version: u32,
    /// None = lock off.
    pub secret: Option<SecretHash>,
    pub kind: SecretKind,
    /// PIN length, so the pad can open as soon as the last digit is typed (0 = passphrase).
    pub pin_len: u8,
    /// Lock after this long away (0 = immediately).
    pub timeout_secs: u32,
    /// Android: offer fingerprint / face (BiometricPrompt) on the lock screen.
    pub biometric: bool,
    /// Android: FLAG_SECURE (blank app-switcher preview, no screenshots).
    pub hide_in_switcher: bool,
    /// Wrong attempts in a row.
    pub failed: u32,
    /// No attempt before this time (ms since epoch).
    pub retry_at: i64,
}

impl Default for LockFile {
    fn default() -> Self {
        Self { version: 1, secret: None, kind: SecretKind::Pin, pin_len: 0, timeout_secs: 60, biometric: true, hide_in_switcher: false, failed: 0, retry_at: 0 }
    }
}

/// What the UI needs (no hashes).
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct LockStatus {
    pub enabled: bool,
    pub locked: bool,
    pub kind: SecretKind,
    pub pin_len: u8,
    pub timeout_secs: u32,
    pub biometric: bool,
    pub hide_in_switcher: bool,
    pub failed: u32,
    /// Milliseconds until the next attempt is allowed (0 = now).
    pub retry_in_ms: u64,
}

/// Result of checking a secret.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Attempt {
    Ok,
    /// Wrong secret. `retry_in_ms` > 0 when a wait starts now.
    Wrong { failed: u32, retry_in_ms: u64 },
    /// Too many wrong attempts: try again later (the secret was not checked).
    Wait { retry_in_ms: u64 },
    /// No lock is set.
    NotSet,
}

/// One app's lock: the file plus the in-memory "locked" flag.
pub struct AppLock {
    path: PathBuf,
    file: LockFile,
    locked: bool,
}

impl AppLock {
    /// `<data_dir>/lock/<app>.json`.
    pub fn path_for(data_dir: &Path, app: &str) -> PathBuf {
        data_dir.join("lock").join(format!("{app}.json"))
    }

    /// Load (a missing file = lock off). Starts locked when a lock is set.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let file = match std::fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
                // Fail open: an unreadable lock file must not lock the user out for good
                // (the lock is a screen guard, not encryption). Keep the bad file.
                tracing::warn!(path = %path.display(), error = %e, "app lock file unreadable; lock off");
                let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                LockFile::default()
            }),
            Err(_) => LockFile::default(),
        };
        let locked = file.secret.is_some();
        Self { path, file, locked }
    }

    pub fn file(&self) -> &LockFile {
        &self.file
    }

    pub fn enabled(&self) -> bool {
        self.file.secret.is_some()
    }

    pub fn locked(&self) -> bool {
        self.locked && self.enabled()
    }

    /// Remaining wait, capped by the current step so a clock set backwards can't
    /// stretch it.
    pub fn retry_in_ms(&self, now: i64) -> u64 {
        let left = (self.file.retry_at - now).max(0) as u64;
        left.min(backoff_secs(self.file.failed) * 1000)
    }

    pub fn status(&self, now: i64) -> LockStatus {
        LockStatus {
            enabled: self.enabled(),
            locked: self.locked(),
            kind: self.file.kind,
            pin_len: self.file.pin_len,
            timeout_secs: self.file.timeout_secs,
            biometric: self.file.biometric,
            hide_in_switcher: self.file.hide_in_switcher,
            failed: self.file.failed,
            retry_in_ms: self.retry_in_ms(now),
        }
    }

    fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.file)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }

    /// Check a secret, with backoff. Saves the attempt counter.
    pub fn check(&mut self, secret: &str, now: i64) -> Result<Attempt> {
        let Some(stored) = self.file.secret.clone() else { return Ok(Attempt::NotSet) };
        let wait = self.retry_in_ms(now);
        if wait > 0 {
            return Ok(Attempt::Wait { retry_in_ms: wait });
        }
        if verify_secret(&stored, secret) {
            if self.file.failed != 0 || self.file.retry_at != 0 {
                self.file.failed = 0;
                self.file.retry_at = 0;
                self.save()?;
            }
            return Ok(Attempt::Ok);
        }
        self.file.failed = self.file.failed.saturating_add(1);
        let secs = backoff_secs(self.file.failed);
        self.file.retry_at = if secs > 0 { now + (secs as i64) * 1000 } else { 0 };
        self.save()?;
        Ok(Attempt::Wrong { failed: self.file.failed, retry_in_ms: secs * 1000 })
    }

    /// Unlock with the secret.
    pub fn unlock(&mut self, secret: &str, now: i64) -> Result<Attempt> {
        let a = self.check(secret, now)?;
        if a == Attempt::Ok {
            self.locked = false;
        }
        Ok(a)
    }

    /// Unlock after the platform confirmed the user (Android BiometricPrompt).
    /// Only when the lock is set and biometrics are allowed; resets the counter.
    pub fn unlock_biometric(&mut self) -> Result<bool> {
        if !self.enabled() || !self.file.biometric {
            return Ok(false);
        }
        self.locked = false;
        if self.file.failed != 0 || self.file.retry_at != 0 {
            self.file.failed = 0;
            self.file.retry_at = 0;
            self.save()?;
        }
        Ok(true)
    }

    /// Test builds only (the caller decides): open without a secret.
    pub fn unlock_unchecked(&mut self) {
        self.locked = false;
    }

    /// Turn the lock on (only while it is off). Starts unlocked.
    pub fn enable(&mut self, kind: SecretKind, secret: &str) -> Result<()> {
        if self.enabled() {
            return Err(Error::Format("The lock is already on. Change it instead.".into()));
        }
        check_new_secret(kind, secret).map_err(Error::Format)?;
        self.file.secret = Some(hash_secret(secret)?);
        self.set_kind(kind, secret);
        self.file.failed = 0;
        self.file.retry_at = 0;
        self.locked = false;
        self.save()
    }

    /// New secret; needs the current one.
    pub fn change(&mut self, current: &str, kind: SecretKind, secret: &str, now: i64) -> Result<Attempt> {
        check_new_secret(kind, secret).map_err(Error::Format)?;
        let a = self.check(current, now)?;
        if a == Attempt::Ok {
            self.file.secret = Some(hash_secret(secret)?);
            self.set_kind(kind, secret);
            self.save()?;
        }
        Ok(a)
    }

    fn set_kind(&mut self, kind: SecretKind, secret: &str) {
        self.file.kind = kind;
        self.file.pin_len = if kind == SecretKind::Pin { secret.len().min(PIN_MAX) as u8 } else { 0 };
    }

    /// "Forgot PIN": turn the lock off after the caller verified the user another way
    /// (the Selfkit encryption passphrase or recovery key, see lock_cmds).
    pub fn reset(&mut self) -> Result<()> {
        self.file.secret = None;
        self.file.pin_len = 0;
        self.file.failed = 0;
        self.file.retry_at = 0;
        self.locked = false;
        self.save()
    }

    /// Turn the lock off; needs the current secret. Keeps the other options.
    pub fn disable(&mut self, current: &str, now: i64) -> Result<Attempt> {
        let a = self.check(current, now)?;
        if a == Attempt::Ok {
            self.file.secret = None;
            self.file.failed = 0;
            self.file.retry_at = 0;
            self.locked = false;
            self.save()?;
        }
        Ok(a)
    }

    /// Options that don't need the secret (the app is open and unlocked when they change).
    pub fn set_options(&mut self, timeout_secs: u32, biometric: bool, hide_in_switcher: bool) -> Result<()> {
        if !TIMEOUTS.contains(&timeout_secs) {
            return Err(Error::Format("Choose Immediately, 1, 5 or 15 minutes.".into()));
        }
        if self.locked() {
            return Err(Error::Format("Unlock first.".into()));
        }
        self.file.timeout_secs = timeout_secs;
        self.file.biometric = biometric;
        self.file.hide_in_switcher = hide_in_switcher;
        self.save()
    }

    /// The app came back after being away (background, unfocused or idle) for `away_ms`.
    /// Returns true when it is now locked.
    pub fn away(&mut self, away_ms: i64) -> bool {
        if self.enabled() && should_lock(self.file.timeout_secs, away_ms) {
            self.locked = true;
        }
        self.locked()
    }

    /// Lock right now (when a lock is set).
    pub fn lock_now(&mut self) {
        self.locked = self.enabled();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = AppLock::path_for(d.path(), "agenda");
        (d, p)
    }

    #[test]
    fn hash_and_verify() {
        let h = hash_secret("2468").unwrap();
        assert!(verify_secret(&h, "2468"));
        assert!(!verify_secret(&h, "2469"));
        assert!(!verify_secret(&h, ""));
        // Fresh salt every time.
        let h2 = hash_secret("2468").unwrap();
        assert_ne!(h.salt, h2.salt);
        assert_ne!(h.hash, h2.hash);
        assert!(verify_secret(&h2, "2468"));
        // The stored form never contains the secret.
        let json = serde_json::to_string(&h).unwrap();
        assert!(!json.contains("2468"));
        assert_eq!((h.m_kib, h.t, h.p), (KDF_M_KIB, KDF_T, KDF_P));
    }

    #[test]
    fn malformed_or_hostile_hash_never_matches() {
        let mut h = hash_secret_with("1234", 1024, 1, 1).unwrap();
        h.salt = "not base64!".into();
        assert!(!verify_secret(&h, "1234"));
        let mut h = hash_secret_with("1234", 1024, 1, 1).unwrap();
        h.m_kib = 4 * 1024 * 1024; // 4 GiB: refused, not allocated
        assert!(!verify_secret(&h, "1234"));
    }

    #[test]
    fn backoff_schedule_escalates_and_caps() {
        for f in 0..=FREE_ATTEMPTS {
            assert_eq!(backoff_secs(f), 0, "attempt {f} is free");
        }
        assert_eq!(backoff_secs(5), 30);
        assert_eq!(backoff_secs(6), 60);
        assert_eq!(backoff_secs(7), 300);
        assert_eq!(backoff_secs(8), 900);
        assert_eq!(backoff_secs(9), 1800);
        assert_eq!(backoff_secs(10), 3600);
        assert_eq!(backoff_secs(u32::MAX), 3600);
        let mut prev = 0;
        for f in 0..20 {
            assert!(backoff_secs(f) >= prev);
            prev = backoff_secs(f);
        }
    }

    #[test]
    fn timeout_rules() {
        // Immediately: any time away locks.
        assert!(should_lock(0, 0));
        assert!(should_lock(0, 1));
        // 1 minute.
        assert!(!should_lock(60, 59_999));
        assert!(should_lock(60, 60_000));
        // 15 minutes.
        assert!(!should_lock(900, 14 * 60_000));
        assert!(should_lock(900, 15 * 60_000));
        // Clock moved backwards: lock.
        assert!(should_lock(900, -5));
    }

    #[test]
    fn secret_rules() {
        assert!(check_new_secret(SecretKind::Pin, "1234").is_ok());
        assert!(check_new_secret(SecretKind::Pin, "123").is_err());
        assert!(check_new_secret(SecretKind::Pin, "12a4").is_err());
        assert!(check_new_secret(SecretKind::Pin, "1234567890123").is_err());
        assert!(check_new_secret(SecretKind::Passphrase, "abcde").is_err());
        assert!(check_new_secret(SecretKind::Passphrase, "abc def").is_ok());
    }

    #[test]
    fn lifecycle_with_backoff_and_persistence() {
        let (_d, p) = tmp();
        let t0 = 1_000_000;
        let mut l = AppLock::open(&p);
        assert!(!l.enabled() && !l.locked());
        assert!(!l.away(10_000_000), "no lock set: never locks");

        l.enable(SecretKind::Pin, "4826").unwrap();
        assert!(l.enabled() && !l.locked(), "starts unlocked right after setting it");
        assert_eq!(l.status(t0).pin_len, 4);
        assert!(l.enable(SecretKind::Pin, "1111").is_err(), "enable twice refused");
        let raw = std::fs::read_to_string(&p).unwrap();
        assert!(!raw.contains("4826"));

        // Default timeout is 1 minute.
        assert!(!l.away(30_000));
        assert!(l.away(61_000));

        // Four free wrong tries, then waits.
        for i in 1..=FREE_ATTEMPTS {
            assert_eq!(l.unlock("0000", t0).unwrap(), Attempt::Wrong { failed: i, retry_in_ms: 0 });
        }
        assert_eq!(l.unlock("0000", t0).unwrap(), Attempt::Wrong { failed: 5, retry_in_ms: 30_000 });
        // Even the right PIN waits.
        assert_eq!(l.unlock("4826", t0 + 10_000).unwrap(), Attempt::Wait { retry_in_ms: 20_000 });
        assert!(l.locked());

        // Restart: still locked, counter kept.
        let mut l = AppLock::open(&p);
        assert!(l.locked());
        assert_eq!(l.status(t0 + 10_000).failed, 5);
        assert_eq!(l.status(t0 + 10_000).retry_in_ms, 20_000);
        // Clock set back a day: the wait is capped by the current step.
        assert_eq!(l.retry_in_ms(t0 - 86_400_000), 30_000);

        assert_eq!(l.unlock("4826", t0 + 30_000).unwrap(), Attempt::Ok);
        assert!(!l.locked());
        assert_eq!(l.status(t0 + 30_000).failed, 0);

        // Change needs the current PIN.
        assert_eq!(l.change("9999", SecretKind::Passphrase, "long secret", t0).unwrap(), Attempt::Wrong { failed: 1, retry_in_ms: 0 });
        assert_eq!(l.change("4826", SecretKind::Passphrase, "long secret", t0).unwrap(), Attempt::Ok);
        assert_eq!(l.file().kind, SecretKind::Passphrase);
        assert_eq!(l.status(t0).pin_len, 0);
        assert!(l.change("long secret", SecretKind::Pin, "12", t0).is_err(), "bad new PIN refused before checking");

        // Options.
        l.set_options(0, false, true).unwrap();
        assert!(l.set_options(120, false, true).is_err());
        assert!(l.away(0), "immediately");
        assert!(!l.unlock_biometric().unwrap(), "biometric off: refused");
        assert!(l.set_options(300, true, true).is_err(), "locked: unlock first");
        assert_eq!(l.unlock("long secret", t0).unwrap(), Attempt::Ok);
        l.set_options(300, true, true).unwrap();
        l.lock_now();
        assert!(l.unlock_biometric().unwrap());
        assert!(!l.locked());

        // Disable needs the current secret; keeps options.
        assert_eq!(l.disable("nope", t0).unwrap(), Attempt::Wrong { failed: 1, retry_in_ms: 0 });
        assert_eq!(l.disable("long secret", t0).unwrap(), Attempt::Ok);
        assert!(!l.enabled());
        let l = AppLock::open(&p);
        assert!(!l.enabled() && !l.locked());
        assert_eq!(l.file().timeout_secs, 300);
        assert!(l.file().hide_in_switcher);
    }

    #[test]
    fn reset_turns_the_lock_off() {
        let (_d, p) = tmp();
        let mut l = AppLock::open(&p);
        l.enable(SecretKind::Pin, "135790").unwrap();
        for _ in 0..6 {
            l.unlock("000000", 0).unwrap();
        }
        l.lock_now();
        l.reset().unwrap();
        assert!(!l.enabled() && !l.locked());
        let l = AppLock::open(&p);
        assert!(!l.enabled());
        assert_eq!(l.status(0).failed, 0);
    }

    #[test]
    fn corrupt_file_fails_open() {
        let (_d, p) = tmp();
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"{not json").unwrap();
        let l = AppLock::open(&p);
        assert!(!l.enabled() && !l.locked());
        assert!(p.with_extension("json.bad").exists());
    }
}
