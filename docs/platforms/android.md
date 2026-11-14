# Android

- **Env:** `source /shared/linuxelf/config/env.sh` → SDK `/shared/linuxelf/config/android-sdk` (platform 37, build-tools 36), AVD `Medium_Phone` (API 37 x86_64), JDK `/shared/linuxelf/bin/android-studio-app/jbr` (export as `JAVA_HOME`).
- **Missing:** NDK (needed by Tauri/Rust) and `cmdline-tools`. Install via Android Studio → SDK Manager → SDK Tools → NDK (Side by side) + Command-line Tools.
- **Package names:** `com.selfkit.{app}`; the ContentProvider authority is `com.selfkit.{app}.provider`, protected by a signature-level permission `com.selfkit.permission.SHARE`.
- **Rust targets:** `aarch64-linux-android`, `x86_64-linux-android` installed.
- No always-on service: WorkManager for sync/backups, AlarmManager for reminders, UsageStatsManager for Chrono ("Usage access" permission).
- Never run the emulator and the Windows Docker env at the same time.
