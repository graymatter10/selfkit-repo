# Dusk — how a night is estimated

Code: `apps/dusk/src-tauri/src/model.rs` (pure, unit-tested). Debug any data folder with `cargo run -p dusk --example dusk_report`.

## Inputs
- **Activity spans** `{start, end, tz}` (UTC ms + offset) from every device: Chrono `app_sessions` + `web_sessions` (desktop, incl. phone sessions synced from Chrono) and Dusk's own `device_use` (phone screen-on intervals). They are unioned; spans closer than 2 min join (`merge_spans`).
- **"Now" counts as activity** while the app is open (`now_active`): opening Dusk in the morning ends the night even before the phone/agent has written its latest session.
- The manual log (`sleep_log`) and, later, outside sleep sessions (`ExternalSleep`, Health Connect).

## One night
1. **Window**: `[day before 18:00, night day 18:00)` local, with the night's offset (most common offset of the activity in it, else the device's current offset; a corrected night uses the log's offset). Consecutive windows tile the timeline.
2. **Quiet stretches** = gaps between activity of ≥ 20 min.
3. **Chains**: quiet stretches of ≥ 60 min separated by ≤ 20 min of activity are one sleep with short wake-ups (checking the time). Shorter pauses (a normal morning's 20–45 min gaps) never extend a sleep.
4. **Main sleep** = the chain with the most quiet time, weighted by hour (21:00–11:00 counts fully, daytime 0.4), with ≥ 2.5 h of quiet time. Bedtime = its start, wake = its end, "woke briefly" = number of wake-ups and minutes awake; asleep = span − awake.
5. Otherwise: **all-nighter** if the window reached 06:00 and there was screen use between 01:00 and 05:00; **not over yet** if the window is still open; else **no data**.
6. **Late use** (for Connections) = screen minutes from local midnight until wake (or 06:00).

## Priority and corrections
Manual times > outside source (Health Connect) > device estimate. A rating or note alone keeps the estimate. Correcting one side (bed or wake) keeps the other from the estimate. The estimate stays visible next to the correction ("was 23:48", thin line in the timeline).

## Naps
Daytime sleep can't be seen in device data (not using a phone isn't sleeping), so naps are manual only (`kind: "nap"`, ≤ 6 h). They show stacked on the night's bar and count toward averages and sleep debt.

## Day start, "last night", time zones
- "Last night" = the night whose key is the current **logical day** (shared `day_start_min`). With day start 04:00, at 02:00 the previous night is still "last night"; with 00:00 the current night shows "Not over yet".
- Next-day tasks/spending use the logical day of the wake time.
- Durations are real elapsed time (UTC); clock times are shown in the offset they happened in, so a night abroad keeps its local times. A flight during the night may shift the window edges; durations stay right.

## Stats
- **Average**: total sleep (night + naps) over nights with data; trend vs the period before.
- **Consistency** 0–100 = 100 − (SD of bedtime + SD of wake time)/2 ÷ 0.9 (≥ 3 nights). ±0 min = 100, ±45 min = 50, ±90 min = 0.
- **Sleep debt** (14 nights) = Σ(goal − slept), surplus offsets shortfall, never below 0.
- **Bed on time** = nights in bed by the bedtime goal + 15 min.

## Connections (last 3 months)
- Screen time after midnight vs quality (rated nights): groups "30+ min after midnight" vs "other nights"; details: none / < 30 / 30–60 / > 60 min.
- Sleep vs tasks done (Agenda) and money spent (Hisab) on the next logical day: "< 7 h" vs "7 h +"; details: < 6, 6–7, 7–8, 8 h +.
- Strength from Pearson |r|: ≥ 0.5 strong, ≥ 0.3 moderate, ≥ 0.15 weak, else no clear link (weak findings say "may"). Needs ≥ 3 nights per group and ≥ 5 pairs.

## Health Connect
Planned "later" (PLANNING). Extension point: `health.rs::sessions()` returns `ExternalSleep` sessions and `model::night_view` already ranks them between the log and the device estimate (pill: "From health connect"). To add: read `SleepSessionRecord` in `DuskUsage.kt` (permission `android.permission.health.READ_SLEEP`), pass them to a new command, store them in a `health_sleep` collection (so they sync to desktop) and return them from `sessions()`.
