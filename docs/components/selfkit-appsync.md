# selfkit-appsync

App-side sync, shared by every app (and by a future companion app). Status: **built and tested; integrated in Chrono, Dusk, Agenda, Hisab, Reflect** (2026-11-15). Android glue: [android-plugins/selfkit-sync](../../android-plugins/selfkit-sync/README.md). UI: Sync settings section + conflict screen, mockups in the wave-2 design sheets.

## Safety defaults (2026-11-15)
- **Sync is off after install or upgrade** (`SyncSettings.enabled = false`): no pass, no background job, no LAN, and Sync now does nothing until the user taps **Turn on sync** (or the "Sync this app" switch) on the Sync page. Reason: every test build shares the public test key, so an installed app would otherwise sync real data with any test-key device on the Wi-Fi.
- With the test key active, the page shows a red "Test key — not private" banner and turning sync on asks a second time ("Set a passphrase" / "Turn on anyway").

## Keys across devices
- Passphrase: at least 12 characters, with a strength meter on the Sync page (the published `keys.json` allows offline guessing, so length matters).
- Setup (`setup_encryption`) writes `keys.json` (the data key wrapped by the passphrase and by the recovery key; never the raw key). Every server pass publishes it to `sk_meta.keyfile` (and to S3 `bundles/_keys/keys.json` when S3 is set, the agent's place) when it changed, so `change_passphrase` reaches the other devices too.
- `unlock` / `unlock_recovery` fetch the newest published `keys.json` (server meta, else S3, else the local copy). Over LAN a locked device can't get it (the LAN channel itself needs the key), so a new device needs the server once, or the key file copied.
- **From the test key to your own key:** setting up encryption on a device that used the test key resets its sync state (everything is re-uploaded with the new key) and marks `sync/rekey-from-test`. Its next server pass sees the server's key check is the test key's, deletes all `sk_records` rows (`PgRemote::wipe_records`) and claims the server with the new key. Other devices then report "encryption key doesn't match" → Unlock with the passphrase → they reset and re-upload their data. Photos are re-uploaded too (uploaded marks cleared). On Android each app has its own secret store, so each app is unlocked once.

## What one sync pass does
1. Gets the data key: secure storage → in test builds, the built-in **test key** (no passphrase prompts). Without a key → "Sync is locked".
2. Opens the app's DB and the shared `settings` DB (`<data_dir>/<app>.db`, `settings.db`).
3. **Picks the transport for this run** (`transport: auto | lan_only | server_only`):
   - **LAN** (if enabled): broadcasts one encrypted beacon, listens for `lan_discovery_ms` (2.5 s) for beacons and for inbound sessions, then connects to every reachable peer: discovered ones, remembered ones (seen in the last 14 days) and `lan_peers` from settings. One encrypted session each (selfkit-core `lan::session`, same protocol as the agent). Any session succeeded → transport = `lan`.
   - Otherwise **server**: Postgres (`sync_once` per DB) after two guards: schema check (a server set up by a newer Selfkit is refused) and key check (`sk_meta.keycheck`, sealed with the data key; a device with the wrong key stops before pushing anything; the first device writes it).
   - **Photos** only travel through S3: after a server pass, and also after a LAN pass when S3 is configured (a failure there is only a note).
4. Saves state: last attempt/success, transport, peer names, plain-language error, log of the last 20 passes, seen peers.

A pass is guarded by an in-process set plus a lock file `<data_dir>/sync/<app>.lock` (`pid time`); a lock left by a killed process (dead pid, or older than 15 min) is taken over. A pass times out after 15 min.

## API (Rust)
```rust
let cfg = AppSyncConfig::new("agenda", data_dir, secrets);   // + photos, test_key, default_owner, device_name, test_server
let sync = AppSync::new(cfg)?;
sync.status().await?            // SyncStatus { headline, owner, unlocked, test_key, last_transport, last_peers, last_error, pending (this device's own unsent edits), conflicts, peers, next_auto_at, ... }
sync.sync_now(Trigger::Manual | Open | Background).await   // SyncOutcome { ok, ran, transport, peers, report, error, note, ... }
sync.settings() / sync.set_settings(&SyncSettings)          // owner, transport, auto_sync, background_minutes, lan_enabled, lan_peers, device_name
sync.server_config() / set_server(Some(&ServerConfig)) / test_server(&cfg).await   // Err(plain sentence)
sync.setup_encryption(pass) -> recovery key · unlock(pass) · unlock_recovery(rk) · change_passphrase · lock
sync.conflicts()? -> Vec<ConflictView>   // A = this device, B = the other; fields that differ; line diff for long text; delete_vs_edit
sync.resolve(store, id, &Choice::KeepA | KeepB | Merge { fields: {k: "a"|"b"}, lines: {k: {idx..}} })?
sync.due(trigger) / sync.schedule()       // scheduler hook: Open at most once a minute, Background every ~15 min; schedule().enabled=false → cancel the job
```
Headlines: "Synced locally with Linux PC · 2 min ago", "Synced via server · 5 min ago", "Waiting to sync · 12 changes saved here", "Not synced · <plain error>", "Locked · unlock to sync", "Synced by the Selfkit companion", "Syncing…".

## In the apps (feature `tauri`)
- `tauri_cmds::default_handle(app, data_dir)`: platform secret store (Android: Keystore-sealed file, wrap key looked up lazily; desktop: keychain) + `AppSync`. `spawn_on_open` runs one pass at start on desktop (asks the agent when it owns sync).
- Commands added to each app's handler list (no plugin permissions needed): `sync_status` (status + settings + platform), `sync_now`, `sync_settings_set`, `sync_server_get` (no secrets) / `sync_server_set` / `sync_server_test` (empty password = keep the stored one), `sync_encryption_setup`, `sync_unlock` (passphrase or recovery key), `sync_change_passphrase`, `sync_conflicts`, `sync_resolve`, `sync_log`. After a pass that pulled data, `data-changed` is emitted.
- Conflict screen content: numbers compare by value (`5000` = `5000.0`, `selfkit_core::conflict::json_eq`); `id`, `created_at`, `updated_at`, `tz_offset` are never shown as choices; list/object fields show one readable item per line (`api.ts` `show`). Phone hosts must give the screen the remaining height with a flex column (not a `100dvh - header` calc), so the action bar stays above the navigation bar.
- The peer rows' green "Here" shows only while sync and LAN are on and the last pass didn't fail (S-1). A failed LAN pass that found no PC says "Waiting to sync · no Selfkit PC found on this network" instead of "no connection" (S-2). Known limit: LAN-only setups can't show unsent edits, because LAN passes don't clear the server `dirty` flag that `pending` counts.
- While sync is off, the "Sync on the same network" and "Automatic" switches show off and disabled ("Used when sync is on"); their stored values are kept (L4-13).
- Shared UI `ui/src/sync/`: `SyncSettings.svelte` (Sync page), `Conflicts.svelte` (conflict screen), `api.ts`. Neutral styles with `k-` prefixed classes; each app maps its tokens with `.sks, .skc { --sync-accent-soft, --sync-on-accent, --sync-surface, --sync-sunk, --sync-line, --sync-ink, --sync-muted, --sync-danger, --sync-ok, --sync-radius }` (defaults fall back to `--accent`, `--surface`, `--line`, `--ink`, `--muted`, `--danger`).
- Each app has a Cargo feature `sync-test-key` (default on in test builds) → `selfkit-appsync/test-key`.
- Desktop: the test key is used when the keychain has no data key or cannot be read, only while no `keys.json` exists.
- Every app registers `tauri_cmds::nav_guard()` (webviews stay on the app's own origins; the Android JS bridges are never handed to an outside page).
- App lock (`lock_cmds`, `lock_*` commands, shared UI `ui/src/lock/`, feature `lock-test`): see [app-lock.md](app-lock.md).

## Read-only mirrors (Reflect on Android)
`SelfkitMirror` (Kotlin) pulls `content://com.selfkit.<app>.provider/changes` page by page and the JNI methods `mirror_cursor` / `mirror_apply` store the records in `<data_dir>/<app>.db` with `apply_peer` (cursor in kv `mirror.cursor`). Enabled per app with manifest meta-data `com.selfkit.sync.MIRROR` (Reflect: `chrono,hisab,agenda,dusk`); runs on init, in the background job, and from the page via `window.SelfkitSync.mirror(apps)`. Never touches the app's own DB. Users: Reflect (`chrono,hisab,agenda,dusk`), Dusk (`agenda,hisab`, for Connections).

Shared settings (`pullSettings` → JNI `settings_adopt`, on init and when the app comes to the foreground): when a sibling's settings changed this app's, `<data_dir>/sync/settings-adopted` gets the time and the Android watcher thread (`spawn_on_open`) emits `data-changed`, so an open page shows the new currency/day start without a restart (C16).

Shared local tables: besides records, a provider serves whole local tables listed in `SHARED_TABLES` (`android.rs`; today only Dusk's `dusk_nights` result cache) at `/table?name=<table>` (one row: `sql`, `rows` as a JSON array; JNI `table_rows`). `SelfkitMirror` copies them after the records (`TABLES` in the Kotlin, JNI `mirror_table`: drop, recreate from the source's CREATE statement, insert all rows in one transaction). Phone-local, never synced (DECISIONS 2026-11-20, B2). An older app without `/table` is skipped.

## Sync owner
`SyncOwner::App` (Android default) runs passes itself. `Agent` (desktop default): `sync_now` asks the running selfkit-agent over its IPC socket and `status` merges the agent's state; with no agent running, the app syncs by itself (with a note). `Companion`: the app steps back (`sync_now` does nothing, `schedule().enabled = false`); a future companion syncs every app through the ContentProviders. The Sync page only offers Companion when the companion app (`com.selfkit.companion`) is installed (bridge `companionInstalled()`); if the owner is Companion without it, the page says "Not syncing · The Selfkit companion isn't installed" and offers "Let this app sync".

## Where things are stored (one place per device)
- Secrets (data key `data-key`, `postgres-url`, `s3-config`): `SecretStore`. Desktop: OS keychain, service `com.selfkit` (the same entries the agent uses, so one setup serves all apps). Android: `FileSecrets` at `<data_dir>/sync/secrets.json`, sealed with a 32-byte wrapping key kept encrypted by the Android Keystore (`SelfkitSecrets.wrapKey`). Tests: `MemorySecrets`.
- `keys.json` (wrapped data key) in the data folder, as before; `unlock` downloads it from S3 (`bundles/_keys/keys.json`, uploaded by the agent) when missing.
- Settings (non-secret, per device): `<data_dir>/sync/settings.json`. State per app: `<data_dir>/sync/state-<app>.json`.
- Photos: records of collection `photos` with a `file` field; bytes at `<data_dir>/<app>-photos/<file>` (Agenda, Hisab, Chrono already do this). S3 key `blobs/<record id>`, encrypted (STREAM). Local kv `appsync.blob.up.<id>` marks uploaded/downloaded files.

## Test key
Feature `test-key` (or `AppSyncConfig.test_key = true`): when no real key is stored, the data key is SHA-256 of a public constant. All test builds share it, so they sync with each other without prompts. It is **not private**: status shows `test_key: true` (red banner on the Sync page), and a real `setup_encryption` replaces it. Builds made by the release workflows (`SELFKIT_RELEASE=1` at compile time, `release_build()`) never use it, and a device that has `keys.json` never falls back to it (unreadable secret store = Locked). Locally built / `ci-apk.sh` APKs still have it. See [../security.md](../security.md#test-key-test-builds-only). `test_server` can carry a dev server for the same builds.

## Plain-language errors
`explain(&Error)`: cannot reach server, timeout, refused password, missing database, storage refused key / missing bucket, certificate problem, wrong encryption key (server key check or LAN handshake), newer server schema, different Selfkit version on the LAN peer, locked, nothing configured, no PC found, busy. Technical detail goes to `SyncOutcome.detail` and the log. Secure storage failures on a Linux desktop say to start a keyring (GNOME Keyring or KeePassXC's Secret Service). The server form is checked before secure storage is touched: Postgres required (unless removing a saved server), `postgres://` scheme, `https://` S3 endpoint with a bucket (L4).

## Tests
- Unit: secrets sealing, lock takeover, headlines, plain errors.
- `tests/scenarios.rs` (devices = temp data folders, peers = selfkit-core `lan::serve`, the agent's LAN service; server tests need Postgres + S3, `--include-ignored`): see the test matrix in the wave-2 report and [../data-and-sync.md](../data-and-sync.md).
- `dev/sync-e2e.sh`: process level with real `selfkit-agent` processes and the `sksync` test binary: two agents on one machine over LAN, kill -9 mid-push + resume, clock skew with `faketime`, transport switching with an agent that also syncs to the server, test-key auto-unlock.
- CI: `.github/workflows/sync-e2e.yml` (push to `test/sync-*`), plus an Android compile check (Rust JNI for aarch64 and the Kotlin plugin).

## Year-sized data (measured 2026-11-15, laptop, local Docker Postgres)
App seeds: Hisab `--year`, Agenda `--year`, Chrono default seed. `year_data_full_and_incremental_sync`:

| App | Records | DB | Server full push | Full pull | LAN full | Incremental (25 changes) LAN / push / pull |
| --- | --- | --- | --- | --- | --- | --- |
| Hisab | 2,051 | 1.1 MB | 17.0 s | 0.7 s | 0.6 s | 0.36 / 0.65 / 0.06 s |
| Agenda | 3,559 | 2.1 MB | 9.5 s | 1.1 s | 1.0 s | 0.60 / 0.55 / 0.06 s |
| Chrono | 22,961 | 12.7 MB | 60.8 s | 6.7 s | 6.1 s | 3.41 / 2.93 / 0.05 s |

Server table for all three: 16.3 MB (encrypted). The first push is slow because core pushes one row per round trip; incremental syncs are well under the 15-min period. A LAN pass no longer sends a peer the versions it wrote itself (`serve_phase` skips `device_id == peer`; the phone's first push of 6458 Chrono records used to come straight back — S-4), so incremental LAN only moves real changes.

## Known limits
- Pushes are one row per round trip (core `sync_once`): a first upload of a year of Chrono took 61 s locally and will take minutes over a mobile link. Batch push is the next core improvement.
- The agent's LAN auto-sync runs at most every 30 s even with `SELFKIT_LAN_SECS` lower (the service loop only checks the interval on beacon ticks/events).
- Photos don't travel over LAN (core LAN protocol has no blobs yet); they wait for S3.
- Discovery on Android needs the MulticastLock (plugin); some routers block broadcast → remembered peers / `lan_peers` still work.

## Code map
- `src/lib.rs`: `AppSync`, config, pass, transport choice, lock, status headline, test key.
- `src/model.rs`: `SyncSettings`, `SyncOwner`, `TransportMode`, `SyncStatus`, `SyncOutcome`, state files.
- `src/lan.rs`: one-shot LAN pass (beacon, listen, connect).
- `src/server.rs`: Postgres pass with schema/key checks, photo sync, server test.
- `src/conflicts.rs`: `ConflictView`, `Choice`, resolve.
- `src/secrets.rs`: `SecretStore`, `FileSecrets`, `MemorySecrets`, `KeychainSecrets`.
- `src/agent.rs`: desktop agent IPC client. `src/explain.rs`: plain errors.
- `src/android.rs` (feature `android`): JNI `SelfkitSyncNative.call` / `setWrapKey`, `android::secrets()` (lazy wrap key), mirror methods.
- `src/tauri_cmds.rs` (feature `tauri`): shared app commands, `default_handle`, `spawn_on_open`.
- `src/lock_cmds.rs` (feature `tauri`): app lock commands + `LockHandle`; feature `lock-test` (test builds: lock screen opens by itself, off when `SELFKIT_RELEASE=1`). See [app-lock.md](app-lock.md).
- `ui/src/sync/`: shared Svelte Sync page + conflict screen.
- `src/bin/sksync.rs`: headless test device for scripts.
