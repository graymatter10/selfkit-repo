# Decisions (newest first)

## 2026-11-20 — Phone mirrors: Dusk nights for Reflect (B2), Agenda/Hisab for Dusk (B4)
- Reflect on Android gets Dusk's sleep through a read-only copy of Dusk's local `dusk_nights` table: Dusk's provider serves it whole (`/table?name=dusk_nights`, allow-list `SHARED_TABLES` in `selfkit-appsync/src/android.rs`) and `SelfkitMirror` replaces the copy in Reflect's `dusk.db` on each mirror pass. Not chosen: publishing nights as synced records — each device estimates its own nights from its own screen use (phone vs PC), so synced nights would conflict or duplicate across devices; too risky before release. Phone-local only, no sync changes.
- Dusk on Android mirrors Agenda and Hisab like Reflect (`com.selfkit.sync.MIRROR=agenda,hisab`), so Connections' next-day cards work on the phone. Reason: both apps are usually installed on the same phone; "On computer" read as broken.

## 2026-11-20 — Sync bug round (QA-U1..U4, R1), Hisab typing lag, laptop limits
- Numbers compare by value in sync (`conflict::json_eq`: `5000 == 5000.0`) for conflict diffs, `same_content`, field clocks and the field merge. Reason: Rust writes whole numbers as f64, JS as integers, so every such field looked changed (QA-U3).
- The conflict screen hides bookkeeping fields (`id`, `created_at`, `updated_at`, `tz_offset`) and shows lists/objects as readable items (one per line, title/name or values without ids/timestamps). Reason: users can't decide between raw JSON or epoch numbers (QA-U2/U3).
- "Selfkit companion" is only selectable when `com.selfkit.companion` is installed (bridge `companionInstalled`, manifest `<queries>`); a phone already set to it shows "Not syncing" with "Let this app sync". Reason: choosing it silently stopped sync (QA-U4).
- Hisab: the add-screen draft goes to localStorage on every key and to SQLite after 0.3 s idle (or when the app is hidden); both copies carry `_t` and boot takes the newer. The main data commands (`snapshot`, `transactions`, `trends`, `save`, `get`, `remove`, `restore`, `draft_set`, `prefs_set`) run as `#[tauri::command(async)]`, off the UI thread. Reason: 0.1.4 typing stalls — a SQLite write per key on the main thread waited for sync passes.
- Same field edited on two devices auto-merges (newer wins) by design (field clocks); QA, scenario and core tests that expected a conflict there now edit long multi-line text (still a conflict) with a short pause so both edits are newer than the shared version. Sync reports (pulled/pushed/received/sent) skip the device directory (`store::DEVICES`), like the pending count. CI runs `cargo test --no-fail-fast` so one run shows every failure.
- Laptop safety (user rule): CPU may not stay >= 70 °C longer than 40 s — `dev/thermal-guard.sh` stops build/test work at 40 s and saves a snapshot ref and powers off if still hot 40 s later; `dev/power-guard.sh` powers off on battery below 30 %. Both post to the notify queue so the lead reports events in the main chat.

## 2026-11-15 — Security hardening (security audit, docs/security.md)
- Release workflows compile with `SELFKIT_RELEASE=1`, which disables the public sync test key and test server. Reason: `sync-test-key` is in every app's default features, so release installers would have synced real data under a public key. Kept the feature default on for test builds so phone testing still needs no passphrase; flipping the default is the lead's call once the phones have real encryption set up.
- A device with `keys.json` never falls back to the test key (unreadable secret store → Locked). Reason: a keychain hiccup silently moved a real-data device onto the public key, including its LAN listener.
- TLS required for every non-local Postgres (missing/`prefer` → `require`, `disable` refused) and https required for non-local S3. Reason: `prefer` let a Wi-Fi MITM strip TLS and ask for the password in clear. Loopback stays plaintext-capable for the Docker fakes.
- `updated_at`/`device_id` copied into the encrypted record payload and preferred on decrypt (backward compatible). Reason: the clear columns were unauthenticated and feed the merge rules.
- LAN: 64 KiB cap on the unauthenticated first frame, incremental buffers, 8 inbound sessions max, monotonic beacon timestamps per device. Reason: pre-auth memory DoS and beacon replay redirect.
- Android: `allowBackup=false` + data extraction rules excluding everything. Reason: plaintext SQLite went to cloud backup / device transfer. Data moves via encrypted sync and bundles instead.
- Webview navigation guard plugin in every app. Reason: Android JS bridges are exposed to any page the WebView shows.
- Hisab CSV export neutralizes formula cells; import caps size. Argon2 parameters from a downloaded key file are bounded.

## 2026-11-15 — Sync in apps, Hisab CSV import, installers
- Sync is integrated into all five apps through `selfkit-appsync` (shared Tauri commands + shared `ui/src/sync` Sync page and conflict screen; Android plugin bridge). Test builds use the `sync-test-key` feature (on by default) so testing never asks for a passphrase.
- On Android, Reflect reads read-only mirrors of the other apps' data through their signature-protected ContentProviders (`SelfkitMirror`).
- Hisab accounts carry `explicit_currency`; new entries default to the main currency unless the account's currency was chosen by the user. Unused never-chosen accounts follow the main currency; used ones get a one-at-a-time "switch?" offer (banner + Settings).
- Hisab SMS import removed (user request); the `sms` collection name stays so old synced records remain valid.
- Hisab CSV import: own export format first-class, generic bank CSV via a mapping step, duplicate skip (date+amount+note), whole-import undo via `import_batch`; hand-written parser (no new dependency).
- Windows installers: per-user NSIS per app (WebView2 bootstrapper), agent installer with HKCU Run autostart (no admin), and a combined Selfkit Setup that runs the per-app installers and carries the offline WebView2 once. Uninstall keeps data unless "Also delete my Selfkit data" is ticked.
- Linux: AppImage outputs are copied per app right after each build (Tauri empties the bundle folder each time).

## 2026-11-15 — Main currency and shared settings writes
- Shared settings are written with `Settings::update(store, |s| …)`: reload, change only the fields this app owns, keep the rest. Fixes stale whole-object saves (e.g. Agenda writing an old USD over Hisab's BDT on desktop).
- Hisab never silently falls back to USD: the last known main currency is kept (Rust state + kv, UI localStorage).
- When the main currency changes, unused default accounts (no entries, zero opening balance) follow automatically; used accounts are only relabelled after the user says yes (numbers kept, not converted); accounts deliberately set to another currency are left alone. Category budgets are converted with saved rates.

## 2026-11-15 — Reflect 0.1
- Amber accent (from the icon); other apps' colours appear only as small source markers.
- Reflect computes its own numbers from the other apps' raw records (read-only), except Dusk's `dusk_nights` cache; screen-time aggregates are kept in memory and updated incrementally.
- Streak settings are a synced record (`reflect_settings`) shared by phone and PC; theme is per device.
- "Late-night phone use" = 00:00–05:00 local time.
- On phones Reflect shows "not shared with phones yet" until the Android data bridge (ContentProvider) lands.

## 2026-11-15 — QA round (Agenda 0.4.1, Hisab 0.1.1, Dusk 0.1.2)
- A damaged/unreadable database is moved aside as `<name>.damaged-<ms>` (never deleted) and the app starts fresh with a one-time notice (`recover.rs` in each app).
- Desktop apps are single-instance (second launch focuses the first; `--capture`/`--rate` forwarded); disabled when `SELFKIT_DATA_DIR` is set so tests can run several.
- Hisab draft date "" means "today" so an overnight draft doesn't save on yesterday.
- Dusk converts Hisab amounts with Hisab's own rates before correlating spending.
- Dusk time entry is a clock dial (user request) instead of a keypad; minutes snap to 5-min marks within ±1 min, exact minute elsewhere; 12h/24h follows the locale; typing remains on desktop only.

## 2026-11-15 — App-side sync (selfkit-appsync)
- One shared crate behind a small interface (status, sync_now, settings, encryption, conflicts/resolve); Android adds Kotlin only for what Rust can't do (WorkManager, Keystore, multicast lock). Apps stay free of sync code and a future companion app uses the same calls.
- Apps run a one-shot LAN pass (beacon + 2.5 s listen + connect to known peers), not the always-on LAN service; if no LAN peer answers, Postgres + S3. Same encrypted protocol as the agent, so it works against the existing agent unchanged.
- Owner switch: `app` | `agent` (desktop default — the agent already syncs every DB; two writers would race) | `companion` (app steps back).
- Server key check (`sk_meta.keycheck`) so a device with the wrong key can never push unreadable rows.
- Photos sync only through S3 for now (core LAN protocol has no blobs yet).
- Test builds use a public built-in test key (feature `test-key`), clearly not private, so testing never asks for a passphrase.
- Core fixes found by the e2e tests: resolved conflicts now clear on the other device; migrations run in BEGIN IMMEDIATE (two processes opening a fresh DB); line_diff trailing-newline; S3 bundle download fsync.

## 2026-11-15 — Chrono 0.1
- Android sessions go into `app_sessions` with platform `android`, so one engine and one timeline serve every device.
- Screen time is the union across devices (phone use while the PC is on counts once); browser app time is replaced by website time on the same device.
- Focus = (Work + Study) ÷ (Work + Study + Social + Entertainment).
- Per-day aggregates are cached locally (`chrono_daily`), invalidated by the store's change counter.
- The 15-min WorkManager job calls the same Rust importer through JNI (one importer).
- App install/uninstall/reinstall is found by diffing the launcher-visible app list on each import (`<queries>`, no QUERY_ALL_PACKAGES); Android no longer delivers package broadcasts. Usage from the last ~15 min before an uninstall can be lost (Android deletes it).
- Notification counts: opt-in listener plus usage events, taking the larger.
- Back/Escape in the note editor save instead of asking.

## 2026-11-15 — Dusk 0.1
- A night is keyed by its morning date with an 18:00–18:00 window, so windows tile the timeline and the morning date is the "next day" for connections.
- The phone stores per-night screen-on intervals (`device_use`), not finished estimates, so the desktop can merge them with computer use and recompute when the method improves.
- Opening the app counts as use; naps are manual only; "screen time after midnight" counts all devices.
- The wind-down reminder is 14 one-shot exact alarms, re-planned on every start/change.
- Reflect reads the local `dusk_nights` cache instead of re-implementing the estimate.
- Connection strength from correlation: ≥ 0.5 strong, ≥ 0.3 moderate, ≥ 0.15 weak ("may").
- Android Back uses Tauri's `onBackButtonPress` (browser history is unreliable in the WebView) — now the pattern for all apps.

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
