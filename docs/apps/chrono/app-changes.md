# App changes (Android app lifecycle)

Chrono records when apps are **installed, removed, reinstalled and updated** on your phones, and keeps the history of removed apps (Digital Wellbeing forgets them).

## How it works
- On every import (when Chrono opens or comes back to the foreground, and every 15 minutes by the WorkManager job) the Kotlin collector sends the list of installed launchable apps: `[package, label, firstInstallTime, lastUpdateTime, system]` from `PackageManager`.
  - Visibility comes from the manifest `<queries>` launcher intent (Android 11+). `QUERY_ALL_PACKAGES` is **not** used: every app a person can open has a launcher activity, so the launcher query covers what Chrono needs, and Google Play restricts that permission.
- Icons: a 48 px PNG (data URL) is sent only for apps that are new or updated since the last send (SharedPreferences `chrono_icons` remembers `lastUpdateTime` per package) and stored in `android_apps` (`p-<package>`: `{package, label, icon}`), together with the label. Labels and icons are never deleted, so a removed app keeps its name and icon everywhere.
- Rust (`android::app_changes`) diffs the list against `app_installs` (this device's last known state) and writes `app_changes` events. The first list a device sends is a baseline (no events).

| Event | Rule | `at` |
| --- | --- | --- |
| `installed` | package not seen before on this device | `firstInstallTime` |
| `uninstalled` | known as installed, missing from the list | the import time (within ~15 min of the real removal) |
| `reinstalled` | known as removed and back, or `firstInstallTime` moved forward (removed and installed again between two imports) | `firstInstallTime` |
| `updated` | `lastUpdateTime` moved forward (not for preinstalled system apps) | `lastUpdateTime` |

- Usage history is keyed by package: Android `app_sessions` keep `app` (the label at import time) and `package`, so a reinstalled app's totals simply continue, and a removed app's sessions stay in every total, the timeline and reports.
- **Why import often:** Android deletes an app's usage events when it is uninstalled, so use since the last import is lost. Chrono imports on open/resume and every 15 minutes, so at most ~15 minutes of use right before a removal can be missing. Manifest receivers for `PACKAGE_ADDED/REMOVED` are no longer delivered to apps since Android 8 (implicit broadcasts), so the list diff is the reliable way.

## Data
- `app_changes` (id `c-<device>-<hash(package)>-<kind>-<at>`): `{package, label, kind: installed|uninstalled|reinstalled|updated, at (UTC ms), tz_offset (min), device}`. Synced, so desktops show the phone's changes.
- `app_installs` (id `i-<device>-<hash(package)>`): `{package, label, device, installed, first_install, last_update, system, uninstalled_at?, last_uninstalled_at?}` — current state per device and package.
- `android_apps` (id `p-<package>`): `{package, label, icon?}`.
- An app counts as **uninstalled** when every device that had it has removed it (`stats::uninstalled`).

## UI
- **Trends → App changes** card (the last three installs/removals/reinstalls, "See all") → **App changes** screen (phone: full page; desktop: main area with a back button): All / Installed / Removed / Reinstalled, "Show updates" switch, grouped by month; each row has the app icon, name, an "Uninstalled" tag if it is gone, the change and time, the device (desktop) and the app's all-time use.
- Removed apps carry an **Uninstalled** tag in Most used (Today), Trends totals, the Timeline session list and the Weekly report. Trends totals have an **Include uninstalled apps** switch (shown when there are any).
- Desktop: the phone's changes arrive through sync. Desktop install tracking is not done (skipped: no cheap, reliable source on every desktop).

## Code
`gen/android/.../Collector.kt` (`packages`, `icon`, `newIcons`), `src-tauri/src/android.rs` (`app_changes`, tests `install_uninstall_reinstall`), `src-tauri/src/stats.rs` (`uninstalled`, `app_changes`, `Total.uninstalled`, `SessionRow.uninstalled`, test `uninstalled_apps_keep_history`), command `app_changes`, `src/screens/AppChanges.svelte`.
