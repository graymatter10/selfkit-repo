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
Ciphertext = XChaCha20-Poly1305 of `{data, tz_offset, deleted, hist}` with AAD `id|collection`. Server migrations: `sync::pg::MIGRATIONS`.

## Sync protocol
1. **Push:** for each dirty row: `UPDATE … WHERE id=$1 AND rev=$base_rev` (or `INSERT … ON CONFLICT DO NOTHING` when `base_rev=0`). Success → `base_rev = new rev`, `dirty = 0`. Zero rows → conflict.
2. **Pull:** `SELECT … WHERE app=$app AND rev > $cursor ORDER BY rev LIMIT 500`. For each row: clean local → overwrite; dirty local with the same content, or whose history already contains the remote version → adopt the rev; remote history contains our version → fast-forward; otherwise → `conflicts`.
   - Known limitation: sequence values commit out of order under concurrent writers, so a pull can in rare cases skip a row. Mitigation planned: an occasional full re-pull (cheap; apply is idempotent).
3. Runs every few minutes (agent / Android job) and on app open. Works with no server configured.

## Conflicts
- Two new items on different devices: different UUIDs, both kept.
- Same item edited on two devices: `conflicts` row; UI offers **Keep A**, **Keep B**, **Merge field by field** (`conflict::merge_fields`), long text gets a line diff (`conflict::line_diff`).

## LAN sync
Server-less device-to-device sync over the local network, fully encrypted. See [lan-sync.md](lan-sync.md).

## Blobs (photos, attachments)
Encrypted whole-file with XChaCha20-Poly1305 STREAM, stored at `blobs/<uuid>` in S3. Phone keeps last 90 days locally. Document-quality WebP (~2400 px) is produced by the app before handing bytes to core.

## Bundles (`.skb`)
`SKB1` magic + STREAM-encrypted **tar** (streams both ways with flat RAM) containing `manifest.json` (app, range, schema version, device), `records.jsonl`, `files/<id>`. Used for monthly backups, manual export/import, server migration.
