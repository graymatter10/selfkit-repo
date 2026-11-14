# Decisions (newest first)

## 2026-11-14 — Identifiers are com.selfkit.{appname}; extra storage is /shared/selfkitextra
User choice. Applies to Tauri/Android/Windows ids, keychain service (`com.selfkit`), native-messaging host (`com.selfkit.agent`). Build output and dev-server data live in `/shared/selfkitextra/` (root disk is tight).

## 2026-11-14 — LAN sync: data key as the pairing secret; version histories instead of server revs
User asked for KDE Connect–style local sync, encrypted. All devices share the data key, so it authenticates peers (no pairing UI, no certificates). Per-session HKDF keys from exchanged nonces plus a frame counter in the AAD give fresh keys and replay protection. Without a central rev, conflicts are found with a bounded per-record version history (16 entries) kept by SQLite triggers. A false conflict is only possible after more than 16 unseen edits, and it just shows the conflict UI. Discovery is UDP broadcast (simple, works on Windows/Linux/Android with a MulticastLock); static peer addresses cover networks without broadcast.

## 2026-11-14 — S3 dev stand-in is SeaweedFS
MinIO no longer publishes public Docker images (Docker Hub and quay both refused). SeaweedFS's S3 gateway is compatible with what we use.

## 2026-11-14 — Shortcuts open a self-contained popup and are user-configurable
User request: a quick-capture shortcut shows a small popup that completes the task on its own, and every shortcut can be changed in Settings. Stored in the shared settings record (`shortcuts` map); defaults Super+Shift+E/T/S/P.

## 2026-11-14 — Local SQLite is plaintext for now; encryption applies to everything leaving the device
SQLCipher would add OpenSSL/C build pain to Android and Windows cross builds. Records, bundles and blobs are encrypted before upload/export; the local DB relies on OS disk encryption + app lock. **Open question for the user** (see STATUS.md). Revisit with `rusqlite` `bundled-sqlcipher-vendored-openssl` if wanted.

## 2026-11-14 — Server sync uses optimistic concurrency on a `rev` column
Each server row has a monotonically increasing `rev` (from a global sequence). A push carries the `rev` the client last saw; the server updates only if it matches, otherwise the push is reported as a conflict and the client stores both versions for Keep A / Keep B / Merge. Standard SQL, works on any Postgres ≥ 12.

## 2026-11-14 — Records are generic `(id, collection, json)` rows in one table per app DB
Lets one sync engine, one bundle format and one conflict UI serve all five apps. App-specific indexes/FTS are added by each app's migrations on top.

## 2026-11-14 — Bundles: tar inside chunked XChaCha20-Poly1305 STREAM
Tar streams in both directions (zip readers need seeking); standard inner layout (manifest.json, records.jsonl, files/); the STREAM construction (64 KiB chunks) keeps RAM flat on weak devices and authenticates truncation.

## 2026-11-14 — Recovery key wraps the data key via HKDF, passphrase via Argon2id
Recovery key is 256 random bits (no stretching needed); passphrase is low-entropy so it uses Argon2id (64 MiB, t=3). Both wrap the same random data key, so changing either is instant.

## 2026-11-14 — Repo is a Cargo workspace; IDs are UUIDv7
UUIDv7 is time-ordered (good SQLite/Postgres locality) and safe to create offline on any device.
