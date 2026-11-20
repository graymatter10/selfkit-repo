# LAN sync (server-less, KDE Connect style)

Devices on the same network sync directly, with no Postgres needed. Works alongside server sync; either one alone is enough.

## Security (everything on the wire is encrypted)
- **No pairing:** all your devices hold the same data key. `LanKeys::derive` = HKDF(data key, "selfkit-lan-v1") → `session-root`, `beacon`.
- **Discovery beacons** (UDP 47616 broadcast, every 30 s + direct reply to new peers): `SKD1 || XChaCha20-Poly1305(beacon key, {device_id, name, port, ts})`. Strangers see random bytes and can't forge beacons. Beacons older than ±10 min are ignored.
- **Sessions** (TCP 47617, falls back to a free port that the beacon announces): handshake swaps 32-byte nonces, then `c2s/s2c = HKDF(session-root, salt = cn‖sn)`. Fresh keys every session; every frame is `len ‖ XChaCha20-Poly1305(dir key, json, aad = frame counter)`, so replay, reorder and drop are detected. A device without the key fails on its first frame. Max frame 16 MiB (first, unauthenticated frame 64 KiB; buffers grow only as bytes arrive), 30 s timeouts, at most 8 inbound sessions at once (more are closed at once). The service ignores a beacon that isn't newer than the last one from the same device (replayed beacons can't redirect sessions). Attack tests: [security.md](security.md#tests-all-in-cargo-test-run-on-ci).
- Test `lan::tests::two_way_lan_sync_is_encrypted_and_incremental` sniffs every byte and fails if record text, device names or collection names show up in plaintext.

## Sync model (no server revisions)
- Each record has `hist`: its last 16 versions (`"updated_at:device_id"`), kept by SQLite triggers (`core/002_versions`), and `lseq`, a local change sequence bumped on every content change from any source.
- A session: hello (device id, name, apps), then each side pulls the other's `changes_since(cursor)` per shared app in pages of 500. Cursor per peer: kv `lan.cursor.<device_id>`. The client pulls first, then says `YourTurn`; the server pulls, then `Bye`. Records whose current version the asking peer wrote itself are left out of its batches once that peer has synced with us before (`since > 0`; the cursor still moves past them), so nothing is echoed back (S-4); a peer pulling from 0 (fresh database, same device id) gets everything, its own records included (C85).
- `Store::apply_peer`: same/older version → ignore. Incoming history contains ours → fast-forward. Ours contains theirs → ignore. Same content under another version → merge histories. Otherwise → conflict (`conflicts.source = 'peer'`; the same Keep A / Keep B / Merge UI).
- LAN-received records stay `dirty`, so they still reach the server. `hist` also travels inside the encrypted server payload, so a server pull of an edit already received over LAN fast-forwards or is adopted instead of conflicting.
- Who connects: the lower device id starts the session when a peer appears and every `interval` (2 min). `sync_now` (agent `sync` command) triggers immediately. Static peers by address can be added for networks where broadcast doesn't reach (VPN, emulator).

## One-shot passes from apps (selfkit-appsync)
Apps (Android, or desktop without an agent) don't run the service. One pass: open a TCP listener on a free port, send a beacon every 700 ms while listening (S-6) (carrying that port) from an ephemeral UDP socket, listen `lan_discovery_ms` (2.5 s) for beacons (direct replies and the shared port 47616) and for inbound sessions (a peer with a lower device id connects to us), connect to a peer as soon as its beacon arrives (its listener is open then; two "Sync now" passes a second apart used to miss each other — S-6), then to every other discovered, remembered (14 days) or configured peer. Same keys, channel and session as the service. Android holds a `MulticastLock` meanwhile (plugin). Errors that mean "found a device but it refused us" (wrong key, other protocol version) are reported in plain words; unreachable remembered peers are silent.

## Agent settings (env for now, Settings UI later)
`SELFKIT_LAN=0` disables it · `SELFKIT_LAN_PEERS=ip:port,…` · `SELFKIT_LAN_PORT` · `SELFKIT_LAN_DISCOVERY_PORT` · `SELFKIT_LAN_SECS` · `SELFKIT_DEVICE_NAME`. Status: `selfkit-agent status` → `lan.peers`.

## Platform notes
- **Firewall:** allow UDP 47616 and TCP 47617 on the private network (Windows prompts on first run; ufw/firewalld on Linux).
- **Android:** needs a `WifiManager.MulticastLock` to receive broadcasts, and runs only while an app is open or inside a WorkManager job (no always-on service).
- Same-machine testing works: the UDP socket uses SO_REUSEADDR/SO_REUSEPORT.

## Not yet
- Photos/attachments over LAN (phase 2, after local blob storage exists).
- Settings UI toggle and a peer list screen (Reflect health screen).
- IPv6 / multicast discovery.

## Code map
- `selfkit-core/src/lan/mod.rs`: service (listen, announce, discover, schedule sessions), `LanConfig`, `LanState`, tests over real sockets.
- `selfkit-core/src/lan/channel.rs`: handshake, key derivation, encrypted framing.
- `selfkit-core/src/lan/discovery.rs`: encrypted beacons, UDP socket.
- `selfkit-core/src/lan/session.rs`: protocol messages, pull/serve phases.
- `selfkit-core/src/store.rs`: `apply_peer`, `changes_since`, `merge_hist`, `hist_push`, migration `core/002_versions`.
- `selfkit-agent/src/sync_loop.rs`: `run_lan` (config from env, store factory).
