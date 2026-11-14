# selfkit-sync (Android)

Kotlin library every Selfkit app includes for sync on Android. The sync logic itself is Rust ([selfkit-appsync](../../docs/components/selfkit-appsync.md), feature `android`); this library adds what only Android can do:

- **WorkManager** periodic job (~15 min, needs a network) + a one-off run on open + run now (`SyncWorker`).
- **MulticastLock** around every pass and while the app is in the foreground (LAN discovery broadcasts).
- **Battery hint**: `needsBatteryTip()` (OPPO/realme/OnePlus/vivo/iQOO/Xiaomi/Huawei/Honor without the exemption) and `batteryIntent()` (vendor "background activity" screen if present, else Android's).
- **Secrets**: `SelfkitSecrets.wrapKey()`: a 32-byte key kept encrypted by the Android Keystore; it seals `<data_dir>/sync/secrets.json` (data key, server passwords).
- **ContentProvider** `content://com.selfkit.<app>.provider` with signature permission `com.selfkit.permission.SHARE` (companion-ready; see the class comment for the URIs and `call()` methods).

## Integrate into an app (checklist)
1. **Rust** (`apps/<app>/src-tauri/Cargo.toml`):
   ```toml
   selfkit-appsync = { path = "../../../selfkit-appsync" }
   [target.'cfg(target_os = "android")'.dependencies]
   selfkit-appsync = { path = "../../../selfkit-appsync", features = ["android"] }
   ```
   Test builds: add `features = ["test-key"]` (auto-unlock, no prompts). Release builds: never.
2. **Rust state**: build one `AppSync` at startup:
   ```rust
   let secrets: Arc<dyn SecretStore> = if cfg!(target_os = "android") {
       Arc::new(selfkit_appsync::android::secrets(&data_dir))   // wrap key set by SelfkitSync.init
   } else { Arc::new(selfkit_appsync::secrets::KeychainSecrets) };
   let sync = AppSync::new(AppSyncConfig::new("<app>", &data_dir, secrets))?;
   ```
   Tauri commands: `sync_status`, `sync_now`, `sync_settings_get/set`, `sync_server_set/test`, `sync_encryption_setup/unlock`, `sync_conflicts`, `sync_resolve`, each a thin wrapper. On desktop call `sync_now(Trigger::Open)` once at start (it asks the agent).
3. **Gradle** (`src-tauri/gen/android`):
   - `settings.gradle`: `include ':selfkit-sync'` and `project(':selfkit-sync').projectDir = new File(rootDir, '../../../../android-plugins/selfkit-sync')`
   - `app/build.gradle.kts`: `implementation(project(":selfkit-sync"))`
4. **Manifest** (`app/src/main/AndroidManifest.xml`, inside `<application>`):
   ```xml
   <meta-data android:name="com.selfkit.sync.APP" android:value="<app>" />
   <meta-data android:name="com.selfkit.sync.LIB" android:value="<app>_lib" />
   ```
   Add `com.selfkit.sync.DATA_SUBDIR` only if the Rust data dir is not `filesDir`. Permissions and the provider come from this library's manifest.
5. **MainActivity.kt**: `SelfkitSync.init(this)` in `onCreate` (after `super`), `SelfkitSync.onForeground(this)` in `onResume`, `SelfkitSync.onBackground(this)` in `onPause`. After the Sync settings screen saves settings, call `SelfkitSync.schedule(context)` (or let the Rust command trigger it through a JS bridge, like `AgendaBars`).
6. **UI**: Sync settings section and conflict screen from the approved mockups; show the battery tip when `needsBatteryTip()` is true (JS bridge), button → `startActivity(batteryIntent())`.
7. **Test**: two phones + the laptop on the same Wi-Fi: status "Synced locally with …"; Wi-Fi off → "Synced via server"; airplane mode → "Waiting to sync · N changes saved here"; edit the same item on two devices → conflict screen.

## Build check
CI (`.github/workflows/sync-e2e.yml`, job `android-check`) builds this module standalone (`gradle -p android-plugins/selfkit-sync assembleRelease`, AGP 9.3.1 / Kotlin 2.2.10 like the apps) and the Rust JNI side for `aarch64-linux-android`. No local Gradle builds.
