# Agenda — .ics export and import

Settings → Calendar file (.ics): **Export Tasks…** (optionally including completed) and **Import .ics File…**. Code: `src-tauri/src/ics.rs`.

- Desktop export writes `agenda-YYYY-MM-DD.ics` to the Downloads folder (`AGENDA_EXPORT_DIR` overrides it, for tests). Android writes to the public Downloads folder through the `AgendaFiles` bridge in `MainActivity.kt` (MediaStore on Android 10+, no permission needed).
- Import reads a file picked with the system file chooser (GTK dialog / Android document picker) and reports "N new, N updated, N unchanged".
- Re-importing never duplicates: a VTODO's UID becomes the task id (unsafe UIDs are hashed to `ics-<hex>`), so the same task updates in place. Photos and history links on an existing task are kept; an existing completion time is kept.
- A task changed here after the file was written keeps its newer version ("kept (newer here)"), so importing an old export never undoes later edits or completions.

## Mapping (Agenda ↔ VTODO)
| Agenda | VTODO |
| --- | --- |
| id | UID |
| title / notes | SUMMARY / DESCRIPTION |
| start + stop | DTSTART + DUE |
| start only | DUE (what task apps show as "due"); repeating tasks use DTSTART (RRULE needs it) |
| all day | `VALUE=DATE`; times are floating local time. Import: UTC (`Z`) is converted to local, TZID is read as local |
| repeat | RRULE `FREQ=DAILY`, `WEEKLY;BYDAY=MO,TU,WE,TH,FR`, `WEEKLY`, `MONTHLY`, `YEARLY` (other rules: nearest FREQ) |
| severity 1–5 | PRIORITY 9/7/5/3/1 + `X-AGENDA-SEVERITY` |
| done | STATUS:COMPLETED + COMPLETED + PERCENT-COMPLETE:100 |
| stage doing | STATUS:IN-PROCESS |
| tags | CATEGORIES |
| reminder | VALARM DISPLAY with relative TRIGGER (all-day tasks remind at 09:00, so the trigger is relative to midnight + 9 h) |
| list | X-AGENDA-LIST (list name; created on import if missing) |
| steps | X-AGENDA-STEPS (JSON) |
| photos | not exported |

Lines are folded at 75 octets. Tests: `ics::tests` (lossless round trip, idempotent re-import, foreign files).
