# selfkit-core

Shared Rust library used by every app and the agent. Status: **usable**. 21 unit tests + 2 integration tests (Postgres, S3), all passing.

Features: `remote` (Postgres + S3, tokio), `keychain` (OS keychain), `lan` (LAN sync). All on by default; Android builds will turn off `keychain`.

## Public API at a glance
- `Store::open(path, app, device_id, app_migrations)`: `put / get / delete / restore / list / list_range / search / count / kv_*`; sync helpers `dirty / mark_synced / apply_remote / apply_peer / changes_since / conflicts / resolve_conflict / import_record / reset_sync_state`.
- `keys::KeyFile::create / unlock_passphrase / unlock_recovery / change_passphrase / rotate_recovery / load / save`.
- `crypto::seal / open` (records), `StreamWriter / StreamReader / encrypt_stream / decrypt_stream` (files).
- `sync::sync_once(&mut store, &remote, &key)`, `sync::pg::PgRemote::connect(url)` (+ `server_schema_version()`, `known_schema_version()`), `sync::s3::BlobStore::new(cfg)`.
- `bundle::export / import / read_manifest`.
- `lan::serve(cfg, store_factory, state)`. One-shot passes for apps live in [selfkit-appsync](selfkit-appsync.md).
- `settings::Settings::load / save / effective_theme / set_shortcut`.
- `conflict::field_diffs / merge_fields / line_diff / apply_line_picks`.
- `paths::data_dir / app_db / key_file / device_id`.

## Code map
- `src/lib.rs`: module list, re-exports.
- `src/error.rs`: `Error` / `Result`.
- `src/time.rs`: UTC ms + local offset.
- `src/paths.rs`: shared data folder, per-app DB paths, device id file.
- `src/crypto.rs`: XChaCha20-Poly1305 seal/open; chunked STREAM writer/reader (64 KiB).
- `src/keys.rs`: data key wrapped by Argon2id passphrase + HKDF recovery key; `keys.json`.
- `src/store.rs`: SQLite schema + migrations (`core/001_init`, `core/002_versions`), CRUD, FTS5, tombstones, dirty tracking, server/peer merge rules, conflicts.
- `src/conflict.rs`: field diff/merge, line diff for long text.
- `src/settings.rs`: shared settings record (day start, currency, theme, shortcuts).
- `src/bundle.rs`: `.skb` export/import (tar inside STREAM).
- `src/keychain.rs`: OS keychain get/set/delete.
- `src/sync/mod.rs`: `Remote` trait, record encryption, `sync_once`.
- `src/sync/memory.rs`: in-memory remote for tests.
- `src/sync/pg.rs`: Postgres remote + server migrations, meta table, DB size.
- `src/sync/s3.rs`: encrypted blobs and bundle upload/download on S3.
- `src/lan/*`: LAN sync, see [../lan-sync.md](../lan-sync.md).
