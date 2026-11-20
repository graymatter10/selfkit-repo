# Reflect (com.selfkit.reflect)

Status: **v0.1, usable on Linux and Android** (on a phone Reflect reads read-only mirrors of the other apps through their providers, `SelfkitMirror`). Read-only stats across Chrono, Hisab, Agenda and Dusk: Today, streaks with your own thresholds, connections, weekly and monthly reviews, and the health screen for the agent and sync. Scope: [PLANNING.md](../../PLANNING.md) § Apps → Reflect. Stats rules: [stats.md](stats.md).

Desktop: one instance (`tauri-plugin-single-instance`; a second launch focuses the first window, skipped when `SELFKIT_DATA_DIR` is set), like Agenda/Hisab/Dusk (C23).

Design (2026-11-15, self-reviewed; the user was asleep and asked not to wait for approval): mockup `scratchpad/reflect/reflect-mockup.html` (contact sheets `contact-phone.png`, `contact-desktop.png`, `contact-dark.png`). Warm neutral greys, one amber accent (#a35d10 light / #e7aa5e dark) that matches the icon. App colours only as tiny "● Chrono" markers that say where a number came from. Red only for errors. Inter (bundled), plain sentences, one big number per card, details behind a tap. Targets ≥ 44 px, text ≥ 12 px. Light/dark follow the system unless set (per device).

## Screens
- **Today** (opens here): date, one headline ("A calm day so far.") and one plain sentence. Four tiles: Screen time (vs a usual same weekday), Spent (vs today's budget), Tasks (done + left today), Last night (sleep, how it felt, bed–wake). "Your day": a 24-hour strip per source (asleep, screens, spending dots, task dots), a now line, the rest of the day hatched. Most used today (5 apps with device). A missing app shows a dashed tile ("Hisab isn't installed") and a note; with no data at all a calm empty page explains what Reflect needs.
- **Streaks**: one card per streak turned on: current run, best run, last 21 days (phone) / 30 days (desktop) as squares (kept = accent, today = outlined while it's still open), "Kept N of the last M days", Change → Settings. "New streak". A dashed hint "Later: habits you tick yourself" (not a feature yet).
- **Connections**: like Dusk's reviewed cards. Topic, strength badge (No clear / Weak / Moderate / Strong link), one sentence, two compared bars, "N days · ● Chrono ● Dusk", Details (4-group breakdown + correlation, "A link doesn't prove a cause"). Five links: late phone use & sleep, screen time & tasks, late phone use & next-day spending, sleep & next-day screen time, sleep & spending. A link whose app is missing is a dashed "Needs Dusk" card; an installed app without data yet says "No Chrono data yet" (L30), and the strength badge only shows once there is enough data. Monthly "Worth knowing" names a share as most (> 75 %), more than half (> 55 %), almost half, about a third/quarter (L29). Strongest first.
- **Review** (phone: one tab with Week | Month; desktop: Weekly review, Monthly review): ‹ › between periods (opens on the **current** week/month, "so far"; ‹ goes back to the period holding the oldest data and always at least one step, empty periods say "No data for this week."; › stops at the current period — `model::earliest_offset`, QA 2026-11-15). Headline + sentence ("A lighter week on screens." / "You finished 23 tasks, slept 7 h 02 a night on average and kept your budget on 4 of 7 days."). Week: a card per source with 7 bars (highest day in the accent, daily budget as a dashed line on Spent; tap a bar for its value), Worth knowing (heaviest screen day + top app, biggest spend, late nights), Streaks this week. Month: summary rows, "Each day" calendar shaded by Screen time / Spent / Sleep / Tasks (tap a day), Worth knowing (longest streak run, top spending category and its share, heaviest screen day), most used apps a day on average, streaks.
- **Health** (phone: icon top right; desktop: bottom of the nav; a dot shows amber for a warning, red for an error): status card ("Everything is running" / "Something needs a look" / "The agent isn't running"), Tracking, Last sync (transport, changes waiting), Local network (devices), Last backup (target, weekly copy), Agent (uptime, memory, version), Sync now / Back up now (agent IPC), Problems this week (agent error log, ERROR red, WARN grey), Storage (server DB meter with the 80 % mark and warning text, this PC per app). Without the agent: "No agent on this PC" + how to install it; on a phone: "The agent runs on your PCs" + what's stored on the phone.
- **Settings**: Streaks (each with a − value + stepper and a switch; New streak: phone under / all screens under / asleep by / tasks done / spending within budget; added ones can be removed with undo), Data sources (found + since when — Dusk counts from its first night with data, not empty cache rows (L31) — or "Not installed" / "No data yet"; on phones "installed" means a mirror exists. Today tiles likewise say "No Dusk data yet · Nothing for today so far." for an installed app without data today, and the "isn't on this device" hint only lists apps that aren't installed — QA-R1), Connections look back (1 month / 3 months / 1 year), Week starts on (Monday / Sunday), Day starts at (shared, read-only here), Theme (System / Light / Dark).

Keyboard (desktop): Ctrl+1–5 pages, Ctrl+, Settings, Ctrl+R reload. Data refreshes every minute while the window is visible.
Android Back: New-streak chooser → Health/Settings → back where you came from → Today → leaves the app.

## Data model
Reflect writes only its own `reflect.db`:
- `reflect_settings` (synced, single record id `settings`): `{streaks: [{id, kind, value, on}], lookback_days: 30|90|365, week_start: 0 (Monday)|6 (Sunday)}`. `kind`: `phone_under` / `screen_under` (value = minutes a day), `asleep_by` (value = local minute of the day, 0 = midnight), `tasks_min` (value = count), `budget` (value unused; uses Hisab's budgets). Defaults: phone under 3 h, budget, asleep by midnight, at least 3 tasks (on), all screens under 5 h (off). Added streaks have ids `s-…`.
- kv `reflect.prefs` (not synced): `{theme: "system"|"light"|"dark"}`.

Reads (read-only, each optional; a missing file = that app isn't installed):
- `chrono.db`: `app_sessions`, `web_sessions`, `chrono_settings` (exclude lists, device names). Kept in memory per day and updated from records whose `updated_at` changed (a year of sessions is read once).
- `hisab.db`: `transactions` (expenses, converted to the main currency with `hisab-rates`), `categories` (names, monthly budgets).
- `agenda.db`: `completions` (falls back to `tasks.done_at`), open `tasks` with a start date (left for today).
- `dusk.db`: table `dusk_nights` (Dusk's per-night result cache). On a phone it is a copy Dusk's provider hands to the mirror (`/table?name=dusk_nights`, B2), as fresh as Dusk's last estimate.
- `settings.db` (shared): `day_start_min`, `main_currency`.
- `<data_dir>/health.json` (agent, schema 1) and the agent IPC socket (`sync_now`, `backup_now`).

## Build and run
```bash
cd apps/reflect && ../../dev/heavy.sh bun 1500 bun install
bun tauri dev                                                   # Linux dev window (port 5184)
bunx vite build && cargo build -j 2 -p reflect --features tauri/custom-protocol   # debug binary with embedded UI
cargo test -j 2 -p reflect                                      # model + reader tests
# a year of sample data (other apps' seeds first, then Reflect's: dusk_nights + health.json)
export SELFKIT_DATA_DIR=/shared/selfkitextra/reflect-demo
cargo run -p chrono-app --example seed -- --days 365
cargo run -p hisab --example seed
cargo run -p agenda --example seed -- --year
cargo run -p reflect --example reflect_seed -- --health warn   # also adds late-night phone use to chrono.db; flags: --health ok|warn|error|stale|none, --health-only, --no-late, --no-dusk, --theme X --theme-only
cargo run -p reflect --example reflect_bench                    # cold/warm load timings
dev/ci-apk.sh reflect 0.1.0                                     # signed out/reflect-0.1.0-arm64.apk (GitHub Actions)
```
Screenshots: `REFLECT_SHOT_DATA=… apps/reflect/scripts/shot.sh <name> <light|dark> <WxH> "<REFLECT_DEMO params>"` (Xvfb on `$REFLECT_DISPLAY`, default :88). Test hooks (desktop env): `REFLECT_WINDOW_SIZE=WxH`, `REFLECT_DEMO` = `tab=today|streaks|connections|week|month|health|settings`, `offset=-n`, `open=<connection id>`, `adding=1`, `scroll=<px>`; `REFLECT_NOW=YYYY-MM-DDTHH:MM` pretends it's that local time (screenshots of a full day).

## Testing (2026-11-15)
- Rust: 13 unit tests (day split across midnight with day start 04:00, unions, late-night use, excludes and device names, incremental Chrono cache incl. edits, Hisab currency/budgets/categories, Agenda completions/open tasks, streak runs and open days, asleep-by across midnight, connections finding a real link and ignoring noise, week/month periods incl. Sunday-first, weekly review sentences, settings round trip).
- Performance (debug build, a year: 35 k Chrono records): cold load 0.70 s, warm 6 ms; Hisab + Agenda + Dusk 51 ms.
- Linux under Xvfb (:88), year of seeded data: every screen in light and dark at phone (400×860) and desktop (1300×820) width, empty states (Hisab and Dusk missing; nothing installed; no agent). Live run with the real `selfkit-agent` (LAN off, own socket dir): Health showed its real state ("Tracking is running", sync not set up, backup failed → red dot); Sync now / Back up now reach the agent over IPC. Driven with real input (XTest): Ctrl+1–5 and Ctrl+, pages, streak stepper, New streak → added, remove → "Streak removed · Undo" → restored, previous week ‹, Details.
- Android (OPPO CPH2565, Android 15; final APK also on vivo V2111, Android 12, 720×1600): installed the CI APK; every tab, Health and Settings show clear "not shared with phones yet" states; dark theme incl. status-bar icons; Back chain Settings → Review → Today → leaves the app; settings and theme survive a force-stop.
- Bugs found and fixed while testing: "less screen time" claimed early in the morning; seed `examples/seed` name clashed with the other apps' seeds (renamed `reflect_seed`); server DB shown as "1 KB" when sync isn't set up; "Everything is running" without sync/backup set up; Sync now / Back up now said "done" though the agent only starts the job (now "started" + health reloads); monthly streak squares overflowed (now a bar); budgets shown with cents; phone settings rows cramped (stepper on its own line); phone missing-data texts said "install" (now "not shared with phones yet").

## Code map
- `src-tauri/src/lib.rs`: Tauri setup, state (`reflect.db`, shared `settings.db`, per-app caches), `with_ctx` (refresh caches → day rows), commands (`overview`, `streaks`, `connections`, `review`, `health`, `agent_cmd`, `sources_status`, `settings_get/set`, `shared_get`, `prefs_get/set`, `dev_hooks`), `RSettings`.
- `src-tauri/src/sources.rs`: read-only readers: Chrono (filters, day split, unions, late-night phone use, per-app time, incremental `ChronoCache`), Hisab (currency, budgets, categories), Agenda (completions, open tasks), Dusk (`dusk_nights`).
- `src-tauri/src/model.rs`: `Day` rows, formatting, Today (tiles, headline, ribbon), streaks, connections (Pearson + groups), weekly/monthly reviews.
- `src-tauri/src/health.rs`: `health.json` + agent IPC client (Unix socket / Windows pipe).
- `src-tauri/examples/reflect_seed.rs`: late-night phone sessions in `chrono.db`, `dusk_nights` from Chrono's phone sessions, sample `health.json`, theme. `reflect_bench.rs`: load timings.
- `src-tauri/gen/android/`: generated project (customized: icons, `MainActivity.kt` with the `ReflectBars` status-bar bridge, proguard keep rule).
- `src/App.svelte`: phone/desktop shell, nav, Android Back, keyboard, test hooks. `src/app.css`: tokens (light/dark) and shared classes.
- `src/lib/`: `api.ts` (typed commands), `state.svelte.ts` (pages, loading, refresh, settings, theme, agent actions), `fmt.ts`, `icons.ts`.
- `src/components/`: pages `TodayPage`, `StreaksPage`, `ConnectionsPage`, `ReviewPage`, `HealthPage`, `SettingsPage`; parts `Tile`, `Ribbon`, `AppList`, `Bars`, `Calendar`, `StreakCard`, `ConnCard`, `Src` (app marker), `Seg`, `Icon`, `Toast`.

## Sync (wave 2, 2026-11-15)
Shared module [selfkit-appsync](../../components/selfkit-appsync.md) + Android plugin [selfkit-sync](../../../android-plugins/selfkit-sync/README.md); same UI in every app (`ui/src/sync/`).
- **Settings → Sync**: status ("Synced locally with Linux PC · 2 min ago" / "Synced via server · …" / "Waiting to sync · N changes saved here"), **Sync now**, local sync toggle + seen devices, server setup (Postgres + optional S3 for photos, Test/Save), encryption setup with recovery key / unlock / change passphrase, who syncs (Android: This app | Selfkit companion; desktop: Background agent | This app), battery tip on OPPO/vivo-style phones, sync log.
- **Conflict screen** (Settings → Sync → Review): Keep A (this device) / Keep B / Merge field by field, long text as a line list to tick; edited-vs-deleted explained.
- **When**: on open, every ~15 min (Android WorkManager, needs a network; desktop: the agent), and Sync now. Transport per run: LAN to a reachable Selfkit device, else the server.
- **Stores synced**: `reflect.db` and the shared `settings.db`.
- **Test builds**: Cargo feature `sync-test-key` (default on) uses the public built-in key, so there are no passphrase prompts; build with `--no-default-features` for real data.
- **Code**: `src-tauri/src/lib.rs` (setup: `selfkit_appsync::tauri_cmds::default_handle("reflect", …)` + `sync_*` commands), Settings row + page wiring in the frontend, `.sks, .skc` token mapping in `src/app.css`, Android: `MainActivity.kt` "Selfkit sync" blocks, `settings.gradle` / `app/build.gradle.kts` module lines, manifest meta-data `com.selfkit.sync.APP/LIB`, plus `com.selfkit.sync.MIRROR=chrono,hisab,agenda,dusk`: on the phone Reflect refreshes read-only copies of those apps (from their providers) on open, when it comes back to the foreground, and in the background job, into `<data dir>/<app>.db`, the same files it reads on the desktop. Provider: `content://com.selfkit.reflect.provider` (signature permission `com.selfkit.permission.SHARE`).

## Not yet
- Android: other apps' data isn't reachable on a phone (each app's DB is private to it), so Reflect shows empty states there. Needs a data-sharing step later (e.g. synced copies or a content provider).
- Health shows this PC only; other devices' agents aren't visible (the agent doesn't sync its health).
- Manual habits (gym, reading): only the "Later" hint.

## Phone QA fixes 2026-11-15
- Review navigation was stuck: on "November 2026 so far" (and 9–15 Nov) ‹ was disabled because data began inside the current period, and both tabs opened on the previous period. Now both open on the current period (`state.svelte.ts` `offset: {week: 0, month: 0}`); ‹ is allowed while the period starts after the oldest data day, and always for at least one step back (`earliest_offset` in `src-tauri/src/model.rs`); › is disabled only at the current period. Test: `model::tests::review_navigation_bounds`. Needs a phone re-check (Week and Month tabs, ‹ to October, "No data" period).

## App lock
Optional PIN/passphrase (+ fingerprint on Android, + "Hide content in app switcher") from Settings → App lock; locks on start and after Immediately / 1 / 5 / 15 min away. Shared implementation: [../../components/app-lock.md](../../components/app-lock.md). Hooks here: `<LockGate appName="Reflect">` in `src/App.svelte`, `<LockSettings>` in `src/components/SettingsPage.svelte` (after Sync), `LockHandle::new("reflect", …)` + `lock_*` commands in `src-tauri/src/lib.rs`, "---- Selfkit lock" blocks in `MainActivity.kt` + `SelfkitLock.kt`, `.skl` tokens in `src/app.css`, feature `lock-test` in `src-tauri/Cargo.toml`.
