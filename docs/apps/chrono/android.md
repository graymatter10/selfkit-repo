# Chrono on Android: usage tracking

Android has no always-on Chrono service. Usage comes from Android itself and is imported into `chrono.db` when the app opens, when it comes back to the foreground, and every 15 minutes by a WorkManager job.

## Permissions (Setup screen, 2 steps)
1. **Usage access** (`PACKAGE_USAGE_STATS`, special app access). The setup screen explains what Chrono will and will never see, then opens Android's per-app "Usage access" page (`Settings.ACTION_USAGE_ACCESS_SETTINGS` with `package:` URI, falling back to the list). Coming back, Chrono re-checks (`AppOpsManager.OPSTR_GET_USAGE_STATS`) and moves on.
2. **Notification counts** (optional, off unless turned on). Opens the notification-listener detail page for `ChronoNotificationListener`. Android warns that the app *could* read notifications; Chrono only counts them.
"Not now" / "Skip" store `setup_done` in kv `chrono.prefs`; both permissions can be changed later in Settings → This phone and in Devices and sources.

## What is collected and how
| Data | Source | Notes |
| --- | --- | --- |
| Per-app foreground sessions (time, opens, timeline) | `UsageStatsManager.queryEvents`: `ACTIVITY_RESUMED` (1), `ACTIVITY_PAUSED` (2) / `ACTIVITY_STOPPED` (23) | Paused → resumed within 2 s in the same app is one session (activity changes). Screen off (`SCREEN_NON_INTERACTIVE` 16), lock (`KEYGUARD_SHOWN` 17) or shutdown (26) end the session. Launchers (HOME intent) and System UI are ignored. Sessions < 1 s dropped. |
| Pickups ("Screen turned on") | `SCREEN_INTERACTIVE` (15) | counted per hour |
| Unlocks | `KEYGUARD_HIDDEN` (18) | counted per hour |
| Notifications per app | `ChronoNotificationListener` (opt-in) | one per new notification; updates of the same notification (same key and `when`), ongoing ones and group summaries are not counted. Kept as hourly counts in SharedPreferences (`chrono_notif`) until imported (pruned after 8 days). `NOTIFICATION_INTERRUPTION` (12) usage events are also counted when Android delivers them (normally only to system apps); the importer takes the larger of the two counts per hour and app. |

App labels come from `PackageManager` (launchable apps are visible through the manifest `<queries>`), stored in `android_apps`.

## Import
- Kotlin `Collector.collect(ctx, since)` builds one JSON batch: `{since, now, events: [[ts, type, package, tz_offset_min]], labels, launchers, notif: [[hour, package, n]], tz_now}`.
- Rust `android::ingest` derives sessions + hourly counters, honours `chrono_settings` (exclude list by label or package; `pause_log` windows are skipped and sessions are cut at the pause start; nothing open is written while paused) and writes `app_sessions` (platform `android`) and `android_events` with deterministic ids, only when content changed (no sync churn).
- Cursor (kv `chrono.import.cursor`): the start of the hour of the earliest still-open session (or of now). The next scan starts there, so hourly counters are always recomputed from whole hours and open sessions are rewritten with the same id when they finish. Never older than 7 days (Android keeps about a week of events).
- Foreground: Svelte calls `android_import` (Rust → `run_mobile_plugin("collect")` → ingest → alerts) on start and every time the app becomes visible.
- Background: `UsagePlugin.load` enqueues the unique periodic `ImportWorker` (15 min, `KEEP`). The worker calls the Rust library through JNI (`ChronoNative.cursor/ingest`, implemented in `src-tauri/src/lib.rs` module `jni_bridge`, opening `chrono.db` in `context.dataDir`, the same folder Tauri's `app_data_dir` uses) and posts the returned alerts on channel `chrono_alerts`.

## Alerts
Goal met / limit reached and the Monday "weekly report is ready" alert are computed in Rust (`alerts.rs`), at most once per day each (kv `chrono.alerts`). Android 13+ needs the notification permission; Chrono asks for it when you turn on alerts or add a goal (Tauri notification plugin).

## Files
- `src-tauri/gen/android/app/src/main/java/com/selfkit/chrono/`: `UsagePlugin.kt` (Tauri plugin: `status`, `open`, `collect`; schedules the worker), `Collector.kt`, `NotifStore.kt`, `ChronoNotificationListener.kt`, `ImportWorker.kt`, `ChronoNative.kt` (JNI), `MainActivity.kt` (edge-to-edge, `ChronoBars` status-bar bridge).
- Manifest: `PACKAGE_USAGE_STATS`, `POST_NOTIFICATIONS`, `<queries>` for launcher/home intents, the listener `<service>`, `adjustResize`. Gradle: `androidx.work:work-runtime-ktx`. ProGuard keeps `com.selfkit.chrono.**` (looked up by name from Rust / JNI) and WorkManager's Room database classes (R8 otherwise strips `WorkDatabase_Impl.<init>` and the app crashes at start).
- `tauri android init` regenerates `gen/android`: re-apply these files, the manifest/gradle/proguard edits, icons (`apps/chrono/icons/android/mipmap-*`, `mipmap-anydpi-v26`, `values/ic_launcher_background.xml` = `#2d6865`) and `drawable/ic_stat_chrono.xml`.
