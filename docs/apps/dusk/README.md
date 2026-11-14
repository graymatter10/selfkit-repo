# Dusk (com.selfkit.dusk)

Status: **v0.1, usable on Linux and Android.** Automatic sleep estimate from device use (Chrono data on desktop, UsageStatsManager on Android), manual log (bedtime, wake time, 1–5 quality, notes, naps), trends, consistency, sleep debt, bedtime goal, wind-down reminder, connections (late screen use vs quality; sleep vs next-day tasks and spending), desktop rate-sleep popup. Later: Health Connect (extension point only, see [estimation.md](estimation.md#health-connect)).

Design: approved mockup `dusk-final.html` (2026-11-15; reviewed design + "This week" on Night + number pad in Adjust). Neutral greys, one violet accent (#6552b8 light / #a898f2 dark), red only for errors and destructive actions, Inter (bundled). Light/dark follow the system unless set.

Feature notes: [estimation.md](estimation.md) (how a night is estimated, naps, all-nighters, time zones, day start, connections) · [android.md](android.md) (Usage access bridge, reminders, testing).

## Screens
- **Night** (opens here): "Last night" with ‹ › to other nights, the big duration (or "No sleep" / "Not over yet" / "No data"), a pill saying where it comes from ("Estimated from screen use" / "Times set by you"), a timeline 20:00→12:00 (screen on in grey, asleep in violet, the estimate as a thin line under a correction), Bedtime / Woke up ("was 23:48" after a correction) / Woke briefly ("1 time · 4 min"), Adjust times, Add nap. Then "How did you sleep?" 1–5 (one tap, undo in the toast; tapping the selected one again removes the rating), Notes (Before bed / After waking, "+ Add a note"), naps, and **This week** (7 nights, average, goal line, the night on screen highlighted). Android without Usage access shows an "Allow" card.
- **Adjust times** (phone: page with back arrow and full-width Save; desktop: side panel): Bedtime / Woke up with −15 · field · +15 and "Estimate 23:48", "Asleep 7h 30m", the two notes, "Use the estimate instead". Phone: tapping a time field shows the in-app number pad (1–9, ".", 0, ⌫, am, pm, Next/Done); typing 2330 / 23:30 / 11:30pm works. Desktop: type on the keyboard, ↑/↓ ±15 min, Tab next field, Enter/Ctrl+Enter save, Esc close. Leaving with changes asks "Discard your changes?" (Back, Esc, back arrow). **Add nap** uses the same page (Fell asleep / Woke up).
- **Trends**: 2 weeks / Month / 3 months; average asleep and bars per night (night + nap, goal line; tap a bar to open the night); four stats (average last 7 nights vs the week before, sleep debt over 14 nights, consistency 0–100, bed on time of 7 nights); bed and wake times chart with the bedtime goal; weekly averages (3 months).
- **Connections** ("Last 3 months. A link doesn't prove a cause."): one card per link with topic, strength badge (No clear / Weak / Moderate / Strong), one plain sentence, two compared bars, and "Details" (4-group breakdown + correlation). Missing data source = muted card ("Hisab isn't on this device").
- **Settings**: sleep goal (±15 min), bedtime goal, wind-down reminder (off by default) + lead time 15/30/45/60 min, new day starts at (shared setting), appearance, data sources (Android: Usage access status + Allow; desktop: Chrono / Agenda / Hisab found), rate-sleep shortcut (desktop), Health Connect "Coming later".
- **Desktop**: left nav Night / Trends / Connections (Ctrl+1/2/3), Settings (Ctrl+,); Night shows the night on the left and the last 14 nights ("has notes" dots, click a bar to open) + stats on the right. Keys on Night: 1–5 rate, ←/→ other nights, A adjust, N add nap.
- **Rate-sleep popup** (desktop): global shortcut `rate_sleep` (default Super+Shift+S, shared settings) or `dusk --rate`: "How did you sleep?", 1–5 key or click, saves and closes; Esc/blur closes.

Android Back (Tauri `onBackButtonPress`): closes Adjust (asks first if there are changes), then Settings, then Trends/Connections → Night, then an older night → last night, then exits.

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
- `hisab.db` (if the file exists): `transactions` `{kind, amount, amount_main, date, time}` → spent per logical day (expenses only, `amount_main` preferred). Any other shape is skipped (the card says no data).
- On Android these files are not on the device, so the two next-day connections say "isn't on this device".

## Build and run
```bash
cd apps/dusk && bun install
bun tauri dev                                              # Linux dev window (port 5180)
cargo build -p dusk --features tauri/custom-protocol        # debug binary with embedded UI
cargo test -p dusk                                          # model, sources, data tests
SELFKIT_DATA_DIR=/tmp/dusk-demo cargo run -p dusk --example dusk_seed     # ~4 months of sample data (+ chrono/agenda/hisab DBs, day start 04:00)
SELFKIT_DATA_DIR=/tmp/dusk-demo cargo run -p dusk --example dusk_report   # print nights, stats, connections
```
Screenshots without a window: Xvfb on :84, then `DUSK_SHOT_DATA=/tmp/dusk-demo apps/dusk/scripts/shot.sh <name> <system|light|dark> <WxH> "<DUSK_DEMO>" [--rate]` (DUSK_DEMO: `tab=night|trends|connections|settings`, `back=<nights>`, `edit=adjust|nap`, `focus=bed|wake|note_before`). Android: [android.md](android.md).

## Code map
- `src-tauri/src/model.rs`: pure model — spans, night windows, `estimate`, `night_view` (log/external/estimate priority), stats, weekly averages, connections; unit tests.
- `src-tauri/src/sources.rs`: read chrono.db / agenda.db / hisab.db, `device_use`, Android events → intervals, `save_device_use`; tests.
- `src-tauri/src/data.rs`: `sleep_log` set/replace/naps, goals, device prefs, `dusk_nights` cache, migrations, user-facing `Error`; tests.
- `src-tauri/src/health.rs`: Health Connect extension point (empty).
- `src-tauri/src/lib.rs`: Tauri commands (`dashboard`, `night`, `connections`, `log_set`, `log_replace`, `nap_*`, `goals_set`, `device_*`, `settings_*`, `android_import`, `sources_status`), `compute`, `reminders` (Android exact alarms / desktop timer), `shortcut` (global shortcut + popup), env hooks `DUSK_DEMO`, `DUSK_WINDOW_SIZE`.
- `src-tauri/examples/dusk_seed.rs`, `dusk_report.rs`: sample data, debugging report.
- `src-tauri/gen/android/`: generated Android project, customized: `DuskUsage.kt` (bridge), `MainActivity.kt`, manifest (usage stats, notifications, exact alarms, `adjustResize`), icons, window background, proguard keep rule.
- `src/main.ts` (App or Capture), `src/app.css` (tokens), `src/App.svelte` (layout, nav, Back, keys, hooks), `src/Capture.svelte` (popup).
- `src/lib/api.ts` (commands + types), `state.svelte.ts` (state, rate/log/naps with undo, prefs/theme, Android import), `fmt.ts` (time/duration formatting), `icons.ts`.
- `src/components/`: `NightPage`, `Adjust` (times/notes/nap + number pad), `TrendsPage`, `ConnectionsPage`, `SettingsPage`, `Timeline`, `NightBars`, `RangeChart`, `StatsGrid`, `Rating`, `Seg`, `Icon`, `Toast`, `Confirm`.
- `scripts/shot.sh`: off-screen screenshots.
