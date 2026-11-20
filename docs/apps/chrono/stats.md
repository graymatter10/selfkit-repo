# Chrono statistics engine (`src-tauri/src/stats.rs`)

All aggregation runs in Rust; the UI only formats.

## Rules
- Sources: `app_sessions`, `web_sessions`, `android_events` (see README Data model). Excluded apps/domains are dropped when loading.
- Each session is split at local-day boundaries using its own `tz_offset` and the shared day start (`settings.day_start_min`).
- **Screen time** = length of the union of all sessions (apps and websites, all devices) — using the phone while the PC is on counts once. Per-device time = union per device.
- **Website time** lives inside the browser app's time: categories use website time on that device instead of the browser app; any browser time without website data counts as `other`.
- **Focus** = (Work + Study) ÷ (Work + Study + Social + Entertainment), in %. `other` is neutral.
- **Longest session** = longest single record piece in the day. First/last use = earliest start / latest end.
- Idle time is never in the data (the writers drop it).
- Peak times: average ms per weekday × clock hour over at least the last 4 weeks.
- Trends compare the per-day average with the previous period of the same kind; totals compare per day too, so a month in progress is not compared with a full month.

## Cache
Table `chrono_daily(day INTEGER, data JSON)` holds one `DayAgg` per local day (items per app/site/device/platform with ms, opens and `rc` — the share time rules moved to another category, per-device union, 24 hourly unions, unlocks, pickups, notifications per package, longest, first, last). It is local and never synced.
- Invalidation: kv `chrono.cache.lseq` remembers the store's change sequence; records changed since then (`lseq` index) mark their days dirty. Changing exclude lists, the day start or the enabled time rules (kv `chrono.cache.key`) clears the cache.
- Missing days are computed in runs of ≤ 31 days with one indexed query each (expression indexes on `$.start` / `$.hour`).
- Measured with the 240-day seed (~23k records), release build: lifetime trends 55 ms cold, 6 ms warm; year + today + report + timeline ≈ 7 ms. A 365-day run (debug build) took 341 ms cold, 47 ms warm.

## Views
`day_view`, `timeline`, `trends(period: day|week|month|year|lifetime)`, `report(week)`, `names(90 days)`, `sources(30 days)`. Long lists (sessions, totals) are virtualized in the UI (`VList.svelte`).
