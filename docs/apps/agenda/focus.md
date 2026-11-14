# Agenda — Focus (Pomodoro) timer

- Start from a task's details ("Start Focus · 25 min"). The length is set in Settings → Focus timer (15/25/45/50 min; break 5/10/15).
- While running: a slim bar (countdown, task, Stop) above the bottom bar (phone) or at the bottom of the main view (desktop). Tap it for the sheet with the ring countdown and "Stop Session".
- At the end: a system notification ("Focus done · <task>", or "Break is over"). The bar then offers "Break 5 min" (or "Focus 25 min" after a break).
- Android: the notification is an exact alarm scheduled at start (fires with the app closed); the session is logged when the app next checks (`focus_state`, every second while visible and on resume). Desktop: the Rust timer thread (every 5 s) logs the session and shows the notification.
- Stopping early logs the session as not completed; sessions under 1 minute are not logged.

Data: running session in local kv `agenda.focus`; finished sessions in collection `focus` (see README data model). Shown in History → Focus Sessions and Stats (focus minutes per day, focus this week). Code: `src-tauri/src/focus.rs`, `src/components/Focus.svelte`, `startFocus/stopFocus/pollFocus` in `src/lib/state.svelte.ts`.
