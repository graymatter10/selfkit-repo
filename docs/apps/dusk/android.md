# Dusk on Android

## Usage access bridge (`DuskUsage.kt`)
Dusk can't rely on Chrono being installed, so it reads `UsageStatsManager` itself through a small JavaScript interface (`window.DuskUsage`, added in `MainActivity.onWebViewCreate`; R8 keep rule in `proguard-rules.pro`):
- `hasAccess()` — AppOps `GET_USAGE_STATS` for Dusk.
- `openAccessSettings()` — opens Settings › Usage access (package deep link, falls back to the list). The user grants it there; when Dusk comes back to the front it re-checks and imports.
- `events(from, to)` — `[[timeMs, type]]` for types 1/2 (activity resumed/paused), 15/16 (screen interactive/non-interactive), 17/18 (keyguard shown/hidden).
- `setDark(dark)` — status/navigation bar icon colour follows the app theme.

Import (`state.svelte.ts::importUsage`, on start and every time the app becomes visible): events from `max(cursor − 1 day, now − 10 days)` to now → Rust `android_import` → `sources::intervals_from_events` (screen-on periods; periods < 45 s with no unlock/app are notification glances and dropped; without screen events, resumed→paused periods) → `device_use` per night (merged, so history survives Android's few days of event retention). The manifest declares `PACKAGE_USAGE_STATS` (`tools:ignore="ProtectedPermissions"`).

## Wind-down reminder
Off by default. Turning it on asks for notification permission (Android 13+) and schedules exact alarms through `tauri-plugin-notification` (`Schedule::At`, allow-while-idle) for the next 14 days at bedtime goal − lead time (ids 7100–7113). Re-planned on every start and every goal/prefs change; turning it off cancels them. Manifest: `POST_NOTIFICATIONS`, `USE_EXACT_ALARM`, `SCHEDULE_EXACT_ALARM` (≤ API 32). Small icon: monochrome crescent `res/drawable/ic_stat_dusk.xml` (builder `.icon()`).

## Other customizations of `gen/android` (re-apply after `tauri android init`)
Icons (`apps/dusk/icons/android/mipmap-*` → `res/mipmap-*`, adaptive `mipmap-anydpi-v26/*.xml`, `values/ic_launcher_background.xml`), `windowBackground` per theme (no white flash), `adjustResize`, `DuskUsage.kt` + `MainActivity.kt`, `res/drawable/ic_stat_dusk.xml`, the `com.selfkit.sync.MIRROR` meta-data, the proguard rule.

## Build
```bash
source /shared/linuxelf/config/env.sh
export JAVA_HOME=/shared/linuxelf/bin/android-studio-app/jbr NDK_HOME=$ANDROID_HOME/ndk/30.0.16248370
cd apps/dusk && ../../dev/heavy.sh gradle 4500 bun tauri android build --apk --target aarch64
(cd src-tauri/gen/android && ./gradlew --stop)
# sign: zipalign + apksigner with $ANDROID_USER_HOME/debug.keystore → out/dusk-0.1.0-arm64.apk
```
The arm64 APK runs on the x86_64 emulator (ARM translation).

## Back button
Handled with Tauri's `onBackButtonPress` (`App.svelte::back`), not browser history: the Android WebView skips history entries added without a user gesture, so a history-based approach left the app on the second Back press (found on the OPPO test). Order: Adjust asks "Discard your changes?" if needed → Adjust → Settings → Trends/Connections → an older night → exit (`core:app:allow-exit`).
