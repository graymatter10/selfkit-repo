# selfkit-agent

Desktop background service, no UI. Status: **usable on Linux** (tracking, sync, LAN sync, IPC). Windows code is written but not yet compiled or tested. Measured: 4.3 MB release binary, ~7–8 MB RAM, ~0% CPU.

## What it does
- Samples the foreground app + idle time every 1 s. Writes `app_sessions` records to `chrono.db` (`{app, platform, start, end, tz_offset}`), flushed every 60 s. Idle (≥120 s) back-dates the end; gaps >15 s (suspend) close the session. App names only, never window titles; exclude list + pause.
- Syncs every app DB to Postgres every 5 min (`SELFKIT_SYNC_SECS`) and on `sync`.
- LAN sync with your other devices ([../lan-sync.md](../lan-sync.md)).
- IPC: Unix socket `$XDG_RUNTIME_DIR/selfkit-agent.sock` / pipe `\\.\pipe\selfkit-agent`, JSON lines: `status, pause, resume, sync_now, today`.

## CLI
`selfkit-agent [run|status|pause|resume|sync|today|init|unlock|help]`. `init` creates `keys.json`, prints the recovery key and stores the data key in the keychain. `unlock` stores the data key after a passphrase check.
Env: `SELFKIT_DATA_DIR`, `SELFKIT_PG_URL` (else keychain), `SELFKIT_DATA_KEY` (dev/test only), LAN vars, `RUST_LOG`.

## Not yet
Browser native-messaging host, backups (monthly bundles to S3), reminders, tray icon, exclude-list config, systemd unit/installer.

## Code map
- `src/main.rs`: CLI, run loop, tracker thread, key init/unlock.
- `src/tracker/mod.rs`: `Probe` trait, platform selection.
- `src/tracker/x11.rs`: i3/X11 active window (`_NET_ACTIVE_WINDOW`, `WM_CLASS`) + screensaver idle.
- `src/tracker/win.rs`: Windows foreground exe + `GetLastInputInfo`.
- `src/sessions.rs`: samples → `app_sessions`; `totals_since`.
- `src/sync_loop.rs`: server sync loop; `run_lan`.
- `src/ipc.rs`: socket/pipe server + client.
- `src/state.rs`: shared state, status JSON, RSS diagnostics.
