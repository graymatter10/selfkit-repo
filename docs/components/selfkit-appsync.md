# selfkit-appsync

App-side sync, shared by every app (and by a future companion app). Status: **built and tested** (no app uses it yet; wave 2 integrates it). Android glue: [android-plugins/selfkit-sync](../../android-plugins/selfkit-sync/README.md). UI: Sync settings section + conflict screen, mockups in the wave-2 design sheets.

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
sync.status().await?            // SyncStatus { headline, owner, unlocked, test_key, last_transport, last_peers, last_error, pending, conflicts, peers, next_auto_at, ... }
sync.sync_now(Trigger::Manual | Open | Background).await   // SyncOutcome { ok, ran, transport, peers, report, error, note, ... }
sync.settings() / sync.set_settings(&SyncSettings)          // owner, transport, auto_sync, background_minutes, lan_enabled, lan_peers, device_name
sync.server_config() / set_server(Some(&ServerConfig)) / test_server(&cfg).await   // Err(plain sentence)
sync.setup_encryption(pass) -> recovery key · unlock(pass) · unlock_recovery(rk) · change_passphrase · lock
sync.conflicts()? -> Vec<ConflictView>   // A = this device, B = the other; fields that differ; line diff for long text; delete_vs_edit
sync.resolve(store, id, &Choice::KeepA | KeepB | Merge { fields: {k: "a"|"b"}, lines: {k: {idx..}} })?
sync.due(trigger) / sync.schedule()       // scheduler hook: Open at most once a minute, Background every ~15 min; schedule().enabled=false → cancel the job
```
Headlines: "Synced locally with Linux PC · 2 min ago", "Synced via server · 5 min ago", "Waiting to sync · 12 changes saved here", "Not synced · <plain error>", "Locked · unlock to sync", "Synced by the Selfkit companion", "Syncing…".

## Sync owner
`SyncOwner::App` (Android default) runs passes itself. `Agent` (desktop default): `sync_now` asks the running selfkit-agent over its IPC socket and `status` merges the agent's state; with no agent running, the app syncs by itself (with a note). `Companion`: the app steps back (`sync_now` does nothing, `schedule().enabled = false`); a future companion syncs every app through the ContentProviders.

## Where things are stored (one place per device)
- Secrets (data key `data-key`, `postgres-url`, `s3-config`): `SecretStore`. Desktop: OS keychain, service `com.selfkit` (the same entries the agent uses, so one setup serves all apps). Android: `FileSecrets` at `<data_dir>/sync/secrets.json`, sealed with a 32-byte wrapping key kept encrypted by the Android Keystore (`SelfkitSecrets.wrapKey`). Tests: `MemorySecrets`.
- `keys.json` (wrapped data key) in the data folder, as before; `unlock` downloads it from S3 (`bundles/_keys/keys.json`, uploaded by the agent) when missing.
- Settings (non-secret, per device): `<data_dir>/sync/settings.json`. State per app: `<data_dir>/sync/state-<app>.json`.
- Photos: records of collection `photos` with a `file` field; bytes at `<data_dir>/<app>-photos/<file>` (Agenda, Hisab, Chrono already do this). S3 key `blobs/<record id>`, encrypted (STREAM). Local kv `appsync.blob.up.<id>` marks uploaded/downloaded files.

## Test key
Feature `test-key` (or `AppSyncConfig.test_key = true`): when no real key is stored, the data key is SHA-256 of a public constant. All test builds share it, so they sync with each other without prompts. It is **not private**: status shows `test_key: true`, and a real `setup_encryption` replaces it. Never ship a release build with it for real data. `test_server` can carry a dev server for the same builds.

## Plain-language errors
`explain(&Error)`: cannot reach server, timeout, refused password, missing database, storage refused key / missing bucket, certificate problem, wrong encryption key (server key check or LAN handshake), newer server schema, different Selfkit version on the LAN peer, locked, nothing configured, no PC found, busy. Technical detail goes to `SyncOutcome.detail` and the log.

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

Server table for all three: 16.3 MB (encrypted). The first push is slow because core pushes one row per round trip; incremental syncs are well under the 15-min period. Incremental LAN on a big DB is ~3 s because the peer's received records echo back once as no-ops (core LAN cursor design).

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
- `src/android.rs` (feature `android`): JNI `SelfkitSyncNative.call` / `setWrapKey`, `android::secrets()`.
- `src/bin/sksync.rs`: headless test device for scripts.
