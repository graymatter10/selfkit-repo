# Windows

Windows 10/11, 64-bit. Everything installs **per user** (no admin), except the WebView2
runtime on the rare PC that lacks it. Installers are built and tested on GitHub Actions
(`windows-latest`), never on the dev machine: `.github/workflows/windows.yml`.

## Installers

| File | What it is |
| --- | --- |
| `SelfkitSetup_<suite>_x64.exe` | **Selfkit Setup**: tick the apps you want + the agent (all ticked by default). Works offline (WebView2 offline installer embedded, used only if missing). |
| `<App>_<version>_x64-setup.exe` | One app (Tauri NSIS installer), e.g. `Agenda_0.4.2_x64-setup.exe`. |
| `SelfkitAgent_<version>_x64-setup.exe` | The background agent alone. |

Where things go:
- Apps: `%LOCALAPPDATA%\<App>\` (e.g. `%LOCALAPPDATA%\Agenda\agenda.exe`), Start menu `Selfkit\<App>`, desktop shortcut optional (checkbox on the last page).
- Agent: `%LOCALAPPDATA%\Programs\Selfkit Agent\` (`selfkit-agent.exe`, `selfkit-agent-start.exe`).
- Setup's own uninstaller: `%LOCALAPPDATA%\Programs\Selfkit\Uninstall Selfkit.exe`.
- Apps & features: one entry per app (`Agenda`, `Chrono`, `Dusk`, `Hisab`, `Reflect`), `Selfkit Agent`, and `Selfkit (all apps)` when Selfkit Setup was used. Publisher: Selfkit.

### The agent installer
- Copies the agent, registers the browser native-messaging host (`selfkit-agent install-native-host`: manifests in `%LOCALAPPDATA%\Selfkit\native-host\`, keys under `HKCU\Software\{Google\Chrome,Chromium,Microsoft\Edge,Mozilla}\NativeMessagingHosts\com.selfkit.agent`), adds autostart and starts the agent.
- **Autostart:** `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `Selfkit agent` → `selfkit-agent-start.exe`. That launcher (`installer/windows/agent-launcher.nsi`, a tiny silent NSIS exe) starts `selfkit-agent.exe run` with its console hidden: the agent is a console program, so starting it directly would show an empty console window at every sign-in.
  - Why not the logon task from `selfkit-agent install-service` (`schtasks /SC ONLOGON`): creating a logon-trigger task needs admin on Windows, and the installers are per-user. The uninstaller still deletes that task if an older manual setup created it.
- Uninstall: stops this user's agent (`taskkill`), removes the Run value, the logon task (if any), the native host (registry + manifests) and the program files.

### Selfkit Setup (combined)
- One NSIS script (`installer/windows/setup.nsi`) with a section per component. Each section carries that component's own installer and runs it silently (`/S`), so every app keeps its own Apps & features entry and can be removed alone. Chosen over one big script because the per-app installers stay the single source of truth (Tauri's install logic, upgrades, shortcuts) and Setup stays small to maintain.
- WebView2: the per-app installers use Tauri's `embedBootstrapper` (small; WebView2 is already part of Windows 10/11, and the bootstrapper downloads it only if missing). Setup embeds Microsoft's **offline** WebView2 installer once (~190 MB) and runs it only when WebView2 is missing, so the whole suite installs without internet. (Before: every app embedded the offline installer, ~190 MB each.)
- Running Setup again adds ticked components and updates installed ones; unticking does not remove anything (use Apps & features).
- `Selfkit (all apps)` in Apps & features runs every installed component's uninstaller silently, then removes itself.

## Uninstall and your data
Uninstalling **keeps your data** by default, so reinstalling carries on where you were.

- **One app:** the uninstall page has an unchecked box **"Also delete my Selfkit data for <App>"**. Ticked, it deletes only that app's data in the Selfkit data folder (below) plus its WebView data (`%APPDATA%\com.selfkit.<app>`, `%LOCALAPPDATA%\com.selfkit.<app>`). Other apps' data and shared files are never touched.
- **Agent:** unchecked box **"Also delete the Selfkit agent's data"**: `agent.json`, `health.json`, `agent-reminders.json`, `logs\`, `backups\` (local backup copies).
- **Selfkit (all apps):** unchecked box **"Also delete ALL my Selfkit data on this PC"**: the whole data folder, `%LOCALAPPDATA%\Selfkit`, every app's WebView data.
- Data you synced to your own server or other devices is never touched.

Data folder = `selfkit_core::paths::data_dir()`: `%SELFKIT_DATA_DIR%` if set, else `%APPDATA%\selfkit`. What belongs to app `<a>`: `<a>.db`, `<a>.db-wal`, `<a>.db-shm`, `<a>-photos\`, `files\<a>\`, `backups\<a>\`, `sync\state-<a>.json`, `sync\<a>.lock`, and for Hisab `exports\`. Shared (only "delete ALL" removes them): `settings.db*`, `keys.json`, `device_id`, the rest of `sync\`. With a custom `SELFKIT_DATA_DIR`, "delete ALL" removes only the files Selfkit is known to write, never the folder itself. The rules live in one place: `installer/windows/selfkit-data.nsh`.

Delete by hand: close the app, then delete the files above from `%APPDATA%\selfkit`.

Note: a file still held open (e.g. the agent syncing that app's database at that moment) cannot be deleted; quit the agent first if a delete must be complete.

## Silent install / uninstall (automation)
```bat
SelfkitSetup_0.1.0_x64.exe /S                           :: everything
SelfkitSetup_0.1.0_x64.exe /S /APPS=agenda,hisab,agent  :: pick components
"%LOCALAPPDATA%\Programs\Selfkit\Uninstall Selfkit.exe" /S [/DELETEDATA]
Agenda_0.4.2_x64-setup.exe /S
"%LOCALAPPDATA%\Agenda\uninstall.exe" /S [/DELETEDATA]
SelfkitAgent_0.1.0_x64-setup.exe /S [/NOSTART]
"%LOCALAPPDATA%\Programs\Selfkit Agent\uninstall.exe" /S [/DELETEDATA]
```
NSIS uninstallers copy themselves to `%TEMP%` and return at once; wait for the Apps & features key to disappear (or add `_?=<install dir>` to wait, then delete `uninstall.exe` yourself).

## Build (CI)
`windows.yml` (push to `test/**` or `ci/installers/**`, by hand, or from `release.yml`):
1. `cargo test -p selfkit-core -p selfkit-agent`, `cargo build --release -p selfkit-agent`.
2. Each app: `bun tauri build --bundles nsis`, copied to `dist/` right away.
3. `makensis` (preinstalled on the runner; `choco install nsis` fallback): launcher → agent installer → stage component installers + download the WebView2 offline installer (Authenticode signature checked) → Selfkit Setup.
4. `installer/windows/test-installers.ps1`: per app: silent install, exe + `Selfkit\<App>.lnk` + Apps & features key, launch 10 s (must stay up and create `<app>.db`), uninstall keeps data, reinstall + `/DELETEDATA` deletes only that app's data. Agent: Run value, native host keys + manifest path, running with no window, `selfkit-agent status` over the named pipe, uninstall removes all of it and keeps data, `/NOSTART`, `/DELETEDATA`. Setup: install all, combined uninstall keeps data, `/APPS=hisab,agent`, combined uninstall `/DELETEDATA` removes the data folder.
5. Artifact `windows-installers` (14 days).

From the dev machine: `installer/ci-installers.sh [installers] [out-dir]` (snapshot of the working tree → `ci/installers/<stamp>`, which triggers only `windows.yml`; waits, downloads to `<out-dir>/<stamp>/`, deletes the branch).

App config (each `apps/<app>/src-tauri/tauri.conf.json` → `bundle`): `publisher`/`copyright` "Selfkit", `windows.webviewInstallMode` `embedBootstrapper`, `windows.nsis`: `installMode` `currentUser`, `startMenuFolder` `Selfkit`, `languages` `["English"]`, `customLanguageFiles` → `installer/windows/app-english.nsh` (Tauri's English strings with the delete-data label changed; re-copy from tauri-bundler when the CLI is upgraded), `installerHooks` → `installer/windows/app-hooks.nsh` (`/DELETEDATA` + per-app data delete).

### Code map
- `installer/windows/selfkit-data.nsh`: data folder + what to delete (per app, agent, all).
- `installer/windows/app-hooks.nsh`, `app-english.nsh`: Tauri per-app installer hooks + strings.
- `installer/windows/agent.nsi`, `agent-launcher.nsi`: agent installer + hidden launcher.
- `installer/windows/setup.nsi`: Selfkit Setup + combined uninstaller.
- `installer/windows/test-installers.ps1`: CI end-to-end test.

## Runtime notes
- **Agent:** per-user (not a Service: Services can't see the active window). Tracking via `GetForegroundWindow` + `GetLastInputInfo`. Compiles and its tests pass on the Windows runner.
  - `selfkit-agent install-service` → `schtasks /Create /SC ONLOGON /TN "Selfkit agent" ...` (needs admin; the installer uses the Run key instead, see above). `--uninstall` deletes it.
  - `selfkit-agent install-native-host` → see above.
  - IPC: named pipe `\\.\pipe\selfkit-agent`. Health: RSS via `GetProcessMemoryInfo`, CPU via `GetProcessTimes`. Reminders: a toast via PowerShell (`Windows.UI.Notifications`); the "is Agenda running" check uses `tasklist`.
- Local cross-build (`x86_64-pc-windows-gnu`) is not set up (no mingw on the dev machine); use CI.
- Never run a Windows Docker env and the Android emulator at the same time.
