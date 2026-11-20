# selfkit-agent

Desktop background service, no UI. Status: **usable on Linux**: tracking, Chrono settings, browser native host, sync, LAN sync, backups and restore, Agenda reminders, health, IPC, installers. On Windows it is built and tested on GitHub Actions (unit tests, installer, autostart, native host, IPC `status`); installers: [../platforms/windows.md](../platforms/windows.md), Linux packages: [../platforms/linux.md](../platforms/linux.md). Measured (debug build, in use): about 25 MB RSS and ~0.2 % CPU. Release build: see STATUS.

## What it does
- **Activity tracking** (X11 / Win32): samples the foreground app and idle time every 1 s and writes `app_sessions` to `chrono.db` (`{app, platform, start, end, tz_offset}`), flushed every 60 s. Idle (≥120 s) back-dates the end. Gaps over 15 s (suspend) close the session. Only app names are read, never window titles. If X isn't there yet (service started before X), it retries every 10 s.
- **Chrono settings** (`chrono.db` → `chrono_settings` / id `settings`, owned by the Chrono app): re-read every 10 s. `paused_until` (UTC ms) pauses tracking. `exclude_apps` (lowercase app names) are never recorded. `exclude_domains` apply to browser sessions (an entry also matches its subdomains). Unknown fields are ignored. A missing record means no pause and no excludes.
- **Pause:** `selfkit-agent pause|resume` pauses this agent in memory, and the browser extension follows it. Chrono's `paused_until` pauses everything on every device that syncs it.
- **Browser native-messaging host** (`com.selfkit.agent`): writes `web_sessions`. See [browser-extension.md](browser-extension.md).
- **Sync** of every app DB to Postgres every 5 min (`SELFKIT_SYNC_SECS`) and on `sync`. Also records the server DB size for the health screen.
- **LAN sync** with your other devices ([../lan-sync.md](../lan-sync.md)).
- **Backups** (monthly archive per app plus a weekly full export, on S3 and optionally kept locally) and **restore**. See below.
- **Reminders:** desktop notifications for Agenda tasks while the Agenda app is closed. See below.
- **Health:** local diagnostics, available as the IPC `health` command and as `<data_dir>/health.json`. See below.
- **IPC:** Unix socket `$XDG_RUNTIME_DIR/selfkit-agent.sock` (mode 0600) / named pipe `\\.\pipe\selfkit-agent`. JSON lines: `{"cmd":"status"|"pause"|"resume"|"sync_now"|"today"|"health"}`, `{"cmd":"backup_now","full":true}`.

## CLI
```
selfkit-agent [run]                  run the agent
selfkit-agent status | health | today | pause | resume | sync
selfkit-agent backup [--full]        run due backups now (in-process; --full forces a full export)
selfkit-agent restore --list         list bundles on S3
selfkit-agent restore [--app A]      restore from S3 into the data folder
selfkit-agent restore --file F.skb   import one bundle
selfkit-agent native-host            native-messaging host (browsers start it themselves)
selfkit-agent install-native-host [--path EXE] [--extension-id ID]… [--chrome-profile DIR]… [--uninstall]
selfkit-agent install-service [--enable] [--uninstall]
selfkit-agent set-secret pg|s3       store the Postgres URL / S3 JSON in the OS keychain (stdin)
selfkit-agent init                   first key setup (passphrase on stdin, prints the recovery key)
selfkit-agent unlock [--recovery]    store the data key in the keychain after a passphrase/recovery check
```
`health` asks the running agent, or falls back to `health.json`. When a browser starts the binary (argument `chrome-extension://…`, or a manifest path plus an add-on id), it switches to native-host mode by itself. Its stdout is the messaging channel there, so all logging goes to stderr.

Env: `SELFKIT_DATA_DIR`, `SELFKIT_PG_URL` (else keychain), `SELFKIT_DATA_KEY` (base64; dev/test only), `SELFKIT_S3_ENDPOINT/_BUCKET/_REGION/_KEY/_SECRET[/_PATH_STYLE=0]` (dev/test; else keychain `s3-config`), `SELFKIT_BACKUP=0`, `SELFKIT_BACKUP_KEEP_LOCAL=1`, `SELFKIT_BACKUP_DELAY_SECS` (first check after start, default 120), `SELFKIT_REMINDERS=0`, LAN vars, `RUST_LOG`.

Config file (optional, non-secret) `<data_dir>/agent.json`:
```json
{ "backup": { "enabled": true, "keep_local": false, "weekly_keep": 4, "local_weekly_keep": 2 },
  "reminders": { "enabled": true },
  "server_db_limit_mb": 512 }
```

## Backups
- **When:** the first check runs 2 min after start, then hourly, plus on `backup_now` / `selfkit-agent backup`. It needs the data key (it is skipped while locked) and a target: S3, or `keep_local`. With neither, health shows the error.
- **Monthly archive** per app (and `settings`): records whose `updated_at` falls in each finished **UTC** month (tombstones included), plus attachments in `<data_dir>/files/<app>/` modified that month. The first run back-fills every month since the oldest record. Months with no changes are marked done but nothing is uploaded. S3 key: `bundles/<app>/monthly/<YYYY-MM>-<dev8>.skb` (`dev8` = last 8 characters of the device id, so two PCs never overwrite each other).
- **Weekly full export** of every app (all records + all attachments), once per ISO week (UTC): `bundles/<app>/weekly/<YYYY-Www>-<dev8>.skb`. The same run uploads `bundles/_keys/keys.json`, the key file wrapped by passphrase and recovery key, so a fresh device can unlock with the recovery key.
- **Retention:** monthly archives are kept forever (they are small, and they are the archive). Weekly exports: the newest `weekly_keep` (4) per app per device on S3. Older ones are deleted, by `BlobStore::delete_bundle`, which only accepts `bundles/` keys. Local copies (`<data_dir>/backups/<app>/<kind>/`, only with `keep_local`): newest `local_weekly_keep` (2) weekly, every monthly.
- **Bundles** are the core `.skb` format (encrypted with the data key; see [../data-and-sync.md](../data-and-sync.md)). They are built in `<data_dir>/backups/tmp/` with flat RAM and uploaded with streaming.
- **State:** `<data_dir>/backups/state.json`: `{monthly: {app: "YYYY-MM"}, weekly: "YYYY-Www", last_run_at, last_ok_at, last_error, target, last_bundles: [{app, kind, label, path, records, bytes}]}`.
- **Restore:** `restore` downloads `keys.json` if it is missing locally. It then imports, per app, the newest weekly export (any device) and every monthly archive. Import is "newer wins", so order doesn't matter and running it again changes nothing. Restored records are marked dirty, and the server pull adopts records that are already the same. Fresh device: `SELFKIT_DATA_DIR=<new> selfkit-agent restore` → if locked: `selfkit-agent unlock --recovery`, then run `restore` again.

## Reminders
Every 20 s the agent reads `agenda.db` (read-only) for `tasks` that have `reminder_min`, a `start` and no `done_at`. Fire time = start − reminder_min, local time. All-day tasks use 09:00, the same rule as Agenda's `reminder_at`. A reminder fires once when its time passes, including one missed by up to 30 min while the agent was off. Fired reminders are stored in `<data_dir>/agent-reminders.json` (`{last_check, fired: {task_id: fire_ms}}`, pruned after 2 days). While an `agenda` process is running, the agent shows nothing, because the app's own timer does it. Linux: `notify-send -a Agenda`. Windows: a toast via PowerShell. Turn off with `reminders.enabled=false` or `SELFKIT_REMINDERS=0`. Later: Hisab loan due dates and Dusk bedtime work the same way.

## Health
`health.json` is rewritten every 60 s (atomically), and the IPC `{"cmd":"health"}` returns `{"ok":true,"health":{…}}` with the same document. Nothing leaves the device. Shape (schema 1):
```jsonc
{
  "schema": 1, "written_at": 0, "version": "0.1.0", "device_id": "…",
  "agent":    { "pid": 1, "started_at": 0, "uptime_secs": 0, "rss_kib": 0, "cpu_percent": 0.2, "cpu_secs_total": 0.16 },
  "tracking": { "on": true, "paused": false, "paused_until": null, "current_app": "firefox", "excluded_apps": 0, "error": null },
  "web":      { "last_session_end": { "chrome": 0, "firefox": 0 } },          // newest web_sessions end per browser
  "sync":     { "configured": true, "last_at": 0, "last_ok_at": 0, "last_error": null, "last_report": {"pulled":0,"pushed":0,"conflicts":0,"undecryptable":0},
                "pending_records": 0, "server_db_bytes": 0, "server_db_limit_bytes": 536870912, "server_db_warn": false },   // warn at ≥ 80 %
  "lan":      { "enabled": true, "error": null, "peers": 0 },
  "backup":   { "enabled": true, "keep_local": false, "target": "s3", "last_run_at": 0, "last_ok_at": 0, "last_error": null,
                "last_weekly": "2026-W46", "last_monthly": { "chrono": "2026-10" } },
  "reminders":{ "enabled": true, "last_fired_at": null, "fired_since_start": 0 },
  "storage":  { "data_dir": "…", "total_bytes": 0, "apps": { "chrono": 0, "hisab": 0, "agenda": 0, "reflect": 0, "dusk": 0, "settings": 0 },
                "files_bytes": 0, "backups_bytes": 0, "logs_bytes": 0 },     // apps = .db + -wal + -shm
  "errors":   [ { "at": 0, "level": "WARN", "source": "selfkit_agent::backup", "message": "…", "pid": 1 } ]   // newest first, max 20
}
```
All times are UTC ms. `cpu_percent` is the share of one core since the previous snapshot (null on the first one). **Error log:** every WARN/ERROR from every agent mode, the native host included, is appended to `<data_dir>/logs/agent-errors.log` as JSON lines `{at, level, source, message, pid}`. It rotates to `.log.1` at 512 KiB.

## Not yet
Tray icon. Windows activity tracking checked by hand on a real PC (CI only checks that the agent starts). On Windows the installer uses an HKCU Run key + hidden launcher instead of `install-service`'s logon task (that needs admin); `install-service` writes the same Run key value (C28). A settings UI for agent.json (Reflect/Settings). Hisab/Dusk reminders.

## Code map
- `src/main.rs`: CLI, run loop, tracker thread (Chrono settings every 10 s, probe retry), backup loop, key init/unlock.
- `src/chrono_settings.rs`: `ChronoSettings` (pause, exclude lists), `normalize_domain`.
- `src/native_host.rs`: native-messaging framing, `web_sessions` validation/upsert, browser-launch detection.
- `src/install.rs`: host manifests (Linux files / Windows registry), systemd unit / Windows logon task, i3 snippet.
- `src/backup.rs`: S3 config, monthly/weekly bundles, retention, state, restore.
- `src/reminders.rs`: Agenda reminder scan, dedupe, notifications. A reminder in the spring-forward DST gap fires an hour later (C24).
- `src/health.rs`: error-log tracing layer, CPU/RSS, storage, health document + writer.
- `src/config.rs`: `agent.json` + env overrides.
- `src/tracker/mod.rs`, `x11.rs`, `win.rs`: activity probes.
- `src/sessions.rs`: samples → `app_sessions`; `totals_since`.
- `src/sync_loop.rs`: server sync loop (+ server DB size); `run_lan`.
- `src/ipc.rs`: socket/pipe server + client. `src/state.rs`: shared state, IPC handlers.
- `examples/`: dev helpers: `chrono_settings` (patch Chrono settings), `put_record` (write any record, e.g. an Agenda task), `seed_history` (months of chrono/hisab records for backup tests).
- `dist/selfkit-agent.service`: systemd user unit template.
