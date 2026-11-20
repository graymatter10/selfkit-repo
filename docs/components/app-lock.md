# App lock

A PIN or passphrase in front of every app's screen (PLANNING.md → Security: "App lock: PIN or fingerprint on every app"). Shared by all five apps: one Rust module, one set of Tauri commands, one Svelte lock screen + settings section, one small Kotlin helper per app.

## Behaviour

- **Off by default.** Settings → App lock → toggle on → choose a PIN (4–12 digits) or a passphrase (6+ characters), typed twice. On a phone the PIN sheet keeps the same height on every step (message, button and switch rows keep their space), so the pad doesn't jump at "Repeat the PIN" (B3).
- **When it locks:** always when the app starts (cold start, process killed), and when the app comes back after being away for at least the chosen time: **Immediately / 1 min / 5 min / 15 min** (default 1 min).
  - Android: "away" = activity stopped → started, measured in Kotlin with `SystemClock.elapsedRealtime()` (counts deep sleep, can't be moved by changing the clock). Opening the camera/gallery from the app counts as away too (with "Immediately" you unlock again after taking a photo; nothing is lost, the lock only covers the screen).
  - Desktop: window unfocused or hidden (blur / `visibilitychange`), and idle (no pointer/keyboard input) for the chosen time, at least 1 minute.
- **Lock screen:** covers everything (fixed, opaque, every other `body` child gets `inert`). Phone + PIN: own number pad (rounded keys, no letters), dots, auto-unlock on the last digit (the PIN length is stored). Desktop or passphrase: one field + Unlock, Enter submits; a PIN also opens on its last digit.
- **Wrong attempts:** 4 free tries, then waits of 30 s, 1 min, 5 min, 15 min, 30 min, then 1 h per try. Nothing is ever wiped. The counter and the "retry at" time are stored, so restarting the app doesn't reset them; a clock set backwards can't stretch a wait beyond its step. While a wait runs, the lock screen shows a live countdown ("Too many tries. Try again in 28 s."); afterwards only "Wrong PIN." stays, never a frozen "Try again in 30 s." (phone bug B5).
- **Fingerprint (Android):** "Unlock with fingerprint" (on by default once a lock is set). BiometricPrompt with `BIOMETRIC_STRONG | DEVICE_CREDENTIAL` (Android 11+; the phone's screen lock is the fallback), `BIOMETRIC_STRONG` + "Use PIN" button before that. The prompt opens by itself when a strong biometric is enrolled; with only a screen lock there is a button on the pad. Success also resets the wrong-attempt counter.
- **Hide content in app switcher (Android):** `FLAG_SECURE` (blank recent-apps preview; also blocks screenshots). Remembered in SharedPreferences and applied in `onCreate`, before the first frame. Independent of the lock being on.
- **Change / turn off:** both need the current PIN/passphrase (and count as attempts). "Lock after", fingerprint and app-switcher options change without it (the app is open and unlocked).
- **Forgot PIN?** If sync encryption is set up on the device (`keys.json`), the Selfkit encryption passphrase or recovery key turns the lock off (checked locally against `keys.json`, no network). Otherwise: Android → clear the app's storage (loses unsynced data); desktop → close the app and delete `<data dir>/lock/<app>.json`.
- **Quick-capture popups** (Agenda/Hisab/Dusk desktop `?view=capture`) are not gated: they only add.

## Test builds: `lock-test`

Every app has a cargo feature `lock-test` (in `default`, like `sync-test-key`) → `selfkit-appsync/lock-test`. In such builds the lock works normally, but the lock screen shows "Test build · opens by itself in 5 s. Tap a key to stay locked." and then calls `lock_test_unlock`; the fingerprint prompt is not opened by itself. So automated phone tests never get stuck, and a person can still test the lock (any key stops the countdown).

Release builds: `release.yml` calls android/linux/windows workflows with `release: true` → `SELFKIT_RELEASE=1` at compile time → `selfkit_appsync::release_build()` → `lock_cmds::test_unlock_allowed()` is false whatever the features say (`lock_test_unlock` is refused, `test_build: false`). Local / `dev/ci-apk.sh` builds keep it. To build without it locally: `cargo build --no-default-features --features sync-test-key` (or none).

## Storage

`<data_dir>/lock/<app>.json` (desktop: the shared Selfkit data folder; Android: the app's files dir), mode 0600, written atomically. Never synced, never in bundles or backups (Android `allowBackup=false`).

```json
{ "version": 1, "secret": { "m_kib": 19456, "t": 2, "p": 1, "salt": "<b64 16 B>", "hash": "<b64 32 B>" },
  "kind": "pin", "pin_len": 4, "timeout_secs": 60, "biometric": true, "hide_in_switcher": false,
  "failed": 0, "retry_at": 0 }
```

- Argon2id, 19 MiB / 2 passes / 1 lane, random salt per hash, constant-time compare; parameters stored (and bounded on read) so they can be raised later.
- An unreadable file fails **open** (renamed to `.json.bad`, lock off): the lock is a screen guard, not encryption, and must not lock the user out for good.
- `secret: null` = lock off; options are kept when it is turned off.

**What it does not do:** encrypt the local data (SQLite stays plaintext, see security.md), or stop someone with file access / root / adb backup of a debuggable build. The lock state lives in the Rust process (`LockHandle`); other commands are not blocked while locked. A 4-digit PIN hash is brute-forceable offline by anyone who can read the file — they could read the database anyway.

## Code map

| What | Where |
|---|---|
| Rules: hash/verify, backoff, timeout, file, `AppLock` | `selfkit-core/src/applock.rs` |
| Tauri commands (`lock_status`, `lock_unlock`, `lock_unlock_biometric`, `lock_test_unlock`, `lock_enable`, `lock_change`, `lock_disable`, `lock_set_options`, `lock_away`, `lock_now`, `lock_reset`), `LockHandle`, `test_unlock_allowed()` | `selfkit-appsync/src/lock_cmds.rs` (feature `tauri`; feature `lock-test`) |
| UI ↔ Rust calls, Android bridge type, shared `lockState` | `ui/src/lock/api.svelte.ts` |
| Lock screen + away/idle tracking + FLAG_SECURE sync | `ui/src/lock/LockGate.svelte` |
| Settings section | `ui/src/lock/LockSettings.svelte` |
| Number pad | `ui/src/lock/PinPad.svelte` |
| Android helper (`window.SelfkitLock`: `takeAway`, `biometricStatus`, `authenticate`, `setSecure`; events `selfkit-lock-resume`, `selfkit-lock-auth`) | `apps/<app>/src-tauri/gen/android/app/src/main/java/com/selfkit/<app>/SelfkitLock.kt` (same file in every app, only the package differs) |
| Per-app hooks | `lib.rs` (`LockHandle::new` + command list), `MainActivity.kt` ("---- Selfkit lock" blocks: `onCreate`, `onStart`/`onStop`, `onWebViewCreate`), `app/build.gradle.kts` (`androidx.biometric:biometric:1.1.0`), `Cargo.toml` (`lock-test`), `App.svelte` (`<LockGate>`), settings screen (`<LockSettings>`), `app.css` (`.skl` token mapping) |

### Theming

Components are neutral and read `--lock-*` variables with fallbacks to the usual app tokens (`--accent`, `--bg`/`--page`, `--card`/`--surface`, `--sep`/`--line`, `--ink`, `--muted`, `--danger`). Each app maps its own in `app.css`:

```css
.skl { --lock-bg: var(--bg); --lock-surface: var(--card); --lock-line: var(--sep); --lock-key: var(--fill); --lock-key-down: var(--faint); }
```

Also: `--lock-accent`, `--lock-accent-soft`, `--lock-on-accent`, `--lock-ink`, `--lock-muted`, `--lock-danger`, `--lock-radius`.

### Adding it to an app

1. `Cargo.toml`: `lock-test = ["selfkit-appsync/lock-test"]` in `default`.
2. `lib.rs` setup: `app.manage(selfkit_appsync::lock_cmds::LockHandle::new("<app>", &paths::data_dir()));` and the eleven `lock_*` commands in `generate_handler!`.
3. Android: copy `SelfkitLock.kt` (change the package), the three MainActivity blocks, the biometric dependency.
4. `App.svelte`: `<LockGate appName="…" phone={…} />` (main window only). Settings: `<LockSettings appName="…" phone={…} />`. `app.css`: the `.skl` line.

## Tests

`cargo test -p selfkit-core applock` (8 tests): hash/verify (fresh salt, never stores the secret), hostile/malformed hashes never match (huge memory parameter refused), backoff schedule (free tries, escalation, cap, monotonic), timeout rules (immediately, boundaries, clock backwards), secret rules, full lifecycle with persistence (restart keeps the counter, clock set back capped, change/disable need the current secret, options, biometric gating), reset, corrupt file fails open.
