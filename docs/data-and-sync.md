# Data, storage and sync

## Local storage (`selfkit-core::store`)
One SQLite DB per app (WAL mode). Core table:

| Column | Type | Notes |
| --- | --- | --- |
| `id` | TEXT PK | UUIDv7 |
| `collection` | TEXT | e.g. `tasks`, `transactions`, `activity_events` |
| `data` | TEXT | JSON document |
| `updated_at` | INTEGER | UTC ms |
| `tz_offset` | INTEGER | minutes, local offset at write time |
| `device_id` | TEXT | device that made the last change |
| `deleted` | INTEGER | tombstone (0/1); undo just flips it back |
| `dirty` | INTEGER | 1 = not yet pushed |
| `base_rev` | INTEGER | server `rev` this local copy is based on (0 = never synced) |
| `hist` | TEXT | JSON array of the last 16 versions `updated_at:device_id` (triggers) |
| `lseq` | INTEGER | local change sequence (LAN sync cursor) |

Also: `records_fts` (FTS5 over `data`), `conflicts`, `kv` (sync cursor, device id), `blobs` (local photo cache index). Schema version in `PRAGMA user_version`; migrations in `store::migrations` (core) plus per-app migrations passed to `Store::open`.

## Server (any Postgres ≥ 12, standard SQL)
```sql
sk_records(id TEXT PK, collection TEXT, updated_at BIGINT, device_id TEXT,
           ciphertext BYTEA, rev BIGINT  -- from sequence sk_rev_seq)
sk_schema(version INT)
```
Ciphertext = XChaCha20-Poly1305 of `{data, tz_offset, deleted, hist, updated_at, device_id}` with AAD `app|id|collection`; the authenticated `updated_at`/`device_id` win over the clear columns (older rows without them fall back). TLS is required for non-local servers (see [security.md](security.md)). Server migrations: `sync::pg::MIGRATIONS`.

## Sync protocol
1. **Push:** for each dirty row: `UPDATE … WHERE id=$1 AND rev=$base_rev` (or `INSERT … ON CONFLICT DO NOTHING` when `base_rev=0`). Success → `base_rev = new rev`, `dirty = 0`. Zero rows → conflict.
2. **Pull:** `SELECT … WHERE app=$app AND rev > $cursor ORDER BY rev LIMIT 500`. For each row: clean local → overwrite; dirty local with the same content, or whose history already contains the remote version → adopt the rev; remote history contains our version → fast-forward; otherwise → `conflicts`.
   - Known limitation: sequence values commit out of order under concurrent writers, so a pull can in rare cases skip a row. Mitigation planned: an occasional full re-pull (cheap; apply is idempotent).
3. Runs every few minutes (agent), every ~15 min on Android (WorkManager), on app open and on "Sync now". Works with no server configured (LAN only).

## App-side sync interface (all apps, companion-ready)
Every app syncs through one shared module, [selfkit-appsync](components/selfkit-appsync.md) (Rust) + [android-plugins/selfkit-sync](../android-plugins/selfkit-sync/README.md) (Kotlin). The interface is small, so a future Selfkit companion app can take over without app rewrites:

| Call | What it does |
| --- | --- |
| `status()` | headline ("Synced locally with Linux PC · 2 min ago" / "Synced via server · …"), owner, lock state, last transport and peers, plain-language error, pending changes, conflicts, seen devices |
| `sync_now(trigger)` | one pass: LAN to a reachable peer if on the same network, otherwise Postgres + S3; `trigger` = manual / open / background |
| `settings()` / `set_settings()` | owner (`app` / `companion` / `agent`), transport (`auto` / `lan_only` / `server_only`), auto sync, period, LAN on/off and extra peers, device name |
| `conflicts()` / `resolve()` | Keep A (this device) / Keep B (other) / Merge field by field with line picks |
| `due()` / `schedule()` | scheduler hook: run on open (at most once a minute) and every ~15 min (WorkManager on Android) |

- **When:** manual "Sync now", on app open, and a ~15 min background job (Android WorkManager, network constraint). On desktop the agent syncs every app DB; the app's Sync now asks the agent (owner `agent`).
- **Transport per run:** same network + peer reachable → LAN; otherwise server. Shown in the status.
- **Sync owner:** a device-level switch. With `companion`, apps stop their own jobs and the companion syncs everything.
- **Shared place:** secrets in the OS keychain (`com.selfkit`) / Android Keystore-sealed file; non-secret settings in `<data_dir>/sync/settings.json`.
- **ContentProvider per app:** `content://com.selfkit.<app>.provider` (`/changes?since=`, insert `/records`, `call(status|sync_now|settings|set_settings|schedule|conflicts|resolve)`), protected by the signature permission `com.selfkit.permission.SHARE`, so only Selfkit apps signed with the same key can use it.
- **Guards:** server key check (`sk_meta.keycheck`) stops a device with the wrong key before it pushes; a server schema newer than the app knows is refused; a killed pass leaves a lock that the next pass takes over; pushes resume where they stopped (the server copy is adopted, no duplicates, no conflicts).

## Conflicts
- Two new items on different devices: different UUIDs, both kept.
- Same item edited on two devices: `conflicts` row; UI offers **Keep A**, **Keep B**, **Merge field by field** (`conflict::merge_fields`), long text gets a line diff (`conflict::line_diff`).
- **Automatic field merge (every collection):** each record carries per-field change clocks (`records.fclock`, `{field: UTC ms}`, kept by `Store::put`, synced in the server payload and over LAN). When two devices edited the same record, `Store::field_merge` compares each field with the last version both share (from `hist`): changed on one side → that side; changed on both → the newer change; filled on one side and empty on the other → the filled value; lists changed on both → combined by item `id` (newer copy of the same item; known limit: an item removed on one device while the other also changed that list comes back). The conflict screen remains for edit vs delete, long text changed on both (line merge), a version without clocks on either side (older build, or an edit made before the upgrade — C2), and exact ties. The first edit of a record that has no clocks yet stamps only the changed fields with now; unchanged fields get the record's old `updated_at` (C1). A bundle import stamps the fields that differ from this device's copy with the bundle's time, so restored values aren't treated as unchanged (C3). Settings writes drop the old in-data `_clock` object of the 151f5e9 builds, and the conflict screen hides it (C15). A record merged with the server's copy after a rejected push is pushed again in the same pass (C9).
- Edited on one device, deleted on the other: same screen (`delete_vs_edit`); Keep A over a delete brings the item back everywhere.
- A conflict that was settled on another device (the incoming version's history contains the other side) is cleared automatically (`Store::clear_superseded_conflict`).

## LAN sync
Server-less device-to-device sync over the local network, fully encrypted. See [lan-sync.md](lan-sync.md).

## Blobs (photos, attachments)
Encrypted whole-file with XChaCha20-Poly1305 STREAM, stored at `blobs/<uuid>` in S3. Phone keeps last 90 days locally. Document-quality WebP (~2400 px) is produced by the app before handing bytes to core.

## Bundles (`.skb`)
`SKB1` magic + STREAM-encrypted **tar** (streams both ways with flat RAM) containing `manifest.json` (app, range, schema version, device), `records.jsonl`, `files/<id>`. Used for monthly backups, manual export/import, server migration.
