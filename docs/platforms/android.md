# Android

- **Env:** `source /shared/linuxelf/config/env.sh` → SDK `/shared/linuxelf/config/android-sdk` (platform 37, build-tools 36), AVD `Medium_Phone` (API 37 x86_64), JDK `/shared/linuxelf/bin/android-studio-app/jbr` (export as `JAVA_HOME`).
- **Installed:** NDK 30.0.16248370 and cmdline-tools (2026-11-15). AVD name is `Medium_Phone_API_37.0` (not `Medium_Phone`).
- **Package names:** `com.selfkit.{app}`; the ContentProvider authority is `com.selfkit.{app}.provider`, protected by a signature-level permission `com.selfkit.permission.SHARE`.
- **Rust targets:** `aarch64-linux-android`, `x86_64-linux-android` installed.
- No always-on service: WorkManager for sync/backups, AlarmManager for reminders, UsageStatsManager for Chrono ("Usage access" permission).
- Never run the emulator and the Windows Docker env at the same time.

## Build and run (Agenda as the example)
```bash
source /shared/linuxelf/config/env.sh
export JAVA_HOME=/shared/linuxelf/bin/android-studio-app/jbr NDK_HOME=$ANDROID_HOME/ndk/30.0.16248370
cd apps/agenda && bun tauri android build --debug --apk --target x86_64     # emulator APK
emulator -avd Medium_Phone_API_37.0 -no-window -no-audio -gpu swiftshader_indirect -memory 2048 &
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
adb shell monkey -p com.selfkit.agenda -c android.intent.category.LAUNCHER 1
adb exec-out screencap -p > shot.png
```
- After building, stop Gradle/Kotlin daemons (`./gradlew --stop` in `gen/android`) — they hold ~2 GB RAM.
- `tauri android init` regenerates `gen/android`; re-apply: icons (`apps/<app>/icons/android/mipmap-*` → `res/mipmap-*`, plus `mipmap-anydpi-v26/ic_launcher*.xml` and `values/ic_launcher_background.xml`) and `android:windowSoftInputMode="adjustResize"` on the activity.
- Keyboard: the phone layout sizes itself to `visualViewport.height`, so the header stays visible and quick add sits above the keyboard (Android 15+ is edge-to-edge).
- Verified 2026-11-15 on the API 37 emulator: launch, quick add with parsing, detail sheet.
