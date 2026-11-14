# Decisions (newest first)

## 2026-11-15 — Hisab 0.1
- Opens to the split-layout add screen; on phones the other sections are top-bar pages instead of a tab bar, leaving room for the number pad.
- Totals convert at current rates (open.er-api.com, cached offline, hand edits win); no per-transaction rate is stored. Amounts are 2-decimal floats.
- Delete is a tombstone with undo; deleting a category or account archives it so history stays correct.
- Loan reminders are off by default and switch on with the first loan that has one. SMS import is opt-in and only queues messages for review, never saves automatically.
- Photos are WebP from the webview canvas (no Rust image library), stored as files; the half-typed entry is kept in SQLite kv so Back/kill never lose it.
- Exports go to Downloads.

## 2026-11-15 — Agenda 0.4
- CalDAV dropped for now (user request); `.ics` export/import kept, subscribable feed skipped.
- Photos are stored as files (`<data_dir>/agenda-photos/`), not in SQLite, so the DB stays small; photo files don't sync until the S3 blob store is wired into the apps.
- The running focus (Pomodoro) state lives in local kv so two devices never fight over a timer.
- Android Back uses Tauri's native back-button event instead of history entries (the WebView skipped re-added entries).
- The Board uses pointer events, not HTML5 drag and drop, so it behaves the same on WebKitGTK.
- On `.ics` import the newer version wins, so an old export can't undo later edits.
- The user kept the as-built screens (no redesign of the new views).

## 2026-11-15 — Browser tracking and agent backups
- The extension sends to the native host once a minute (the browser starts the host per message) instead of keeping a port open: nothing resident, and it works even when the agent isn't running because the host writes SQLite directly.
- The extension's own pause is local to that browser; Chrono's `paused_until` and the agent's pause apply on top. Only domains are stored; incognito is never tracked.
- Monthly backup bundles use UTC month boundaries with the device id in the name (deterministic, two PCs never overwrite each other). Monthly kept forever; weekly full exports keep 4 on S3 and 2 locally. `keys.json` (wrapped key file) is uploaded with the weekly export so a fresh device restores with only the recovery key.
- Agent reminders stay silent while the Agenda app runs, to avoid double notifications.
- The Chrome extension id is fixed by a public `key` in the manifest; no private key is kept.

## 2026-11-15 — Parallel build agents share the machine through locks
Several agents build apps at once. `dev/heavy.sh` serializes heavy jobs (Gradle, bundles, Docker) behind a RAM/disk gate, `dev/emu-lease.sh` gives one agent the emulator at a time, and `dev/notify-gate.sh` lets the user be warned 90 s before heavy steps (cooling). Cargo shares one target dir, which serializes compiles by itself.

## 2026-11-15 — Own clean minimal design, inspired by Apple apps but not copying them
User accepted Agenda v0.3 and wants Agenda to keep that style. For the other apps: clean, minimal, no bloat, easy to use, few colours, with Apple apps as inspiration only. The user doesn't use iOS or macOS, so a close copy of their look feels odd. Each app gets its own original design.

## 2026-11-15 — Agenda v0.3: clean iOS-style, minimal colour
User: colour distracts; make it like premium iOS apps, distraction-free; phone and desktop may differ. Neutral greys/white (black in dark), one blue accent, red only for overdue and destructive actions; severity shown as "!!"/"!!!" text, not colours. Phone: large title, segmented filter, inset grouped list, bottom "New Task". Desktop: sidebar + list + inspector. Form-based adding stays (user prefers it to plain-language parsing, which was removed). Unsaved input is never discarded silently: Cancel/Escape/Back ask first.

## 2026-11-15 — No predefined design system; each app is designed for its own job
User decision after testing Agenda on a real phone ("hard to add"). The split layout and themes A/B/D are no longer rules. Every app gets its own design (any style, must not be distracting), mocked up and approved before coding. Agenda is being redesigned from the user's reference: stat cards, filter chips, severity 1–5 task cards, a full "New task" form with start/stop and "remind before start". `ui/` stays as plumbing (IPC, icons, toast), not as a visual rulebook.

## 2026-11-15 — Frontend stack details
- Vite 8 (rolldown/oxc) has no esbuild: no `vitePreprocess`, no `minify: "esbuild"`. Svelte 5 compiles TypeScript itself.
- Svelte 5 runes, Tailwind 4 with theme tokens in `@selfkit/ui`; no component library dependency (bits-ui not needed yet: the few controls are native elements styled with tokens). Revisit if menus/date pickers get complex.
- Native `<input type=date/time>` pickers (via `showPicker`) instead of custom pickers: small, accessible, work on Android and WebKitGTK.
- Env-only test hooks (`AGENDA_DEMO`, `AGENDA_WINDOW_SIZE`) let the real app be driven into states for screenshots under Xvfb; inert unless set.
- Phone layout height follows `visualViewport` (keyboard) plus `adjustResize`.

## 2026-11-14 — The tasks app is named Agenda (was Tickd)
User choice. Identifier `com.selfkit.agenda`, data file `agenda.db`, docs in `docs/apps/agenda/`. PLANNING.md keeps the original name as history.

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
