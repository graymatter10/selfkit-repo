# Windows

- **Cross-build:** target `x86_64-pc-windows-gnu` (rustup target installed). Needs mingw-w64 linker — plan: build inside a Docker image with mingw + Wine for smoke tests (no sudo needed). Real tests later on a GitHub Actions Windows runner.
- **Agent:** per-user startup task (not a Service — Services can't see the active window). Tracking via `GetForegroundWindow` + `GetLastInputInfo`.
- **Installer:** WebView2 `offlineInstaller` mode.
- Never run the Windows Docker env and the Android emulator at the same time.
