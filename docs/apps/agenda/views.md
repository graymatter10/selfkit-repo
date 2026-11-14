# Agenda — views, tags and Markdown

Switch views with the phone bottom bar icons, the desktop sidebar, or keys 1–6. Every view respects the tag filter.

- **Tasks** (Today list, the home screen): grouped Overdue / Today / Tomorrow / Later / No date / Done; Active / All / Done.
- **Upcoming**: Overdue, then the next 14 days day by day (Today and Tomorrow always shown, other empty days hidden). Repeating tasks appear on each day they repeat ("07:00 · repeats"). A line at the end counts tasks after the 14 days.
- **Calendar**: Monday-first month grid. Phone: dots per day (accent = open, grey = only done); swipe or arrows change month. Desktop: up to two task titles per day plus "+n more" / "n done". The selected day's tasks show below, with "+ Add" to create a task on that day.
- **Board**: To Do / Doing / Done. Desktop: three columns; drag a card with the mouse (pointer events, works on WebKitGTK). Phone: one column at a time (segmented control with counts) and "‹ To Do / Doing ›" buttons on each card. Moving into Done completes the task (a repeating task moves to its next date and back to To Do); moving out of Done reopens it. Every move has Undo. Done shows the 30 most recent.
- **History** (own screen): Completed (every completion, grouped by day, with time, on time / late and tags) and Focus Sessions. Tap an entry to open its task.
- **Stats** (own screen, never on the home screen): tiles (done this week, on-time rate for 30 days and all time, focus this week) and bar charts: completed per day (14 days), per week (12 weeks), focus minutes per day. Hover/tap a bar to read its value. On time = completed before its due time (stop, else timed start, else end of the all-day date).

## Tags
Lowercase words without '#', cleaned in Rust (`tasks::clean_tags`). Form: type and press space, comma or Enter; tap a suggestion; Backspace in an empty field removes the last tag. Filter: phone "…" → Tag; desktop sidebar Tags; or tap a tag chip in details. New tasks get the active filter's tag.

## Markdown notes
`src/lib/markdown.ts` (no dependencies): headings, paragraphs, line breaks, bold, italic, strike, inline and fenced code, links and bare URLs (http/https/mailto only; they open in the system browser via `tauri-plugin-opener`), bullet/numbered lists, checklists (display only), quotes, rules, `![](photo:<id>)`. Safety: text is HTML-escaped first, only fixed tags are generated, then an allowlist sanitizer re-checks tags and attributes. Remote images are never loaded (offline, private).
