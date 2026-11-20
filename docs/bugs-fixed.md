# Bug fixes (branch claude/batch-fixes-1655a1, 2026-11-20)

Fixes for bugs logged in `docs/bugs-linux.md` (L*), `docs/bugs-phone.md` (P*, lead B*) and `docs/bugs-code.md` (C*). Not built per fix; CI builds `test/fixes-*` copies of this branch.

| Bug | Fix | Files | Needs device retest |
|-----|-----|-------|---------------------|
| L1 / P1 | Starts/Ends picker opens on the day of its default time (next full hour / start + 1 h), not always today | `apps/agenda/src/components/WhenSheet.svelte` | Linux (set clock or wait until 23:xx) |
| B3 | PIN set-up sheet keeps message, Next and "Use a passphrase instead" rows on every step (hidden, not removed), so the pad stays put at "Repeat the PIN" | `ui/src/lock/LockSettings.svelte` | OPPO (all 5 apps share it) |
| B5 | Lock screen no longer keeps a fixed "Wrong PIN. Try again in 30 s." after the wait ends; the live countdown line shows the wait, then "Wrong PIN." | `ui/src/lock/LockGate.svelte` | OPPO (5 wrong PINs, wait, fingerprint cancel) |
| B2 | Lead chose option 2: Dusk's provider serves the local `dusk_nights` table (`/table?name=`), `SelfkitMirror` copies it into Reflect's `dusk.db`. Phone-local, no sync change (DECISIONS 2026-11-20) | `selfkit-appsync/src/android.rs`, `android-plugins/selfkit-sync/.../SelfkitDataProvider.kt`, `SelfkitMirror.kt` | OPPO: Dusk and Reflect APKs both new; open Dusk once, then Reflect → Today sleep tile |
| B4 | Lead chose option 1: Dusk mirrors `agenda,hisab` on Android (manifest + page call on open/foreground); Settings row shows Found / Agenda only / Hisab only / Not found | `apps/dusk/src-tauri/gen/android/app/src/main/AndroidManifest.xml`, `apps/dusk/src/lib/state.svelte.ts`, `SettingsPage.svelte` | OPPO: Dusk Settings + Connections next-day cards |
| B5 / L7 | (updated) after the wait the lock screen says "Enter your PIN" again | `ui/src/lock/LockGate.svelte` | OPPO + Linux |
| P1 | same as L1 | — | — |
| L2 | no text caret in buttons (task rows) | `apps/agenda/src/app.css` | Linux |
| L3 | Esc in a task form field closes / asks to discard | `apps/agenda/src/App.svelte` | Linux |
| L4 | server form checked before secure storage; keyring hint on Linux | `selfkit-appsync/src/tauri_cmds.rs`, `explain.rs`, `ui/src/sync/SyncSettings.svelte` | Linux without keyring |
| L5 | not a code bug: release installers must be built by the release workflows (`SELFKIT_RELEASE=1`, which disables `lock-test`/test key) | — | lead |
| L6 | lock screen shows the full lock file path | `selfkit-appsync/src/lock_cmds.rs`, `selfkit-core/src/applock.rs`, `ui/src/lock/*` | Linux |
| L8 | search Cancel fits in the desktop header | `apps/agenda/src/App.svelte` | Linux |
| L9 | Esc closes the "…" menu; view keys wait while it is open | `apps/agenda/src/App.svelte` | Linux |
| L10 | .ics re-import of an unchanged export = unchanged (empty fields ignored, numbers by value) | `apps/agenda/src-tauri/src/ics.rs` | Linux |
| L11 / P7 | real version in the Settings footer; PRODID without version | `SettingsSheet.svelte`, `ics.rs` | both |
| L2-1 / C31 | Enter in the note saves on desktop + capture popup | `apps/hisab/src/components/AddPanel.svelte` | Linux |
| L2-2 / C32 | number pad stays on desktop while the note has focus | same | Linux |
| P2 | not fixed: alarm was windowed, i.e. `canScheduleExactAlarms()` was false for the plugin despite `USE_EXACT_ALARM`. Needs a device check (`adb shell cmd appops get com.selfkit.agenda SCHEDULE_EXACT_ALARM`, `dumpsys alarm`); ColorOS may also batch alarms of background apps | — | OPPO |
| P3 | reminder notifications use monochrome app icons (Agenda new `ic_stat_agenda`, Hisab existing `ic_stat_hisab`, Dusk new `ic_stat_dusk`) | `apps/*/src-tauri/src/lib.rs`, `res/drawable/ic_stat_*.xml` | OPPO |
| P4 | done tasks without a completion record are listed from the task (`task-<id>`) in History/Stats | `apps/agenda/src-tauri/src/tasks.rs` | OPPO |
| P5 | unsaved New Task input kept in localStorage and reopened after a kill | `TaskForm.svelte`, `App.svelte`, `state.svelte.ts` | OPPO |
| P6 | long words wrap in the details title and steps | `TaskDetail.svelte` | OPPO |
| P8 | not fixed: needs profiling (no concrete cause) | — | — |
| C1–C3 | clockless records stamp only changed fields; one clockless side = conflict; imports stamp restored fields | `selfkit-core/src/store.rs` | sync tests |
| C4 | not fixed: reminders after a WorkManager sync with Agenda closed need an Agenda-specific hook (Kotlin → Agenda JNI to reschedule, or SyncWorker hook); bigger than a bug fix — lead decision | — | — |
| C5 | lock stop time kept per process (companion object) | `apps/*/…/SelfkitLock.kt` | OPPO ("Don't keep activities") |
| C6 | Forgot PIN wrong passphrase counts as a wrong attempt with waits | `lock_cmds.rs`, `applock.rs` | both |
| C7 | newest reply wins (Overview, Insights, History) | `apps/hisab/src/lib/state.svelte.ts`, `Insights.svelte`, `History.svelte` | Linux |
| C8 | read-only stores on DBs without `fclock` (temp view) | `selfkit-core/src/store.rs` | Windows mixed versions |
| C9 | merged record pushed again in the same pass | `selfkit-core/src/sync/mod.rs` | sync |
| C10 | pending draft write flushed before Save | `apps/hisab/src/lib/state.svelte.ts` | — |
| C11 | not fixed (documented limit): clock set forward ends a lock wait; monotonic clocks stop in deep sleep / restart with the process | `docs/components/app-lock.md` | — |
| C12 | `lock_unlock_biometric` refused outside Android | `lock_cmds.rs` | — |
| C13 | all lock commands `#[tauri::command(async)]` | `lock_cmds.rs` | Linux |
| C14 | agent init needs 12 characters | `selfkit-agent/src/main.rs` | — |
| C15 | settings drop legacy `_clock`; conflict screen hides it | `selfkit-core/src/settings.rs`, `selfkit-appsync/src/conflicts.rs` | — |
| C16 | marker file + watcher → `data-changed` after shared settings were adopted | `selfkit-appsync/src/{android,tauri_cmds,lib}.rs` | OPPO (two apps open) |
| C17 | MutationObserver keeps new body children inert while locked | `ui/src/lock/LockGate.svelte` | — |
| C18 | recurring entries id `rec-<item>-<date>` | `apps/hisab/src-tauri/src/recur.rs` | two devices |
| C19 | seeded Cash/categories fixed ids | `apps/hisab/src-tauri/src/model.rs` | two fresh devices |
| C20 | merge result checked against cross-field rules, else conflict | `selfkit-core/src/store.rs` | sync |
| C21 | `repeat_day` anchor for monthly/yearly repeats (Rust + calendar preview) | `tasks.rs`, `time.ts`, `api.ts` | — |
| C22 / C25 | CSV dedupe only against existing entries (+kind, account); DR/CR as own word | `apps/hisab/src-tauri/src/csvimport.rs` | — |
| C24 | DST gap reminders fire an hour later | `apps/agenda/src-tauri/src/lib.rs`, `selfkit-agent/src/reminders.rs` | — |
| C26 | search filtered to task text | `tasks.rs` | — |
| C27 | `device_id` exclusive create | `selfkit-core/src/paths.rs` | — |
| C30 | completion id per occurrence `c-<task>-<start>` | `tasks.rs` | two devices |
| L2-3 / C33 | "#tags" in the editor's note become tags (`splitNoteTags`) | `apps/hisab/src/lib/fmt.ts`, `AddPanel.svelte`, `TxSheet.svelte` | Linux |
| L2-4 / C34 | selects right-aligned on WebKitGTK, date/time wrapper pushed right, Note/Tags left | `TxSheet.svelte`, `apps/hisab/src/app.css` | Linux |
| L2-5 / C40 | partly: a failing Undo now shows "Couldn't undo: …" (all apps). Whether the click arrives on WebKitGTK still needs a real-mouse check | `ui/src/toast.svelte.ts` | Linux (real mouse) |
| L2-6 / C41 | Ctrl+, opens Settings from Home too | `apps/hisab/src/App.svelte` | Linux |
| L2-8 / C49 | rates shown in the direction ≥ 1, typed values converted back | `apps/hisab/src/components/Settings.svelte` | Linux + OPPO |
| L4-1 / C35 | wider, smaller-font desktop time fields in 12h | `apps/dusk/src/components/Adjust.svelte` | Linux 12h |
| L4-2 / C36 | `tdisp` for Bed on time, day start list, Trends legend | `StatsGrid.svelte`, `SettingsPage.svelte`, `TrendsPage.svelte` | Linux 12h |
| L4-3 / C37 | singular counts (Dusk, Hisab, Chrono) | `StatsGrid.svelte`, `Timeline.svelte`, Hisab `Insights`/`Categories`, Chrono `Today` | — |
| L4-4 / C38 | "–" for sleep debt / bed on time without nights | `StatsGrid.svelte` | Linux |
| L4-5 / C39 | minute arrow keys carry the hour | `TimeDial.svelte` | Linux |
| L4-6 / C42 | overlapping naps refused | `apps/dusk/src-tauri/src/lib.rs` (`nap_add`) | Linux |
| L4-7 / C44 | Esc closes Adjust/Nap wherever focus is | `apps/dusk/src/App.svelte` | Linux |
| L4-8 / C43 | future bed/wake/nap times refused | `lib.rs` (`log_set`, `nap_add`) | Linux |
| L4-9 / C45 | "Needs 3 nights" until the score exists | `StatsGrid.svelte` | Linux |
| L12 | `\p{M}` in the #tag pattern (Bangla) | `apps/hisab/src/lib/fmt.ts` | OPPO |
| L13 | strict quartile buckets, biggest day darkest | `apps/hisab/src/charts/Heatmap.svelte` | Linux |
| L14 / P10 | net worth chart: 0 line labelled, negative min labelled | `apps/hisab/src/charts/Trend.svelte` | both |
| L16 | CSV export UTF-8 BOM; export sheet wording | `export.rs`, `ExportSheet.svelte` | Windows Excel |
| P9 | Insights amounts nowrap + smaller | `Insights.svelte` | OPPO |
| P12 | 2 decimals after 10 whole digits | `AddPanel.svelte` | OPPO |
| L3-3 / C54 | extension shipped with the agent (.deb /usr/share/selfkit-agent/extension, tarball extension/, Windows $INSTDIR\extension); How to add names the real paths; package test checks it | `installer/linux/{build-agent-packages,test-packages}.sh`, `installer/windows/agent.nsi`, `apps/chrono/src/screens/Sources.svelte` | linux.yml + windows.yml |
| L3-1 / C52 | extension closes the session on onFocusChanged(WINDOW_ID_NONE) | `extension/background.js` | Linux Chromium |
| L3-2 / C53 | native host reply carries `idle_minutes`; extension applies it with setIdleInterval | `selfkit-agent/src/{chrono_settings,native_host}.rs`, `extension/background.js` | Linux |
| P17–P20 | Chrono: average window named, short change text (+minutes for big jumps), last use 23:59 past midnight, "N sources this week" | `screens/Today.svelte`, `screens/Trends.svelte`, `App.svelte` | OPPO |
| P32 | Reflect tile label wraps instead of "Screen t…" | `apps/reflect/src/components/Tile.svelte` | OPPO |
| P33 | Tasks tile counts open tasks from earlier days ("3 overdue") | `apps/reflect/src-tauri/src/model.rs` | OPPO |
| P34 | in-progress review averages leave today out | `model.rs` (`review`) | OPPO |
| P27 | partly: nap row in Notes wraps instead of "3…". The missing nap part on the highlighted bar wasn't reproducible from code (NightBars draws `nap_min` on every bar); needs the night's data | `NightPage.svelte` | OPPO |
| P29 | not a bug in code: "of N nights" counts the last 7 nights that have a bedtime (5 of 7 had one); consistency mixes bed and wake spread by design (estimation.md). Wording left as is | — | — |
| P31 | Adjust: "Estimate …" and labels wrap instead of running under −15 | `Adjust.svelte` | OPPO 12h |
| L4-16..L4-19, P28, P30 | Dusk: select on click, averages = nights with main sleep, wider goal, shortcut validated + registration error shown, "< 1 min" nowrap, Settings scroll to top after render | `Adjust.svelte`, `NightPage.svelte`, `SettingsPage.svelte`, `App.svelte`, `lib/state.svelte.ts`, `src-tauri/src/lib.rs` | Linux + OPPO |
| C51 | documented as a known Linux limitation (lead decision) | `docs/platforms/linux.md` | — |
| L12 (all apps) | checked: Hisab's `splitNoteTags` is the only #tag parser in the apps; Agenda/Hisab tag cleaning in Rust keeps any characters | — | — |
| L25 | Chrono: recover.rs (copy of Dusk's) at startup for chrono.db + settings.db, `startup_notice` toast | `apps/chrono/src-tauri/src/{recover,lib}.rs`, `src/lib/{api,state.svelte}.ts` | Linux (damage the DB) |
| P16, P21, P22 | Chrono: See all → Trends Day of that day; Today tab → today; goals Add/Edit → Settings goals section (+ new form when none) | `apps/chrono/src/App.svelte`, `screens/Trends.svelte`, `screens/Settings.svelte`, `lib/state.svelte.ts` | OPPO |
| L3-5 / C56 | Chrono rail says "Not recording" + hint when no device reported in 7 days | `apps/chrono/src/App.svelte` | Linux fresh |
| P24 | partly: Facebook Lite, Telegram web/X packages default to Social. Telegram as Entertainment not reproducible from the defaults ("telegram" and org.telegram.messenger are Social) — likely a stored category override (synced settings); needs the phone's chrono_settings.categories | `apps/chrono/src-tauri/src/categories.rs` | OPPO |
| P11 / P23 | Hisab: loan save toast + Settings say Android blocks notifications; Chrono: Alerts/Weekly report lines say so | `LoanSheet.svelte`, Hisab + Chrono `Settings.svelte` | OPPO (Don't allow) |
| L4-14 / C62 | time sheet focuses the dial whenever there is a keyboard (not the Android app), not only at ≥ 900 px | `TimeSheet.svelte` | Linux narrow window |
| L4-15 / C63 | note counters "n / 500" from 400 chars | `Adjust.svelte` | Linux |
| P25 | Add nap: end before start shows "Woke up must be after you fell asleep" (no wrapped 23h 45m) | `apps/dusk/src/components/Adjust.svelte` | OPPO |
| P26 | covered by B4 (mirrors on the phone) + L4-10 (neutral text) | — | OPPO |
| C28 | `install-service` on Windows writes HKCU Run "Selfkit agent" (installer's value + launcher), drops the logon task | `selfkit-agent/src/install.rs` | Windows (untested here) |
| C29 | Agenda running = comm any case or exe name agenda/agenda.AppImage/agenda_* | `selfkit-agent/src/reminders.rs` | Linux .deb/AppImage |
| C23 | Chrono and Reflect: single-instance plugin on desktop (Cargo.lock edited by hand: two dependency lines) | `apps/{chrono,reflect}/src-tauri/{Cargo.toml,src/lib.rs}`, `Cargo.lock` | Windows/Linux double launch |
| L4-12 / C50 | Dusk dark `--seg-on` lighter than the track. C51 (native <select> list follows the GTK theme, white in dark mode) not fixed: needs an in-page listbox or GTK_THEME at start — lead decision | `apps/dusk/src/app.css` | Linux dark |
| L4-13 / C57 | LAN + Automatic switches off and disabled while sync is off | `ui/src/sync/SyncSettings.svelte` | all apps |
| L4-11 / C47 | Trends: axis ticks in 12h ("9 pm"), weekly rows "Week of 16 Nov", chart says "last 30 nights" when the span is longer | `RangeChart.svelte`, `TrendsPage.svelte`, `lib/fmt.ts` | Linux 12h |
| L4-10 / C46 | Connections: neutral "wasn't found on this device" text; desktop without Chrono marks the late-screen card unavailable | `apps/dusk/src-tauri/src/{model,lib}.rs`, `ConnectionsPage.svelte` | Linux |
| L2-7 / C48 | focus ring inset inside horizontal scrollers; clicked tile blurred after Save | `apps/hisab/src/app.css`, `AddPanel.svelte` | Linux |
| L2-10 / C59 | `OptionalDate` component: empty date shows "None"/"Never"/"Any" (native text hidden) + clear button; loan due, goal deadline, recurring end, History range | `apps/hisab/src/components/OptionalDate.svelte` + 4 users | Linux + OPPO |
| L2-9 / C58 | Save toast and rows use `catLabel` ("Food + Bills" for splits) | `apps/hisab/src/lib/state.svelte.ts`, `AddPanel.svelte`, `TxRow.svelte` | Linux |
| L2-12 / C61 | Hisab and Reflect (Today tile, budget streak, reviews): budget remaining and the home budget bar count only spending in budgeted categories (`budget_spent`) | `apps/hisab/src-tauri/src/stats.rs`, `Summary.svelte`, `api.ts`, `apps/reflect/src-tauri/src/{sources,model}.rs` | Linux |
| L2-11 / C60 | goal progress clamped to 0–100 %; goal "Take out" can't exceed what is saved (Rust normalize + sheet check) | `apps/hisab/src-tauri/src/model.rs`, `GoalSheet.svelte` | Linux |

## Still open (not started yet, 2026-11-21 ~01:30)
C23, C28, C29, C46/L4-10, C47/L4-11, C48/L2-7, C50/C51/L4-12, L4-13, L3-1..5, L15 (Bangla in PDF needs an embedded font), P11, P13, P14 (perf), P15–P20 (Chrono), and anything logged after C51/P20/L16.

## Decisions taken (lead, 2026-11-20)
- B2 → provider-shared `dusk_nights` (not synced night records: each device has its own estimate, would conflict/duplicate).
- B4 → Dusk mirrors Agenda/Hisab like Reflect.
