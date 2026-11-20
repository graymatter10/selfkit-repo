# Security and encryption

Audit 2026-11-15 (security agent). What is protected, how, which tests prove it, and what is still open.

## Threat model
One user, several own devices. Attackers we defend against:

| Attacker | Can | Must not be able to |
| --- | --- | --- |
| Same Wi-Fi (café, hotel, flatmate) | sniff, inject, replay UDP/TCP, spoof beacons, MITM DNS/TCP | read or change any record, join LAN sync, steal server credentials |
| Server operator / leaked DB or bucket (Postgres, S3) | read and rewrite every row and object | read records, photos, bundles, key file contents; forge records |
| Internet MITM on the way to the server | strip/downgrade TLS | see credentials or traffic |
| Another app on the phone | send intents, query providers, read shared storage | read Selfkit data or drive sync |
| Another local user/process on the PC (other uid) | connect to sockets/pipes in shared dirs | talk to the agent, read data |
| Malicious web page / extension | load pages in a browser, message extensions | reach the native host or app bridges |

Privacy defaults: Chrono stores app names and domains only (never titles or URLs); private windows are never tracked; exclude lists + pause. The Android signing key is backed up encrypted with the recovery key.

Out of scope: malware running as the same user / root on a device (it can read the plaintext SQLite and the keychain), a stolen unlocked device, physical attacks.

## Crypto and keys
- **Cipher:** XChaCha20-Poly1305 (24-byte random nonce from the OS CSPRNG) for records; STREAM (BE32, 64 KiB chunks, truncation-detecting) for files and bundles.
- **Data key:** 32 random bytes (OsRng), encrypts everything.
- **Passphrase envelope:** Argon2id (m=64 MiB, t=3, p=1, 16-byte salt) → KEK → wraps the data key (AEAD). Change passphrase = rewrap only. Parameters read from a key file are bounded (m ≤ 256 MiB, t ≤ 16, p ≤ 8) because `keys.json` can come from S3/the server (`keys::tests::tampered_kdf_params_are_refused`). Minimum passphrase: 12 characters (apps and `selfkit-agent init` — C14).
- **Recovery key:** 32 random bytes shown once as 13 groups of base32; HKDF-SHA256 → KEK → wraps the same data key. 256-bit, so no brute force.
- **Key file:** `keys.json` holds only the two envelopes; uploaded to S3 (`bundles/_keys/keys.json`) so a new device can unlock. Anyone holding it can try passphrases offline (Argon2id slows that down): use a long passphrase.
- **Secrets storage:** data key, Postgres URL, S3 keys. Desktop: OS keychain (`keyring`, service `com.selfkit`); only when no keychain service exists at all (no Secret Service on D-Bus, e.g. bare i3 — `keychain_absent`; a locked keyring or a dismissed prompt is an error, never a reason to use the file, C86) the apps keep them in `<data_dir>/sync/secrets.json`, mode 0600, not wrapped (`KeychainOrFile`, C80/S-5). That file is as safe as the plaintext app databases next to it: same-user malware is out of scope (above), anyone else needs the user's account. A working keychain always wins and removes the file copy. The agent still uses only the keychain. Android: `<data_dir>/sync/secrets.json` sealed with a wrap key that the Android Keystore protects (`SelfkitSecrets`); file mode 0600 inside the app sandbox.
- AEAD verification is constant-time (RustCrypto); there are no hand-written MAC/secret comparisons.

## Server sync (Postgres + S3)
- Records are encrypted on the device before upload. Ciphertext = AEAD of `{data, tz_offset, deleted, hist, updated_at, device_id}` with AAD `app|id|collection`, so the server can't read content or move a ciphertext to another record/collection/app. `updated_at` and `device_id` are also stored in clear for routing; since 2026-11-15 the authenticated copies inside the ciphertext win, so a server that rewrites them changes nothing (`sync::tests::server_cannot_rewrite_metadata_or_move_ciphertext`). Rows written before that still decrypt (they fall back to the clear columns).
- **Key check:** `sk_meta.keycheck` (sealed with the data key) stops a device with a wrong key before it pushes (apps; the agent does not run it yet, see limitations).
- **TLS is enforced:** Postgres URLs for any non-local host always use TLS with certificate + hostname checks (rustls, webpki roots): missing `sslmode` or `sslmode=prefer` becomes `require`, `sslmode=disable` is refused. Only loopback/Unix-socket servers (the local Docker fakes) may run without TLS. Before this, the default `prefer` let a MITM answer "no TLS" and request the password in clear (`sync::pg::tests::tls_policy`, `tls_strip_mitm_gets_no_password`: a fake server that strips TLS receives only the 8-byte SSLRequest).
- **S3:** `http://` endpoints are refused unless the host is loopback; use `https://` (`sync::s3::tests::plain_http_only_for_local_endpoints`). Blob keys are `blobs/<id>` with ids checked for `/`, `\`, `..`; bundle deletion only accepts `bundles/` keys.
- **SQL:** every query uses bind parameters; the only dynamic SQL is fixed column lists. No injection path.

## LAN sync
See [lan-sync.md](lan-sync.md). Summary:
- No pairing: possession of the data key is the authentication. `LanKeys` = HKDF(data key, "selfkit-lan-v1").
- **Beacons** (UDP 47616): `SKD1 || AEAD(beacon key, {device_id, name, port, ts})`. Strangers can't read or forge them. Older than ±10 min → ignored; the service also ignores a beacon that is not newer than the last one from the same device, so a captured beacon replayed from another address can't redirect sessions (`lan::tests::replayed_beacons_are_ignored`).
- **Sessions** (TCP 47617): both sides send 32 random nonces; per-direction keys = HKDF(session root, cn‖sn). Every frame is AEAD with the frame counter as AAD → replay, reorder, drop and reflection fail. A captured session replayed later fails at the first frame (`channel::tests::captured_session_cannot_be_replayed`). A rogue device without the key gets nothing and changes nothing (`lan::tests::rogue_peer_with_wrong_key_gets_nothing_and_changes_nothing`). Protocol version is checked inside the encrypted hello.
- **DoS bounds:** the first (unauthenticated) frame may be at most 64 KiB, later frames 16 MiB; buffers grow only with bytes that actually arrive; 30 s timeout per handshake/frame; at most 8 inbound sessions at once, extra connections are closed immediately (`stranger_cannot_make_us_buffer_big_frames`, `truncated_frame_is_an_error`, `connection_flood_is_capped`).

## Test key (test builds only)
`selfkit-appsync` feature `test-key` (apps: `sync-test-key`, currently in every app's `default` features) makes a build use SHA-256 of a **public** constant as the data key when no real key is stored. Anything synced with it (LAN and server) is readable and writable by anyone with a test build or the source. Guards:
1. **Release builds never use it.** `.github/workflows/release.yml` passes `release: true` to `android.yml`, `linux.yml`, `windows.yml`, which export `SELFKIT_RELEASE=1` at compile time; `selfkit_appsync::release_build()` then disables the test key and the test server whatever the features say.
2. **It never replaces a real key.** Once `keys.json` exists on the device (encryption set up), an empty or unreadable secret store means "Locked", never the test key (`tests::test_key_never_replaces_a_real_key`). Before, a keychain read error silently put a real-data device on the public key.
3. The Sync page shows a red "Not private: test key in use" banner while it is active.
APKs built locally or by `dev/ci-apk.sh` are NOT release builds: they still carry the test key. Phones with real data on such builds must set up encryption (Sync → Set up encryption), which replaces the test key.

## Android
- Providers `content://com.selfkit.<app>.provider`: exported, guarded by `com.selfkit.permission.SHARE` (`protectionLevel="signature"`) in the manifest and again in code (`checkCallingOrSelfPermission`); no `grantUriPermissions`; update/delete do nothing. Probed on the vivo 2026-11-15: `adb shell content query/call` → `SecurityException: Permission Denial … requires com.selfkit.permission.SHARE`.
- FileProvider (`${applicationId}.fileprovider`, Tauri template) is not exported; URIs are only granted to the camera app for captures.
- Exported components: launcher activity (ignores intent data), Hisab widget receiver (`APPWIDGET_UPDATE` is a protected broadcast: shell broadcast refused), Chrono notification listener (needs `BIND_NOTIFICATION_LISTENER_SERVICE`).
- `usesCleartextTraffic=false` in release builds.
- **Backups off:** `android:allowBackup="false"` + `dataExtractionRules` excluding everything (cloud backup and device transfer), because the local DBs are plaintext SQLite. Data moves through encrypted sync and `.skb` bundles. (Before: `ALLOW_BACKUP` flag set on every app.)
- **WebView:** CSP `default-src 'self'` in every app; Kotlin JS bridges (`SelfkitSync`, `SelfkitPhotos`, `AgendaFiles`, …) are exposed to whatever page the WebView shows, so every app registers `tauri_cmds::nav_guard()`, which blocks navigation to anything but the app's own origins (`nav_tests::only_app_pages_are_allowed`). Outside links go to the system browser (opener). File names coming from the page are reduced to a bare name before writing to Downloads/Pictures on Android ≤ 9.

## Desktop
- Agent IPC: Unix socket `$XDG_RUNTIME_DIR/selfkit-agent.sock` (dir 0700, socket 0600); Windows named pipe with `reject_remote_clients` and `first_pipe_instance`. Commands are a fixed enum (no arguments that reach a shell or path).
- Native-messaging host: only `chrome-extension://ijiimbgfojojhjlchgbkkhhodifbnami/` and Firefox `chrono@selfkit` in the manifests; messages ≤ 1 MiB; every session validated (id charset/length, hostname, time bounds, excludes, never overwrites another collection).
- Extension: permissions `tabs idle alarms storage nativeMessaging`, no host permissions, no content scripts, no remote code, no `externally_connectable`; only talks to the native host.
- Tauri: CSP without remote origins (Hisab also allows `https://open.er-api.com` for rates); capabilities `core:default`, `notification:default`, `opener:default` (Agenda), `core:app:allow-exit` (Dusk, Reflect); no fs/shell/http plugins; single-instance args only match fixed flags (`--capture`, `--rate`); no deep links.

## Backups and exports
- `.skb` bundles: `SKB1` + STREAM (authenticated, truncation detected) tar; tar entry names checked (`files/<id>`, no `/`, `\`, `..`); the manifest's app must match.
- Hisab CSV export: text cells starting with `= + - @ TAB CR` get a leading `'` (CSV/formula injection), numbers are untouched; import strips it again and refuses files over 16 MB / 100 000 rows (`export::tests::csv_cells_cannot_be_formulas`, `csvimport::tests::formula_guard_roundtrip_and_size_cap`). CSV/PDF exports are plaintext by design (the user asked for them).

## Known limitations (with proposed fixes)
- **Metadata in clear on the server:** app, collection name, record id (UUIDv7 = creation time), `updated_at`, device id, row sizes and counts. Fix: replace `collection` by HMAC(data key, collection) in a new server schema version, drop `device_id`, round `updated_at`.
- **Server rollback/withholding:** a malicious server can serve an older ciphertext of a record (clean local copies accept it) or hide rows. Fix: signed per-record version counters or a client-side high-water mark per id (reject `updated_at` older than the one already applied).
- **Blobs are not bound to their id:** STREAM blobs have no AAD, so the server can swap two photos. Fix: STREAM AAD = `blob|<id>` with a new magic (`SKS2`), old blobs readable as before.
- `BlobStore::get_raw` reads a whole object into memory (malicious server → large allocation). Fix: stream to a temp file with a size cap.
- The agent's server sync loop does not run the key check (`server::preflight`); a misconfigured agent could push rows under another key (they'd show up as undecryptable). Fix: call the same preflight in `sync_loop`.
- Beacons are recognisable (`SKD1` magic, cadence, length ≈ name length): an observer learns that a Selfkit device is present, not who. Fix: drop the magic, pad to a fixed size.
- Plaintext local SQLite (see DECISIONS.md); app lock is UI-only (Argon2id-hashed PIN in a local file, backoff, no wipe; other commands are not blocked while locked; test builds auto-open the lock screen unless `SELFKIT_RELEASE=1`). See [components/app-lock.md](components/app-lock.md).
- Unix socket falls back to the shared temp dir when `XDG_RUNTIME_DIR` is unset (squatting by another local user). Fix: `/tmp/selfkit-<uid>/` created 0700 and owner-checked, same path in `selfkit-appsync/src/agent.rs`.
- Chrome's extension id comes from the public `key` in `manifest.json`; an unpacked extension with the same key gets the same id and could talk to the native host (fake `web_sessions`, read pause state). Needs the user to load it in developer mode. Fix: publish the extension (store-signed) and drop the key.
- FileProvider paths are the Tauri template's (`external-path .`, `cache-path .`). Fix: narrow to the capture folder.
- 8-character minimum passphrase is short for an offline attack on `keys.json`. Proposal: 12+ characters or a strength meter.
- `cargo audit` was not run (needs a network install); key crates (rustls 0.23.46, ring 0.17.14, tokio 1.54, tar 0.4.46, tokio-postgres 0.7.18, crossbeam-channel 0.5.17, tracing-subscriber 0.3.23) are past their known advisories.

## Tests (all in `cargo test`, run on CI)
`selfkit-core`: `lan::channel::tests::{roundtrip_and_wrong_key, captured_session_cannot_be_replayed, stranger_cannot_make_us_buffer_big_frames, truncated_frame_is_an_error}`, `lan::tests::{two_way_lan_sync_is_encrypted_and_incremental (wire sniffer), rogue_peer_with_wrong_key_gets_nothing_and_changes_nothing, connection_flood_is_capped, replayed_beacons_are_ignored}`, `lan::discovery::tests::beacons_are_private`, `sync::tests::server_cannot_rewrite_metadata_or_move_ciphertext`, `sync::pg::tests::{tls_policy, tls_strip_mitm_gets_no_password}`, `sync::s3::tests::plain_http_only_for_local_endpoints`, `keys::tests::tampered_kdf_params_are_refused`. `selfkit-appsync`: `tests::test_key_never_replaces_a_real_key`, `tauri_cmds::nav_tests::only_app_pages_are_allowed`, `secrets::tests::file_secrets_sealed_roundtrip`. Hisab: `export::tests::csv_cells_cannot_be_formulas`, `csvimport::tests::formula_guard_roundtrip_and_size_cap`. Agent: `native_host` validation tests. Manual: provider/broadcast/activity probes with adb (above).
