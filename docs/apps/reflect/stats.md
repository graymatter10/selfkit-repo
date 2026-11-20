# Reflect statistics rules (`src-tauri/src/model.rs`, `sources.rs`)

All numbers are computed in Rust; the UI only lays them out.

## Days
- One row per **logical day** (shared `day_start_min`; a session is split at day boundaries using its own `tz_offset`). Rows go from the oldest data of any app (at most 400 days back) to today.
- A source's value is "no data" before that app's first record; after it, a day without records counts as 0.
- **Screen time** = union of all app and website sessions on all devices (phone use while the PC is on counts once), same as Chrono. **Phone** = union of Android sessions. Excluded apps/domains from `chrono_settings` are dropped.
- **Late phone use** = phone minutes between 00:00 and 05:00 local on that calendar morning.
- **Spent** = Hisab expenses on that day in the main currency (`hisab-rates`; a time before the day start counts for the day before). **Daily budget** = sum of the monthly category budgets ÷ days in that month; it is compared with spending in categories that have a budget only (split parts per category), like Hisab (C61).
- **Tasks** = Agenda completions on that day (every completion of a repeating task counts).
- **Sleep** = Dusk's `dusk_nights` row for the night that ended that morning (`asleep_min`; all-nighter = 0, `none` = no data). Bedtime = local clock of `bed`.

## Today
- Screen time vs **a usual same weekday** (average of the last 8 same weekdays with data). Before 21:00 a lower number isn't called "less" yet ("A usual Sunday is 4 h 00").
- Headline: "A busy day on screens." (> 25 % and 30 min over usual), "A spendier day than planned." (over today's budget), "A day after a short night." (< 6 h), else "A calm day (so far)." when something good can be said, else "Here's your day so far."
- Ribbon: the logical day; sleep from last night's bed–wake, screens merged (gaps < 1 min joined), spending at the typed time, tasks at completion time.

## Streaks
- Kept per day: phone/all screens **under** the value; spending ≤ daily budget; asleep **by** the time (compared on an 18:00-based clock so 23:40 is before 00:00); tasks **at least** the value.
- Today: an "under" streak stays open (outlined) until the day is over; a tasks streak counts as soon as it's reached; sleep is final (last night is over).
- Current = kept days in a row back from today (an open today is skipped). A day with no data ends a run. Best = longest run ever.

## Connections
- Over the last `lookback_days` finished days. Pearson r between the two series; strength from |r|: ≥ 0.5 strong, ≥ 0.3 moderate, ≥ 0.15 weak, else no clear link (same as Dusk). Needs ≥ 3 days in each group, else "Not enough days yet".
- Groups: late phone use ≥ 30 min vs other nights; screen time under vs over your median (rounded to 30 min); sleep under 7 h vs 7 h+. Details split into 4 groups.

## Reviews
- Week = 7 days from the chosen week start; month = calendar month. Default: the last full one. Comparisons use the previous period's **per-day** average, so a period in progress is fair.
- Screen and sleep: average a day/night; money and tasks: totals. Budget: under/over the period's summed daily budgets, plus days kept.
- Worth knowing: heaviest screen day + its top app, biggest single spend (week) or top category share (month), nights with ≥ 15 min of late phone use, longest run of the first streak in the month (≥ 3 days).
