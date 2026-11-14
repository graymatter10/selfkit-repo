# Windows

- **Cross-build:** target `x86_64-pc-windows-gnu` (rustup target installed). Needs mingw-w64 linker — plan: build inside a Docker image with mingw + Wine for smoke tests (no sudo needed). Real tests later on a GitHub Actions Windows runner.
- **Agent:** per-user startup task (not a Service — Services can't see the active window). Tracking via `GetForegroundWindow` + `GetLastInputInfo`. None of the Windows code has been compiled yet (no mingw on the dev machine; `cargo check --target x86_64-pc-windows-gnu` fails in the C build scripts of SQLite/ring).
  - `selfkit-agent install-service` → `schtasks /Create /SC ONLOGON /TN "Selfkit agent" /TR "\"<exe>\" run" /RL LIMITED /F` (`--uninstall` deletes it).
  - `selfkit-agent install-native-host` → manifests in `%LOCALAPPDATA%\Selfkit\native-host\` (`com.selfkit.agent.chrome.json`, `.firefox.json`), registered with `reg add` under `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.selfkit.agent`, `…\Chromium\…`, `…\Microsoft\Edge\…`, `HKCU\Software\Mozilla\NativeMessagingHosts\com.selfkit.agent` (default value = manifest path).
  - IPC: named pipe `\\.\pipe\selfkit-agent`. Health: RSS via `GetProcessMemoryInfo`, CPU via `GetProcessTimes`. Reminders: a toast via PowerShell (`Windows.UI.Notifications`); the "is Agenda running" check uses `tasklist`.
- **Installer:** WebView2 `offlineInstaller` mode.
- Never run the Windows Docker env and the Android emulator at the same time.
