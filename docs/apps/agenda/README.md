# Agenda (com.selfkit.agenda)

Status: **v0.4.1, usable on Linux and Android.** (0.4.1 = QA fixes, see [QA 2026-11-15](#qa-2026-11-15).) Everything in v0.3 (form-based adding, severity 1–5, start/stop, reminders, repeat, steps, lists, search, undo, settings, desktop quick capture) plus tags, Markdown notes, photos, Upcoming / Calendar / Board views, completed-task history, stats, a Pomodoro ("Focus") timer and `.ics` export/import. Formerly "Tickd".

**CalDAV dropped for now (user request, 2026-11-15).** `.ics` export/import covers moving tasks to and from other apps. The subscribable feed is not built (optional; see Decisions in the v0.4 report). Not yet: in-app sync on Android (the desktop agent syncs `agenda.db`), syncing photo files (S3 blob sync).

Design: clean iOS-style, minimal colour (see DECISIONS 2026-11-15 "Agenda v0.3"; v0.4 screens approved by the user 2026-11-15). Neutral greys and white (black in dark), one blue accent, red only for overdue and destructive actions. No coloured severity: High shows "!!", Critical "!!!".

Feature docs: [ics.md](ics.md) (export/import mapping) · [focus.md](focus.md) (Pomodoro) · [photos.md](photos.md) · [views.md](views.md) (Upcoming, Calendar, Board, History, Stats, tags, Markdown).

## Screens
- **Phone**: large title, search field and Active / All / Done (list view only), an inset grouped list (Overdue, Today, Tomorrow, Later, No date, Done). Bottom bar: "⊕ New Task" on the left, view switcher icons on the right (Tasks, Upcoming, Calendar, Board). The "…" menu has Sort by, Tag filter, History, Stats, Settings. A tag filter shows as a "#tag ×" chip under the title. A running focus session shows as a slim bar above the bottom bar.
- **Upcoming / Calendar / Board / History / Stats**: see [views.md](views.md).
- **New / Edit Task** (sheet): Cancel · New Task · Add. Title + notes (Markdown), Photos (Add Photo → Android sheet "Take photo / Choose from gallery", camera photos stay private; desktop "Add Photo…"; viewer has Save to gallery — [photos.md](photos.md)), Tags (chips, suggestions), Severity, Starts / Ends (picker sheet; an empty Starts opens on the next full hour, an empty Ends on start + 1 h, and the calendar day follows that time, so at 23:50 it is tomorrow 00:00 — L1), Duration pills, Reminder, Repeat, List, Steps. Leaving with unsaved input (Cancel, Escape — also while typing in a field (L3) — Android Back) asks "Discard this new task?".
- **Details**: "‹ Tasks" · Edit, title + status line, notes rendered as Markdown with photos in the same card (tap → full-screen photo), tag chips (tap → filter), grouped rows incl. Board column, tickable steps, "Start Focus · 25 min", "Mark as Done", red "Delete Task".
- **Desktop (≥ 900 px)**: sidebar (Active, Overdue, All, Completed; Views: Upcoming, Calendar, Board; Lists; Tags; History, Stats, Settings), main view, inspector panel for details/forms.
- **Settings** (sheet): Appearance, default reminder, default severity, Focus timer lengths (15/25/45/50, break 5/10/15), Calendar file (.ics) export/import, desktop quick-capture shortcut.
- **Quick-capture popup** (desktop): global shortcut (default Super+Shift+T) or `agenda --capture`.

Keyboard (desktop): N new · / search · 1–6 views (Tasks, Upcoming, Calendar, Board, History, Stats) · J/K or ↑/↓ move · X done · E edit · Del delete · Esc close (also the "…" menu, L9).
Search: every word must start a word of the title, notes, a tag or a step title (the full-text index also holds JSON keys and ids, so "title" or "inbox" used to match every task — C26).

Android Back: closes the topmost layer (menu, photo, focus sheet, settings, picker, form after asking, details, search, History/Stats → Tasks); with nothing open the app goes to the background. It uses Tauri's `back-button` event (`onBackButtonPress`) instead of history entries: the WebView skips history entries re-added without a tap, so two quick Backs used to leave the app from the details screen (bug found in v0.4 testing). `AgendaBars.toBackground()` in MainActivity.kt leaves the app.

## Data model (selfkit-core records)
- `tasks`: `title, notes (Markdown), severity 1–5, start ("YYYY-MM-DDTHH:MM" local or "YYYY-MM-DD" all day), stop, reminder_min (minutes before start; 0 = at start), repeat (daily|weekdays|weekly|monthly|yearly), repeat_day (day of month a monthly/yearly repeat returns to after a short month clamped the start: Jan 31 → Feb 28 → Mar 31; only while the start still matches it — C21), list, steps[{id,title,done}], tags [lowercase, no '#'], stage ("doing" = Board column Doing; absent = To Do; done = done_at set), photos [photo ids], done_at (UTC ms), created_at (UTC ms), last_done_at, last_completion (id of its latest `completions` record)`.
- `completions` (history + stats): `task, title, done_at (UTC ms), tz_offset_min, due ("YYYY-MM-DDTHH:MM" local, absent if undated), on_time (bool, absent if undated), tags`. One per completion, also for repeating tasks; id `c-<task id>-<start>` (one per occurrence), so the same occurrence ticked on two devices before syncing stays one entry (C30). Un-completing deletes it; undo restores it.
- `focus` (Pomodoro log): `task?, title, kind (focus|break), start, end (UTC ms), minutes, planned_min, completed (ran to the end), tz_offset_min`.
- `photos`: `file, mime, w, h, bytes, sha256, created_at`; the image itself is `<data_dir>/agenda-photos/<file>`.
- `lists`: `name`. Built-in Inbox (id `inbox`) is shown as "Personal".
- Local kv (not synced): `agenda.prefs` (`theme, reminder, severity, focus_min, break_min`), `agenda.focus` (running session).
- v1 records are converted when read (`tasks::normalize`): due_date/due_time → start, priority → severity, subtasks → steps, status "doing" → stage doing.
- Rules: stop needs start and must not be before it. Completing a repeating task moves start/stop to the next occurrence and leaves the Doing column.
- Undo = the frontend keeps the previous data and calls `replace_task`; delete undo = `restore_task`.

## Reminders and notifications
- Android: `tauri-plugin-notification` schedules exact alarms (`Schedule::At`, allow-while-idle). A reminder time in the spring-forward DST gap fires an hour later (it was dropped — C24). Small icon: monochrome checkbox `res/drawable/ic_stat_agenda.xml` (the plugin's default was Android's generic "i" — P3). Every change reschedules or cancels; startup reschedules all. Permission is asked the first time a reminder or focus session is started. All-day tasks remind relative to 09:00. Focus sessions schedule one alarm at their end (fixed id).
- Desktop: a timer thread checks every 5 s, shows due reminders and finishes focus sessions (log + notification).

## Build and run
```bash
cd apps/agenda && bun install
bun tauri dev                                              # Linux dev window
cargo build -j 6 -p agenda --features tauri/custom-protocol   # debug binary with embedded UI
cargo test -j 6 -p agenda                                  # model, ics, focus tests
SELFKIT_DATA_DIR=/tmp/agenda-demo cargo run -p agenda --example seed -- --photo a.jpg   # sample data (4 months)
SELFKIT_DATA_DIR=/tmp/agenda-year cargo run -p agenda --example seed -- --year --photo a.webp   # a year: ~860 done, 150 open, 9 habits (~975 completions), ~700 focus sessions
```
Android: see [../../platforms/android.md](../../platforms/android.md). Release: `bun tauri android build --apk --target aarch64`, then zipalign + apksigner (debug keystore).

Test hooks (desktop, env): `AGENDA_WINDOW_SIZE=WxH`, `AGENDA_EXPORT_DIR` (where Export writes; default Downloads), `AGENDA_DEMO` params: `view=list|upcoming|calendar|board|history|stats`, `tag=`, `filter=`, `panel=new`, `title=`, `start=`, `picker=start`, `detail=<n>`, `edit=1`, `settings=1`, `menu=1`, `toast=1`, `focus=<n>` (start focus on task n), `focusmin=<m>`, `focusopen=1`, `photo=1`, `scroll=<px>`.

## Code map
- `src-tauri/src/lib.rs`: Tauri setup, state, commands (tasks, history, focus, photos, ics, export), `reminders` module (mobile alarms / desktop timer incl. focus end), `shortcut` module, env test hooks.
- `src-tauri/src/tasks.rs`: task model: normalize, validate (tags/stage/photos), list/create/update/replace/toggle (repeat, completions), history, search, lists, reminder time, tests.
- `src-tauri/src/ics.rs`: VTODO export/import (parser, folding, mapping), tests. `src-tauri/src/focus.rs`: focus sessions. `src-tauri/src/photos.rs`: photo files + metadata.
- `src-tauri/examples/seed.rs`: sample data relative to now (open tasks, tags, Markdown, photos, 120 days of history and focus); `--year` = a year of everyday use (long/emoji/RTL titles included).
- `src-tauri/src/recover.rs`: open a DB even when its file is damaged (set aside as `<name>.damaged-<ms>`, fresh DB, one-time toast via `startup_notice`). Same file in hisab and dusk.
- `src-tauri/gen/android/`: generated Android project (customized: icons, exact-alarm permissions, `adjustResize`, camera `<queries>`, `MainActivity.kt` bridges `AgendaBars` (status bar) and `AgendaFiles` (save .ics to Downloads); `SelfkitPhotos.kt` = `window.SelfkitPhotos` (camera capture cleanup, Save to gallery — [../../components/photo-source.md](../../components/photo-source.md))).
- `src/App.svelte`: phone/desktop layout, view switcher, search, menu, Android Back handling, keyboard shortcuts, test hooks.
- `src/lib/api.ts`: command wrappers + types. `src/lib/state.svelte.ts`: state, grouping, tags, create/edit/toggle/delete/board moves with undo, focus, prefs, `closeGuard`. `src/lib/time.ts`: dates, status, groups, sort, repeat occurrences. `src/lib/markdown.ts`: safe Markdown renderer + sanitizer. `src/lib/photos.ts`: scale/encode/save/load photos. `src/lib/photo-source.ts`: re-exports the shared camera/gallery sheet and Save to gallery from `ui/src`.
- `src/components/`: `TaskList`, `TaskRow`, `TaskForm`, `WhenSheet` (+ `Wheel`), `TaskDetail`, `Notes`, `Photo`, `PhotoViewer`, `UpcomingView`, `CalendarView`, `BoardView`, `HistoryView`, `StatsView` (+ `Bars`), `Focus` (bar + sheet), `Sidebar`, `SettingsSheet`, `Seg`, `Confirm`, `Toast`.

## Sync (wave 2, 2026-11-15)
Shared module [selfkit-appsync](../../components/selfkit-appsync.md) + Android plugin [selfkit-sync](../../../android-plugins/selfkit-sync/README.md); same UI in every app (`ui/src/sync/`).
- **Settings → Sync**: status ("Synced locally with Linux PC · 2 min ago" / "Synced via server · …" / "Waiting to sync · N changes saved here"), **Sync now**, local sync toggle + seen devices, server setup (Postgres + optional S3 for photos, Test/Save), encryption setup with recovery key / unlock / change passphrase, who syncs (Android: This app | Selfkit companion; desktop: Background agent | This app), battery tip on OPPO/vivo-style phones, sync log.
- **Reminders after sync** (Android): on start and after every sync that brought changes (`data-changed`), `reminders::reschedule_all` cancels alarms of tasks that are no longer active (completed/deleted/moved on another device) and schedules new or changed ones. A task with a reminder fires on every phone that has Agenda.
- **Conflict screen** (Settings → Sync → Review): Keep A (this device) / Keep B / Merge field by field, long text as a line list to tick; edited-vs-deleted explained.
- **When**: on open, every ~15 min (Android WorkManager, needs a network; desktop: the agent), and Sync now. Transport per run: LAN to a reachable Selfkit device, else the server.
- **Stores synced**: `agenda.db` and the shared `settings.db`. Photos (`photos` + `agenda-photos/`) go through S3 when configured.
- **Test builds**: Cargo feature `sync-test-key` (default on) uses the public built-in key, so there are no passphrase prompts; build with `--no-default-features` for real data.
- **Code**: `src-tauri/src/lib.rs` (setup: `selfkit_appsync::tauri_cmds::default_handle("agenda", …)` + `sync_*` commands), Settings row + page wiring in the frontend, `.sks, .skc` token mapping in `src/app.css`, Android: `MainActivity.kt` "Selfkit sync" blocks, `settings.gradle` / `app/build.gradle.kts` module lines, manifest meta-data `com.selfkit.sync.APP/LIB`. Provider: `content://com.selfkit.agenda.provider` (signature permission `com.selfkit.permission.SHARE`).

## QA 2026-11-15
Tested with a year of data (`seed --year`) on Linux (Xvfb, debug build, software rendering) and on the vivo (non-destructive on the OPPO).
- Year data: open ≈ 0.5–1.1 s; screen switches 90–300 ms (Completed list was 380 ms → now renders the newest 100 done tasks with "Show More"); CPU per switch 30–130 ms in WebKit. Memory: ~250 MB PSS total (app + WebKit processes), 440–475 MB summed RSS (empty data: 383 MB RSS, the WebKit baseline).
- Fixed: damaged/unreadable `agenda.db` or `settings.db` made the app close at start forever (now set aside + fresh DB + toast); two desktop instances ran at once (double reminders, shortcut lost) — now `tauri-plugin-single-instance` (a second launch focuses the window, `--capture` opens the popup in the running one; skipped when `SELFKIT_DATA_DIR` is set, for tests); held Enter / double click in the quick-capture popup added the task several times; double tap on a repeating task's circle (or Board move) could complete it twice; a save reloaded the task list twice (reloads are now coalesced); an emptied time in the capture popup sent "NaN" as start; desktop empty-state said "Tap “New Task”".
- Verified: `kill -9` during writes (6 runs) leaves `PRAGMA integrity_check` ok; reminder fires after the process is killed and after force-stop + relaunch (rescheduled once, no duplicate); deleting the task removes a shown notification; Back with the keyboard open only hides it; double tap on Add creates one task.

## Phone QA fixes 2026-11-15 (photos)
- Photo viewer: the status bar switches to light icons while the black viewer is open (`AgendaBars.setDark(true)`) and back to the theme's colours on close (`applyTheme()`); the clock was invisible before.
- New/Edit Task form: tapping a photo thumbnail opens the viewer (Back closes the viewer first, then the form); the thumbnail grid has the same 6 px inset on every side.
- Files: `src/components/PhotoViewer.svelte`, `TaskForm.svelte`, `Photo.svelte` (block image). Needs a phone re-check (status bar icons in light theme, thumbnail inset).

## App lock
Optional PIN/passphrase (+ fingerprint on Android, + "Hide content in app switcher") from Settings → App lock; locks on start and after Immediately / 1 / 5 / 15 min away. Shared implementation: [../../components/app-lock.md](../../components/app-lock.md). Hooks here: `<LockGate appName="Agenda">` in `src/App.svelte`, `<LockSettings>` in `src/components/SettingsSheet.svelte` (under Sync), `LockHandle::new("agenda", …)` + `lock_*` commands in `src-tauri/src/lib.rs`, "---- Selfkit lock" blocks in `MainActivity.kt` + `SelfkitLock.kt`, `.skl` tokens in `src/app.css`, feature `lock-test` in `src-tauri/Cargo.toml`. The quick-capture popup is not gated (it only adds tasks).
