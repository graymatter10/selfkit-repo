# Agenda (com.selfkit.agenda)

Status: **v0.3, usable on Linux and Android.** Form-based task adding, severity 1–5, start/stop, reminders (Android exact alarms, desktop timer), repeat, steps, lists, search, undo, settings, desktop quick-capture popup. Not yet: photos, CalDAV, in-app sync on Android (the desktop agent syncs `agenda.db`). Formerly "Tickd".

Design: clean iOS-style, minimal colour (see DECISIONS 2026-11-15 "Agenda v0.3"). Neutral greys and white (black in dark), one blue accent, red only for overdue and destructive actions. No coloured severity: High shows "!!", Critical "!!!".

## Screens
- **Phone**: large title "Tasks", search field, segmented control Active / All / Done, an inset grouped list (Overdue, Today, Tomorrow, Later, No date, Done), and a bottom "⊕ New Task" bar. The "…" menu has Sort by (Start date, Severity, Newest) and Settings.
- **New / Edit Task** (sheet): Cancel · New Task · Add. Title + notes, Severity segmented control, Starts / Ends rows opening a sheet with quick chips, a Monday-first calendar, an all-day switch and hour/minute wheels; Duration pills; Reminder, Repeat and List expand in place as checklists; Steps. Leaving with unsaved input (Cancel, Escape, Android Back) asks "Discard this new task?".
- **Details**: "‹ Tasks" · Edit, title with status line (red only when overdue), grouped rows, tickable steps, "Mark as Done" and a red "Delete Task" (both undoable from the toast).
- **Desktop (≥ 900 px)**: sidebar (Active, Overdue, All, Completed with counts; Lists; Settings), the list, and an inspector panel for details/forms.
- **Settings** (sheet): Appearance System/Light/Dark, default reminder, default severity, desktop quick-capture shortcut.
- **Quick-capture popup** (desktop): global shortcut (default Super+Shift+T) or `agenda --capture`. Title, severity, No date/Today/Tomorrow, time, reminder. Enter adds, Esc/blur closes.

Keyboard (desktop): N new · / search · J/K or ↑/↓ move · X done · E edit · Del delete · Esc close.

Android Back: closes the topmost layer (menu, settings, picker, form after asking, details, search).

## Data model (selfkit-core records)
- `tasks`: `title, notes, severity 1–5, start ("YYYY-MM-DDTHH:MM" or "YYYY-MM-DD" all day), stop, reminder_min (minutes before start; 0 = at start), repeat (daily|weekdays|weekly|monthly|yearly), list, steps[{id,title,done}], done_at (UTC ms), created_at`.
- v1 records are converted when read (`tasks::normalize`): due_date/due_time → start, priority → severity, subtasks → steps.
- Rules: stop needs start and must not be before it. Completing a repeating task moves start/stop to the next occurrence.
- `lists`: `name`. Built-in Inbox (id `inbox`) is shown as "Personal".
- Prefs (kv `agenda.prefs`): `theme`, default `reminder`, default `severity`.
- Undo = the frontend keeps the previous data and calls `replace_task`; delete undo = `restore_task`.

## Reminders
- Android: `tauri-plugin-notification` schedules exact alarms (`Schedule::At`, allow-while-idle; manifest has USE_EXACT_ALARM / SCHEDULE_EXACT_ALARM). Every change reschedules or cancels; startup reschedules all. Permission is asked the first time a task with a reminder is saved. All-day tasks remind relative to 09:00. Verified firing on the API 37 emulator.
- Desktop: a timer thread checks every 20 s and shows a notification.

## Build and run
```bash
cd apps/agenda && bun install
bun tauri dev                                            # Linux dev window
cargo build -p agenda --features tauri/custom-protocol   # debug binary with embedded UI
cargo test -p agenda                                     # model tests
SELFKIT_DATA_DIR=/tmp/agenda-demo cargo run -p agenda --example seed   # sample data
```
Android: see [../../platforms/android.md](../../platforms/android.md). Screenshots without a window: `dev/agenda-shot.sh` (AGENDA_DEMO params: `panel=new`, `title=`, `start=`, `picker=start`, `detail=<n>`, `edit=1`, `settings=1`, `menu=1`, `filter=`, `toast=1`).

## Code map
- `src-tauri/src/lib.rs`: Tauri setup, state, commands, `reminders` module (mobile alarms / desktop timer), `shortcut` module (global shortcut + capture window), env test hooks (`AGENDA_DEMO`, `AGENDA_WINDOW_SIZE`).
- `src-tauri/src/tasks.rs`: task model: normalize, validate, list/create/update/replace/toggle (repeat), search, lists, reminder time, tests.
- `src-tauri/examples/seed.rs`: sample data relative to now; `--theme X` only sets the theme pref.
- `src-tauri/gen/android/`: generated Android project (customized: icons, exact-alarm permissions, `adjustResize`).
- `src/main.ts`: mounts `App` or `Capture` (`?view=capture`).
- `src/app.css`: colour tokens (light/dark) and the few utility classes.
- `src/App.svelte`: phone/desktop layout, search, menu, Android Back handling, keyboard shortcuts.
- `src/Capture.svelte`: quick-capture popup (sets its own transparent background in code; a global style would leak into the main window).
- `src/lib/api.ts`: command wrappers + types. `src/lib/state.svelte.ts`: state, grouping, create/edit/toggle/delete with undo, prefs, `closeGuard`. `src/lib/time.ts`: date parsing/labels, status, groups, sort, reminder hint.
- `src/components/`: `TaskList`, `TaskRow`, `TaskForm`, `WhenSheet` (+ `Wheel`), `TaskDetail`, `Sidebar`, `SettingsSheet`, `Seg`, `Confirm`, `Toast`.
