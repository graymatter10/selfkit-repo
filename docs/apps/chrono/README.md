# Chrono (com.selfkit.chrono)

Status: **v0.1, built and tested on Linux (Xvfb, seeded + live agent data) and on Android.** Activity dashboard over `chrono.db`: Today, Timeline, Trends, Weekly report, Settings, Devices and sources, daily notes with photos, Android usage tracking (Usage access + optional notification counts, 15-min background import), goals/limits with alerts. Desktop tracking runs in the agent (`app_sessions`, [../../components/selfkit-agent.md](../../components/selfkit-agent.md)); browser tracking in the extension + native host (`web_sessions`, [../../components/browser-extension.md](../../components/browser-extension.md)). Scope: [PLANNING.md](../../PLANNING.md) § Apps → Chrono.

Design: the reviewed design approved by the user on 2026-11-15 (scratchpad `review/chrono/chrono-reviewed.html`). Neutral greys, teal accent (#0f8a78 / dark #2bb39c) only for "on / you", categories have their own colours (Work blue, Study violet, Social pink, Entertainment amber, Other grey, always with a text label), red only when a limit is over. Plain sentences instead of bare numbers. Tap/click for details (no hover-only info). Chart text ≥ 12 px, targets ≥ 44 px. Light/dark follow the shared Selfkit theme (System/Light/Dark).

## Screens
- **Today** (opens here): screen time today (big number) + one plain sentence ("About a usual day for you — 8 min more than your daily average (6 h 04)", "So far today. Your daily average is …"), a 24-hour strip of when any screen was used, first use / last use / longest session. Then the daily note ("Add a note about today" or text + photos), Goals and limits ("26 min left", "4 min to go", "Done", red "12 min over"), Most used (bar in the category colour, device and category label beside it), Where the time went (stacked bar, %, time, "Focus 56% — of your time was Work or Study"), Phone ("Screen turned on N times", "Unlocked N times", notifications), This week so far vs the same days last week, Devices. ‹ › moves to past days (title = weekday; past goals read "Kept · 20 min under", "Met", "short"); desktop shows "Back to today".
- **Timeline**: one lane per device (phones first, each PC followed by "Browser on <PC>"), blocks coloured by category; tap/click a block → details row (name, category, device, time range, length). Sessions list newest first (virtualized), device filter, "N short checks under 1 minute · Show".
- **Trends**: Day / Week / Month / Year / All with ‹ ›. Leads with the daily average in words and the change vs the previous period, then bars per hour/day/month/year (tap a bar → value), dashed average line; Totals (Apps / Websites / Devices / Categories, "31% more than Oct" when ≥ 10%, per-day fair comparison, "Show all" → virtualized list); When you use screens (hour × weekday heatmap, sentence "Busiest on weekdays around 10–11, and on weekend evenings around 21–22", tap a cell); Daily habits (screen turned on, unlocked, notifications, Focus, longest session).
- **Weekly report** (Mon–Sun, defaults to last week, ‹ ›): average a day + change vs the week before, 7 bars with busiest/quietest labelled, goals as days kept (dots per day), where the time went + Focus vs the week before, most used with changes ("1 h 40 less"), Worth knowing (longest session, late nights after 23:00, phone pickups, notifications and the top app, busiest hour).
- **Settings**: Tracking (status, Pause for 15 min / 1 hour / Until tomorrow / Until I resume, Resume; privacy note), This phone (Android: Usage access, Notification counts, last import), Goals and limits (+ New: limit or goal for all screen time / a category / an app / a website, minutes presets; tap to edit/delete with undo; on/off switches), Alerts, Weekly report switch, Never track (apps and websites incl. subdomains, undo on remove), Categories (search, per-name category), Devices and sources, Appearance (shared).
- **Devices and sources**: one card per device seen in 30 days (rename by tapping the name; "This device"; Recording / latest data …): Active app (window manager / Windows), Time away threshold (`idle_minutes`), Firefox / Chrome (Connected or "How to add"), Usage access / Notification counts for phones; Add a device (how-to).
- **Android setup** (first launch, 2 steps): "Let Chrono see how you use this phone" (Usage access; what Chrono sees and never sees) → "Count notifications too?" (optional, off unless turned on). See [android.md](android.md).
- **Note editor**: full screen on phone, dialog on desktop; text + photos. See [notes.md](notes.md).
- **App changes**: from Trends (card with the last three changes → "See all"): installs, removals, reinstalls (updates on request), grouped by month, with icons and each app's all-time use; removed apps show an "Uninstalled" tag in totals, the timeline and reports, and Trends has "Include uninstalled apps". See [app-changes.md](app-changes.md).
- **Pause**: one control: the status line pill on phone Today, the Recording card in the desktop rail (menu: 15 min, 1 hour, until tomorrow, until I resume; Resume when paused). Writes `chrono_settings.paused_until` + `pause_log`; the agent, the native host and the Android importer honour it.

Keyboard (desktop): T Today · L Timeline · R Trends · W Weekly report · , Settings · ←/→ previous/next day or week · N note for the shown day · P pause/resume · Esc close. Android Back (Tauri `back-button` event since 0.1.1, like Agenda/Hisab; with nothing open `ChronoBars.toBackground()` leaves the app — the old history-entry approach left the app on the second Back after closing the photo viewer or photo sheet, because the WebView skips history entries re-added without a tap) closes the topmost layer ("Add photo" sheet, photo, note editor — saving changes —, pause menu, sources, settings, past day, other tab → Today).

## Data model (`chrono.db`, selfkit-core records)

All times are **UTC ms**; `tz_offset` is the local offset in **minutes east of UTC** at the time of the event (e.g. +330 for UTC+5:30). "Local day" of an event = `start + tz_offset*60000` floored to the day, shifted by the shared day-start setting. Idle time is never recorded (writers drop it). Every record also carries the store's `device_id` column (device that wrote it).

### `chrono_settings` — shared settings (single record, id `settings`)
Written by the Chrono app (desktop and Android); **read by the agent and the browser native host**. Missing record or missing field = default.

```json
{
  "paused_until": null,
  "exclude_apps": ["keepassxc", "com.example.bank"],
  "exclude_domains": ["mybank.com"],
  "categories": { "code": "work", "anki": "study", "youtube.com": "entertainment", "com.whatsapp": "social" },
  "goals": [
    { "id": "g1", "kind": "limit", "scope": "category", "target": "entertainment", "minutes": 60, "enabled": true },
    { "id": "g2", "kind": "goal",  "scope": "category", "target": "study",         "minutes": 90, "enabled": true }
  ],
  "alerts": true
}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `paused_until` | UTC ms or `null` | Tracking is paused while `now < paused_until`. `null` (or a past time) = recording. "Until I resume" = `4102444800000` (2100-01-01). Writers must not record anything while paused. |
| `exclude_apps` | string[] | App names exactly as recorded in `app_sessions.app` (desktop `WM_CLASS`/exe name, Android package name). Compare **case-insensitively**. Excluded apps are never recorded. |
| `exclude_domains` | string[] | Domains without scheme. `example.com` also excludes every subdomain (`*.example.com`). Compare case-insensitively, ignore a leading `www.`. |
| `categories` | object name → category | Key: app name, Android package, or domain. Value: `study`, `work`, `social`, `entertainment`, `other` (or any user-made lowercase name). Unmapped names fall back to Chrono's built-in defaults, then `other`. Only Chrono uses this. |
| `goals` | array | Daily goals/limits. `kind`: `limit` (stay under) or `goal` (reach at least). `scope`: `total` (all screen time, `target` ignored), `category`, `app` or `domain`. `minutes` per local day. `enabled` bool. Only Chrono uses this. |
| `alerts` | bool | Chrono shows notifications when a limit is reached or a goal is met (default `true`). |
| `device_names` | object device id → name | Chrono-only display names for devices. |
| `weekly_report` | bool | Chrono shows "Your weekly report is ready" on Monday mornings (default `true`). |
| `idle_minutes` | number or `null` | Minutes without keyboard/mouse after which time is not counted. **Writers (agent, extension/native host) should honour it**; `null` = their default (2 min). Set in Devices and sources. |
| `pause_log` | `[[from, to], …]` UTC ms | Chrono-only: recent pauses (newest last, max 50) so the Android importer can skip stretches it imports later. The current pause has `to = paused_until`. |

Excluding/pausing does not delete past data; the Chrono UI hides excluded names everywhere too.

### `app_sessions` — foreground app sessions (desktop agent, Android importer)
`{app, platform, start, end, tz_offset}` + optional `package`, `device`.
- Desktop (agent): `platform` = `linux` | `windows`, `app` = `WM_CLASS`/exe name. Device = record `device_id`.
- Android (this app's importer): `platform` = `android`, `app` = app label (e.g. "WhatsApp"), `package` = package name, `device` = device id. Record id is deterministic (`a-<device>-<start>-<hash(package)>`), so re-imports overwrite instead of duplicating.

### `web_sessions` — browser tab time (extension via the agent's native host)
`{domain, browser: "chrome"|"firefox", device, start, end, tz_offset}`. Active tab only, idle excluded, incognito never recorded. Written by the collectors' native host; Chrono only reads it.

### `android_events` — hourly phone counters (Android importer)
One record per device per local hour that had activity; id `e-<device>-<YYYYMMDDHH>` (hour in UTC), re-imports overwrite.
`{hour (UTC ms of the hour start), tz_offset, device, unlocks, pickups, notifications: {package: count}}`.
- `unlocks` = `KEYGUARD_HIDDEN` events; `pickups` = `SCREEN_INTERACTIVE` events (screen turned on).
- `notifications` = notifications posted per package, from the opt-in NotificationListenerService (and from `NOTIFICATION_INTERRUPTION` usage events if Android delivers them).

### `android_apps` — package → label (Android importer)
id `p-<package>`: `{package, label}`. Lets desktop screens show labels for packages (e.g. in notifications).

### `app_changes`, `app_installs` — Android app lifecycle (Android importer)
`app_changes`: `{package, label, kind: installed|uninstalled|reinstalled|updated, at, tz_offset, device}`; `app_installs`: current state per device and package. `android_apps` also keeps a small `icon` (PNG data URL). See [app-changes.md](app-changes.md).

### `day_notes` — daily summary note (Chrono, any device)
id `n-<YYYY-MM-DD>` (one per local day, so devices converge): `{day: "YYYY-MM-DD", text (≤ 1000 chars), photos: [photo id], updated_at (UTC ms)}`. An empty note is deleted (tombstone; undo restores it). See [notes.md](notes.md).

### `photos` — note photos (Chrono)
id = UUIDv7: `{file, mime: image/webp|jpeg|png, w, h, bytes, sha256, created_at (UTC ms)}`. Bytes live in `<data_dir>/chrono-photos/<file>` (document quality, longest side ≤ 2400 px), not in SQLite. Same scheme as Agenda's photos. Not yet uploaded to S3 (blob sync is a later step for all apps).

### Local only (not synced)
- Table `chrono_daily` (migration `chrono/001_daily`): per-day aggregate cache, rebuilt on demand. Not a record collection, never synced. Expression indexes `chrono_rec_start` / `chrono_rec_hour` on `json_extract(data,'$.start'|'$.hour')`.
- kv `chrono.import.cursor`: next Android import scan start (UTC ms, hour aligned). kv `chrono.import.at`: last import time.
- kv `chrono.alerts`: alert ids already shown today (`{day, ids}`), so each alert fires once per day per device.
- kv `chrono.prefs`: `{setup_done}` (Android setup dismissed).

## Alerts
Rust `alerts.rs`: a limit reached ("Limit reached · Entertainment — 1 h 32 today, your limit is 1 h 30.") or a goal met, and on Monday from 08:00 "Your weekly report is ready". Each fires once per day per device (kv `chrono.alerts`), never while paused, only when `alerts` / `weekly_report` are on. Desktop: a timer thread in the app checks every 60 s while Chrono runs (tauri-plugin-notification). Android: after every import (foreground and the 15-min worker). The agent does not show Chrono alerts yet.

## Build and run
```bash
cd apps/chrono && bun install && bun tauri dev                      # Linux dev window
bunx vite build && cargo build -p chrono-app --features tauri/custom-protocol --bin chrono   # debug binary with embedded UI
cargo test -p chrono-app                                             # 12 model tests
SELFKIT_DATA_DIR=/tmp/chrono-demo cargo run -p chrono-app --example seed            # 240 days (default; --days N), 3 devices, notes, app changes
SELFKIT_DATA_DIR=/tmp/chrono-demo cargo run --release -p chrono-app --example seed -- --bench   # cold/warm timings
dev/ci-apk.sh chrono 0.1.0                                           # signed out/chrono-0.1.0-arm64.apk (GitHub Actions)
```
Screenshots without a window: `dev/chrono-shot.sh <name> <light|dark> <WxH> "<CHRONO_DEMO params>"` (Xvfb on `$CHRONO_DISPLAY`, default :82; params `view=today|timeline|trends|report`, `day=-1`, `week=-7`, `settings=1`, `sources=1`, `note=1`, `setup=1|2`, `pause=1`, `scroll=<px>`). `dev/xin.py` (XTEST via ctypes) clicks and types under Xvfb when xdotool is missing.

## Code map
- `src-tauri/src/lib.rs`: Tauri setup, state (`chrono.db` + shared `settings.db`), commands (`day_view`, `timeline`, `trends`, `report`, `names`, `sources`, `info`, `settings_get/save`, `pause`, `resume`, `theme_get/set`, `prefs_get/set`, `note_get/save`, `photo_save/get/export`, `android_status/open/import`, `dev_hooks`), desktop alert timer, `mobile` module (Kotlin plugin glue), `jni_bridge` (worker entry points), env hooks `CHRONO_DEMO`, `CHRONO_WINDOW_SIZE`.
- `src-tauri/src/stats.rs`: day splitting, unions, daily cache, all screen data ([stats.md](stats.md)).
- `src-tauri/src/settings.rs`: `chrono_settings` model, pause/resume, exclude matching.
- `src-tauri/src/categories.rs`: default categories, browser detection, Focus.
- `src-tauri/src/android.rs`: Android batch → sessions + hourly counters ([android.md](android.md)).
- `src-tauri/src/alerts.rs`: goal/limit/weekly alerts.
- `src-tauri/src/notes.rs`: `day_notes` + photo files ([notes.md](notes.md)).
- `src-tauri/examples/seed.rs` (+ two sample photos): sample data relative to now; `--bench`, `--settings-only`, `--theme`.
- `src-tauri/gen/android/app/src/main/java/com/selfkit/chrono/`: Kotlin (UsagePlugin, Collector, NotifStore, ChronoNotificationListener, ImportWorker, ChronoNative, MainActivity, SelfkitPhotos = `window.SelfkitPhotos` for camera capture cleanup and Save to gallery).
- `src/App.svelte`: phone/desktop shell, navigation, pause menu, Android Back, keyboard, demo hooks. `src/app.css`: tokens (light/dark) and components from the approved design.
- `src/lib/`: `api.ts` (typed commands), `state.svelte.ts` (state, theme, pause, Android refresh), `fmt.ts` (durations, dates, categories), `photos.ts` (scale + encode), `photo-source.ts` (shared camera/gallery sheet, Save to gallery).
- `src/screens/`: `Today`, `Timeline`, `Trends`, `Report`, `Settings`, `Sources`, `Setup`.
- `src/components/`: `DayRibbon`, `LaneChart`, `BarChart`, `Heatmap`, `WeekChart`, `GoalRows`, `TopRows`, `CatBreakdown`, `NoteCard`, `NoteEditor`, `Photo`, `PhotoViewer`, `VList`, `Seg`, `Icon`, `Toast`.

## Testing (2026-11-15)
- Rust: 10 unit tests (day splitting across midnight/timezones/day start, unions, category browser correction, cache invalidation, exclusions, views, goals, alerts once a day, Android session derivation incl. re-import/pause/exclude/listener counts, notes + photos, sources/report extras).
- Linux under Xvfb: every screen in light/dark at phone and desktop width with the 240-day seed; live run with the real `selfkit-agent` on Xvfb + i3: sessions appeared on Today within a minute; Pause from the UI → the agent stopped recording; "Never track" feh → hidden and not recorded; a 2-min total limit fired the alert; a note with a photo (GTK file chooser, JPEG → WebP 1600×1200) survived an app restart; photo viewer opens and closes.
- Android (vivo V2111, Android 12, real phone): first-run setup → Usage access granted in the real Settings page → app moved to step 2 by itself → notification access granted → Today showed the phone's real last 7 days (sessions, unlocks, screen-on, 50 notifications from `NOTIFICATION_INTERRUPTION` events); a 15-min total limit → "Limit reached · screen time" notification; Pause 15 min → no time recorded while paused; Timeline, Trends, Weekly report on a 360 dp screen. WorkManager job is scheduled (JobScheduler shows it running); a run of the JNI import path inside the worker could not be confirmed from logs (release build logs only failures and none appeared).
- App changes, for real on the vivo: a tiny test app (`com.selfkit.chronotest`, built with aapt2/d8, no Gradle) was installed → used 1 min → Chrono showed "Chrono Test · Installed" with its icon; uninstalled → "Removed" + "Uninstalled" tag, usage kept; reinstalled and used → "Reinstalled", all-time use continued (1 → 2 min); removed again at the end (test app no longer on the phone). OPPO (Android 15): setup "Not now" path, note saved with Android Back, system photo picker opens, note deleted again.
- Bugs found and fixed while testing: R8 stripped WorkManager's Room constructor (crash at start), stale `TauriActivity.kt` after `android init`, hero minutes rounded differently from goals, report average divided by 7 on a phone's first week and days before the first data counted as "kept", unfair month-in-progress comparisons, lane order, missing separators, cramped heatmap rows on phones, native selects unstyled, demo hooks racing init, Escape in the note editor dropped typed text (now saves like Back), "Recording on 1 devices" and a misleading "Recording" when phone Usage access is off (now "Phone time is off · Allow").

## Sync (wave 2, 2026-11-15)
Shared module [selfkit-appsync](../../components/selfkit-appsync.md) + Android plugin [selfkit-sync](../../../android-plugins/selfkit-sync/README.md); same UI in every app (`ui/src/sync/`).
- **Settings → Sync**: status ("Synced locally with Linux PC · 2 min ago" / "Synced via server · …" / "Waiting to sync · N changes saved here"), **Sync now**, local sync toggle + seen devices, server setup (Postgres + optional S3 for photos, Test/Save), encryption setup with recovery key / unlock / change passphrase, who syncs (Android: This app | Selfkit companion; desktop: Background agent | This app), battery tip on OPPO/vivo-style phones, sync log.
- **Conflict screen** (Settings → Sync → Review): Keep A (this device) / Keep B / Merge field by field, long text as a line list to tick; edited-vs-deleted explained.
- **When**: on open, every ~15 min (Android WorkManager, needs a network; desktop: the agent), and Sync now. Transport per run: LAN to a reachable Selfkit device, else the server.
- **Stores synced**: `chrono.db` and the shared `settings.db`. Photos (`photos` + `chrono-photos/`) go through S3 when configured.
- **Test builds**: Cargo feature `sync-test-key` (default on) uses the public built-in key, so there are no passphrase prompts; build with `--no-default-features` for real data.
- **Code**: `src-tauri/src/lib.rs` (setup: `selfkit_appsync::tauri_cmds::default_handle("chrono", …)` + `sync_*` commands), Settings row + page wiring in the frontend, `.sks, .skc` token mapping in `src/app.css`, Android: `MainActivity.kt` "Selfkit sync" blocks, `settings.gradle` / `app/build.gradle.kts` module lines, manifest meta-data `com.selfkit.sync.APP/LIB`. Provider: `content://com.selfkit.chrono.provider` (signature permission `com.selfkit.permission.SHARE`).

## Not yet
- Photo files are not uploaded to S3 (records sync; files stay on the device that added them). Orphaned photo files are not cleaned up.
- Desktop alerts only while the Chrono app runs (the agent could take this over).
- Devices and sources cannot see another phone's permission state (shows "Data received").

## Phone QA fixes 2026-11-15 (photos)
- Photo viewer: light status-bar icons while the black viewer is open (`ChronoBars.setDark(true)`, existing bridge method, no MainActivity change), theme colours restored on close (`applyTheme()`). File: `src/components/PhotoViewer.svelte`. Needs a phone re-check in light theme.

## App lock
Optional PIN/passphrase (+ fingerprint on Android, + "Hide content in app switcher") from Settings → App lock; locks on start and after Immediately / 1 / 5 / 15 min away. Shared implementation: [../../components/app-lock.md](../../components/app-lock.md). Hooks here: `<LockGate appName="Chrono">` in `src/App.svelte`, `<LockSettings>` in `src/screens/Settings.svelte` (card after Sync), `LockHandle::new("chrono", …)` + `lock_*` commands in `src-tauri/src/lib.rs`, "---- Selfkit lock" blocks in `MainActivity.kt` + `SelfkitLock.kt`, `.skl` tokens in `src/app.css`, feature `lock-test` in `src-tauri/Cargo.toml`.
