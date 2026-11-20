# Dusk (com.selfkit.dusk)

Status: **v0.1.1, usable on Linux and Android.** (0.1.1 = QA fixes, see [QA 2026-11-15](#qa-2026-11-15).) Automatic sleep estimate from device use (Chrono data on desktop, UsageStatsManager on Android), manual log (bedtime, wake time, 1–5 quality, notes, naps), trends, consistency, sleep debt, bedtime goal, wind-down reminder, connections (late screen use vs quality; sleep vs next-day tasks and spending), desktop rate-sleep popup. Later: Health Connect (extension point only, see [estimation.md](estimation.md#health-connect)).

Design: approved mockup `dusk-final.html` (2026-11-15; reviewed design + "This week" on Night; the Adjust number pad was replaced by a clock dial on 2026-11-15, see [Time picker](#time-picker-clock-dial)). Neutral greys, one violet accent (#6552b8 light / #a898f2 dark), red only for errors and destructive actions, Inter (bundled). Light/dark follow the system unless set.

Feature notes: [estimation.md](estimation.md) (how a night is estimated, naps, all-nighters, time zones, day start, connections) · [android.md](android.md) (Usage access bridge, reminders, testing).

## Screens
- **Night** (opens here): "Last night" with ‹ › to other nights, the big duration (or "No sleep" / "Not over yet" / "No data"), a pill saying where it comes from ("Estimated from screen use" / "Times set by you"), a timeline 20:00→12:00 (screen on in grey, asleep in violet, the estimate as a thin line under a correction), Bedtime / Woke up ("was 23:48" after a correction) / Woke briefly ("1 time · 4 min"), Adjust times, Add nap. Then "How did you sleep?" 1–5 (one tap, undo in the toast; tapping the selected one again removes the rating), Notes (Before bed / After waking, "+ Add a note"), naps, and **This week** (7 nights, average, goal line, the night on screen highlighted). Android without Usage access shows an "Allow" card.
- **Adjust times** (phone: page with back arrow and full-width Save; desktop: side panel): Bedtime / Woke up with −15 · field · +15 and "Estimate 23:48", "Asleep 7h 30m", the two notes, "Use the estimate instead". Phone: tapping a time opens the [clock dial](#time-picker-clock-dial) in a bottom sheet (Cancel/Done); an empty pair (Add nap, "Enter times" with no estimate) opens it straight away and goes on from the first time to the second. Desktop: the field can still be typed (2330 / 23:30 / 11:30pm), ↑/↓ ±15 min, Alt+↓ or the clock button opens the dial as a small dialog, Tab next field, Enter/Ctrl+Enter save, Esc close. A wake time not after bedtime (typed pair, or typed time + estimate) shows "Wake time must be after bedtime (…)" under the fields, same rule as `log_set`. Leaving with changes asks "Discard your changes?" (Back, Esc, back arrow). **Add nap** uses the same page (Fell asleep / Woke up).
- **Trends**: 2 weeks / Month / 3 months; average asleep and bars per night (night + nap, goal line; tap a bar to open the night); four stats (average last 7 nights vs the week before, sleep debt over 14 nights, consistency 0–100, bed on time of 7 nights); bed and wake times chart with the bedtime goal; weekly averages (3 months).
- **Connections** ("Last 3 months. A link doesn't prove a cause."): one card per link with topic, strength badge (No clear / Weak / Moderate / Strong), one plain sentence, two compared bars, and "Details" (4-group breakdown + correlation). Missing data source = muted card ("Hisab wasn't found on this device. Install or open Hisab to see this link." · "Hisab isn't on this device"); on a computer without Chrono the late-screen card says so instead of "from Chrono" (L4-10).
- **Settings**: sleep goal (±15 min), bedtime goal (tap → clock dial, saved on Done), wind-down reminder (off by default) + lead time 15/30/45/60 min, new day starts at (shared setting), appearance, data sources (Android: Usage access status + Allow; desktop: Chrono / Agenda / Hisab found), rate-sleep shortcut (desktop), Health Connect "Coming later".
- **Desktop**: left nav Night / Trends / Connections (Ctrl+1/2/3), Settings (Ctrl+,); Night shows the night on the left and the last 14 nights ("has notes" dots, click a bar to open) + stats on the right. Keys on Night: 1–5 rate, ←/→ other nights, A adjust, N add nap.
- **Rate-sleep popup** (desktop): global shortcut `rate_sleep` (default Super+Shift+S, shared settings) or `dusk --rate`: "How did you sleep?", 1–5 key or click, saves and closes; Esc/blur closes.

Android Back (Tauri `onBackButtonPress`): closes an open clock dial (= Cancel), then Adjust (asks first if there are changes), then Settings, then Trends/Connections → Night, then an older night → last night, then exits.

## Time picker (clock dial)
Every time in Dusk (Adjust: bedtime/fell asleep and woke up; Settings: bedtime goal) is picked on one clock dial (`src/components/TimeDial.svelte`, wrapped by `TimeSheet.svelte`). The keypad is gone (user: "select time from a clock", 2026-11-15).
- **Readout** on top: big hour and minutes; the one being set is tinted violet; tap either to switch.
- **Face**: tap or drag the hour first. 24h: 1–12 on the outer ring, 13–23 and 00 on the inner ring. 12h: one ring plus an am/pm toggle next to the readout. Releasing the hour moves on to minutes by itself (260 ms).
- **Minutes**: 00, 05 … 55 labelled, dots for single minutes. A drag snaps to the 5-minute mark when within ±1 minute of it, otherwise to the exact minute (so 1-minute precision by aiming between marks); the knob shows a dot when off a mark.
- **Hand** turns the short way round with a 220 ms ease (70 ms while dragging); off with reduced motion.
- **Keys**: ←↓ / ↑→ −/+1 (Shift: 5 minutes, PageUp/PageDown ±5), digits type minutes, Tab hour → minutes → am/pm → Cancel → Done (focus stays in the dialog), Enter = Done, Esc = Cancel (doesn't close the Adjust panel behind it).
- **Container**: phone (< 900 px) bottom sheet with handle, title, "Estimate 23:48" hint, Cancel / Done; desktop a 340 px dialog with a key hint line. While open it takes over `app.closeGuard`, so Android Back = Cancel, and hands the previous guard (Adjust's "Discard your changes?") back on close.
- **12h / 24h**: there is no app or shared setting, so it follows the system locale (`Intl` hour cycle; `uses12h()` in `lib/fmt.ts`). In 12h the time fields and the wind-down line show "11:30 pm"; values are still stored as "HH:MM". Android WebView reports the language's default, not the system 24-hour switch.
- Starting value: the field's time, else the estimate, else 23:00 / 07:00 (nap: 14:00, and the wake time starts 30 min after "Fell asleep" — `current()` in `Adjust.svelte`; re-verified 2026-11-15, the phone build that showed 2:30 pm after 4:15 pm predates it).
- Tested 2026-11-15 on Linux (Xvfb: drag hour on the inner ring, drag minutes incl. a 1-minute stop, Alt+↓/arrows/Tab/Enter/Esc on desktop) and on the vivo (720×1600, 12h locale, APK 0.1.2): `adb shell input swipe` on the dial, nap added and deleted, bedtime goal changed and restored, Back closes the sheet (Adjust and Settings), wake-before-bed message, dark mode.

## Data model (`dusk.db`, selfkit-core records)

All times are **UTC ms**; `tz_offset` = local offset in **minutes east of UTC** at that moment.
A **night** is keyed by the local calendar date of the morning it ends in (`"YYYY-MM-DD"`, the wake date). Its search window is `[previous day 18:00, that day 18:00)` local time.

### `sleep_log` — manual entries (synced)
`{night, kind: "main"|"nap", bed, wake, tz_offset, quality, note_before, note_after, created_at}`
- `kind = "main"`: the correction/rating/notes for a night. Id is deterministic `n-<night>` so devices converge on one record. `bed`/`wake` (UTC ms) may be `null` = "use the estimate" (e.g. only a rating or a note was given).
- `kind = "nap"`: extra sleep on day `night` (random UUIDv7 id); `bed` and `wake` required, ≤ 6 h.
- `quality`: 1–5 or `null` (1 Awful · 2 Poor · 3 OK · 4 Good · 5 Great).
- `note_before` ("Before bed") / `note_after` ("After waking"): text ≤ 500 chars or `null`.
- Rules: wake after bed; ≤ 20 h. Typed times are placed on the night with the night's own offset (evening times ≥ 18:00 on the day before).
- Undo: the frontend keeps the previous entry and calls `log_replace`; nap delete is a tombstone (`nap_restore` = undo).

### `device_use` — screen-on intervals from the phone (synced)
One record per device per night, id `u-<device>-<night>`; re-imports merge (union) into it.
`{night, device, platform: "android", tz_offset, intervals: [[start, end], ...], source: "usage_stats"}`
- Written by Dusk on Android from UsageStatsManager. Desktop Dusk unions these with Chrono's sessions, so the estimate covers every device.

### `dusk_settings` — goals (synced, single record id `settings`)
`{goal_min: 480, bedtime: "23:00", winddown_min: 30}`. Missing record/field = default.

### Local only (not synced)
- kv `dusk.device`: `{winddown_on: false, theme: "system"|"light"|"dark", import_cursor}` — per device (the reminder is per device, off by default).
- Table `dusk_nights` (migration `dusk/001_nights`): per-night result cache, rewritten whenever Dusk computes a night. **Reflect can read it** (read-only): `night TEXT PK, bed INT, wake INT, tz_offset INT, asleep_min INT, nap_min INT, quality INT NULL, source TEXT ('estimate'|'logged'|'none'|'allnighter'), late_use_min INT, updated_at INT`.
- Shared settings (`settings.db`): reads/writes `day_start_min`, reads `main_currency`, `shortcuts.rate_sleep`.

## Reads (read-only)
- `chrono.db`: `app_sessions` (all platforms, incl. Android sessions synced from Chrono) and `web_sessions` — `{start, end, tz_offset}`.
- `agenda.db`: `tasks` with `done_at` (tasks done per logical day; days between the first task and today count 0 when nothing was done).
- `hisab.db` (if the file exists): `transactions` `{kind, amount, currency, amount_main, date, time}` → spent per logical day (expenses only). `amount_main` if present, else `amount` converted from `currency` into the main currency with Hisab's `rates` record (`hisab-rates`; Hisab never stores `amount_main`); a currency without a rate is left out. Any other shape is skipped (the card says no data).
- On Android Dusk reads read-only copies of Agenda and Hisab made by `SelfkitMirror` (manifest `com.selfkit.sync.MIRROR=agenda,hisab`; refreshed on open, when Dusk comes back to the foreground and in the background job) at the same paths. Settings → Tasks and spending says Found / Agenda only / Hisab only / Not found; a missing app's next-day card says "isn't on this device" (B4).

## Build and run
```bash
cd apps/dusk && bun install
bun tauri dev                                              # Linux dev window (port 5180)
cargo build -p dusk --features tauri/custom-protocol        # debug binary with embedded UI
cargo test -p dusk                                          # model, sources, data tests
SELFKIT_DATA_DIR=/tmp/dusk-demo cargo run -p dusk --example dusk_seed     # ~4 months of sample data (+ chrono/agenda/hisab DBs, day start 04:00)
SELFKIT_DATA_DIR=/tmp/dusk-demo cargo run -p dusk --example dusk_report   # print nights, stats, connections
```
Screenshots without a window: Xvfb on :84, then `DUSK_SHOT_DATA=/tmp/dusk-demo apps/dusk/scripts/shot.sh <name> <system|light|dark> <WxH> "<DUSK_DEMO>" [--rate]` (DUSK_DEMO: `tab=night|trends|connections|settings`, `back=<nights>`, `edit=adjust|nap`, `focus=bed|wake|note_before`, `dial=a|b` (Adjust) or `dial=goal` (Settings) opens the clock dial, `clock=12|24` forces the dial format). Android: [android.md](android.md).

## Code map
- `src-tauri/src/model.rs`: pure model — spans, night windows, `estimate`, `night_view` (log/external/estimate priority), stats, weekly averages, connections; unit tests.
- `src-tauri/src/sources.rs`: read chrono.db / agenda.db / hisab.db, `device_use`, Android events → intervals, `save_device_use`; tests.
- `src-tauri/src/data.rs`: `sleep_log` set/replace/naps, goals, device prefs, `dusk_nights` cache, migrations, user-facing `Error`; tests.
- `src-tauri/src/health.rs`: Health Connect extension point (empty).
- `src-tauri/src/recover.rs`: damaged DB → set aside as `<name>.damaged-<ms>`, fresh DB, one-time toast (`startup_notice`).
- `src-tauri/src/lib.rs`: Tauri commands (`dashboard`, `night`, `connections`, `log_set`, `log_replace`, `nap_*`, `goals_set`, `device_*`, `settings_*`, `android_import`, `sources_status`), `compute`, `reminders` (Android exact alarms / desktop timer), `shortcut` (global shortcut + popup), env hooks `DUSK_DEMO`, `DUSK_WINDOW_SIZE`.
- `src-tauri/examples/dusk_seed.rs`, `dusk_report.rs`: sample data, debugging report.
- `src-tauri/gen/android/`: generated Android project, customized: `DuskUsage.kt` (bridge), `MainActivity.kt`, manifest (usage stats, notifications, exact alarms, `adjustResize`), icons, window background, proguard keep rule.
- `src/main.ts` (App or Capture), `src/app.css` (tokens), `src/App.svelte` (layout, nav, Back, keys, hooks), `src/Capture.svelte` (popup).
- `src/lib/api.ts` (commands + types), `state.svelte.ts` (state, rate/log/naps with undo, prefs/theme, Android import), `fmt.ts` (time/duration formatting), `icons.ts`.
- `src/components/`: `NightPage`, `Adjust` (times/notes/nap), `TimeDial` (clock face + readout), `TimeSheet` (dial in a bottom sheet / desktop dialog, Back/Esc/Enter), `TrendsPage`, `ConnectionsPage`, `SettingsPage`, `Timeline`, `NightBars`, `RangeChart`, `StatsGrid`, `Rating`, `Seg`, `Icon`, `Toast`, `Confirm`.
- `scripts/shot.sh`: off-screen screenshots.

## Sync (wave 2, 2026-11-15)
Shared module [selfkit-appsync](../../components/selfkit-appsync.md) + Android plugin [selfkit-sync](../../../android-plugins/selfkit-sync/README.md); same UI in every app (`ui/src/sync/`).
- **Settings → Sync**: status ("Synced locally with Linux PC · 2 min ago" / "Synced via server · …" / "Waiting to sync · N changes saved here"), **Sync now**, local sync toggle + seen devices, server setup (Postgres + optional S3 for photos, Test/Save), encryption setup with recovery key / unlock / change passphrase, who syncs (Android: This app | Selfkit companion; desktop: Background agent | This app), battery tip on OPPO/vivo-style phones, sync log.
- **Conflict screen** (Settings → Sync → Review): Keep A (this device) / Keep B / Merge field by field, long text as a line list to tick; edited-vs-deleted explained.
- **When**: on open, every ~15 min (Android WorkManager, needs a network; desktop: the agent), and Sync now. Transport per run: LAN to a reachable Selfkit device, else the server.
- **Stores synced**: `dusk.db` and the shared `settings.db`.
- **Test builds**: Cargo feature `sync-test-key` (default on) uses the public built-in key, so there are no passphrase prompts; build with `--no-default-features` for real data.
- **Code**: `src-tauri/src/lib.rs` (setup: `selfkit_appsync::tauri_cmds::default_handle("dusk", …)` + `sync_*` commands), Settings row + page wiring in the frontend, `.sks, .skc` token mapping in `src/app.css`, Android: `MainActivity.kt` "Selfkit sync" blocks, `settings.gradle` / `app/build.gradle.kts` module lines, manifest meta-data `com.selfkit.sync.APP/LIB`. Provider: `content://com.selfkit.dusk.provider` (signature permission `com.selfkit.permission.SHARE`).

## QA 2026-11-15
Tested with the default seed (120 nights; the user asked not to use a year for Dusk) on Linux and the 0.1.0 APK on the vivo.
- Linux: open ≈ 1.1 s; screen switches 70–190 ms CPU; ~215 MB PSS total, ~380 MB summed RSS.
- Fixed: changing only one time in Adjust (e.g. bedtime 03:00 while the estimated wake was 02:52) was saved and the night showed "No data" with no way back — `log_set` now checks the bed/wake pair that will show and refuses with "Wake time must be after bedtime" (and "Use the estimate" shows whenever a time was typed); the spending connection summed Hisab amounts in other currencies as if they were the main currency (e.g. ৳3,500 counted as $3,500); damaged `dusk.db`/`settings.db` crash at start; two desktop instances (single-instance plugin, `--rate` goes to the running one).
- Verified: unreadable `chrono.db` / truncated `agenda.db` → Dusk still opens, the cards degrade; Usage access denied → "Allow" card; rapid rating taps end in a consistent saved state; invalid typed time (9999) refused.

## Phone QA fixes 2026-11-15
- Nap rows and the Night page times (bed/wake cells, "was …", last screen use, timeline ticks and its label) use the same 12h/24h rule as the pickers (`hmd()` = `tdisp(hm())` in `lib/fmt.ts`), so a 12h phone no longer shows "16:15–17:30" next to a "4:15 pm" picker. Timeline ticks read "3 am" in 12h.
- 2026-11-21 (Linux hunt): "Bed on time" card and the "New day starts at" list follow the 12h/24h rule too (L4-2); desktop time fields are wider with a smaller font in 12h so "11:30 pm" isn't clipped under the clock button (L4-1); "1 night with data" (L4-3); Sleep debt shows "–" with no nights instead of "None" (L4-4); on the dial, minute arrow keys carry into the hour (10:59 → 11:00, L4-5). A nap that overlaps the night's sleep (this night's or the next one's) or another nap is refused (L4-6). Times later than now are refused for the night and for naps (L4-8). Esc closes Adjust / Add nap wherever focus is (L4-7). Consistency says "Needs 3 nights" until the score exists (L4-9); Bed on time shows "–" with no nights; the Trends legend uses 12h too.
- The nap screen header says which day: "Add nap" · "Nap · Sun 15 Nov" (was the night range "Sat 14 – Sun 15 Nov"); naps belong to the day of the night's morning.
- Default wake time for a nap = fell asleep + 30 min: already in the tree (clock-dial work), verified in code.
- Needs a phone re-check (12h locale: nap row format, header).

## App lock
Optional PIN/passphrase (+ fingerprint on Android, + "Hide content in app switcher") from Settings → App lock; locks on start and after Immediately / 1 / 5 / 15 min away. Shared implementation: [../../components/app-lock.md](../../components/app-lock.md). Hooks here: `<LockGate appName="Dusk">` in `src/App.svelte`, `<LockSettings>` in `src/components/SettingsPage.svelte` (after Data), `LockHandle::new("dusk", …)` + `lock_*` commands in `src-tauri/src/lib.rs`, "---- Selfkit lock" blocks in `MainActivity.kt` + `SelfkitLock.kt`, `.skl` tokens in `src/app.css`, feature `lock-test` in `src-tauri/Cargo.toml`. The capture popup is not gated.
