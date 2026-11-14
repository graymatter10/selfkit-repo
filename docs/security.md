# Security and encryption

- **Cipher:** XChaCha20-Poly1305 (24-byte random nonce) for records; STREAM (BE32, 64 KiB chunks) for files and bundles.
- **Data key:** 32 random bytes, encrypts everything.
- **Passphrase envelope:** Argon2id (m=64 MiB, t=3, p=1, 16-byte salt) → KEK → wraps data key. Change passphrase = rewrap only.
- **Recovery key:** 32 random bytes shown once as 13 groups of base32 (`XXXX-XXXX-…`); HKDF-SHA256 → KEK → wraps the same data key.
- **Key file:** `keys.json` in the data folder holds only the two envelopes (safe to sync/print-adjacent). Also synced to server as record `sk_keys` so a new device can unlock with the passphrase.
- **Secrets storage:** unlocked data key, DB URL, S3 keys in the OS keychain (`keyring` crate: Secret Service / Credential Manager; Android Keystore via plugin).
- **App lock:** PIN / fingerprint gate in each app (UI-level).
- **Privacy defaults:** Chrono stores app names only, never titles; incognito never tracked; exclude list + pause.
- **LAN sync:** beacons and sessions encrypted with keys derived from the data key, fresh per-session keys, counter-bound frames. See [lan-sync.md](lan-sync.md).
- **Local DB:** plaintext SQLite for now — see DECISIONS.md (open question).
- **Android signing key:** back up encrypted with the recovery key.
