# Linux (i3, X11)

- **Build output:** `/shared/selfkitextra/target` (set in `.cargo/config.toml`; root disk is tight).
- **Tauri deps:** webkit2gtk-4.1 installed (2.54.2).
- **Agent install:**
  1. `cargo build --release -p selfkit-agent && install -m755 /shared/selfkitextra/target/release/selfkit-agent ~/.local/bin/`
  2. Keys and secrets: `selfkit-agent init` (or `unlock` on a second device), then `selfkit-agent set-secret pg` and `selfkit-agent set-secret s3` (JSON `{"endpoint","bucket","region","access_key","secret_key","path_style"}`). They go into the Secret Service keychain.
  3. Autostart: `~/.local/bin/selfkit-agent install-service --enable` writes `~/.config/systemd/user/selfkit-agent.service` (template: `selfkit-agent/dist/selfkit-agent.service`; `MemoryMax=64M`, `Restart=on-failure`), then runs `daemon-reload` + `enable --now`. Without `--enable` it only writes the file and prints the next steps.
  4. i3: a systemd user service doesn't get `DISPLAY`, so add to `~/.config/i3/config`:
     `exec --no-startup-id systemctl --user import-environment DISPLAY XAUTHORITY && systemctl --user restart selfkit-agent`
     Without systemd, use just: `exec --no-startup-id ~/.local/bin/selfkit-agent run`. If the agent starts before X, the tracker retries every 10 s, and sync, backups and reminders keep running meanwhile.
  5. Browser: load `extension/` unpacked, then `~/.local/bin/selfkit-agent install-native-host` (Chrome, Chromium, Firefox manifests).
  6. Check: `selfkit-agent status`, `selfkit-agent health`; logs: `journalctl --user -u selfkit-agent`, errors also in `~/.local/share/selfkit/logs/agent-errors.log`.
  - Remove: `selfkit-agent install-service --enable --uninstall`, `selfkit-agent install-native-host --uninstall`.
- **Reminders:** `notify-send` (libnotify). It needs a notification daemon (dunst, etc.) on the session bus.
- **Tracking:** active window via `_NET_ACTIVE_WINDOW` + `WM_CLASS`, idle via X11 ScreenSaver extension (`x11rb`). Testing under Xvfb: run a WM (i3 with its own `ipc-socket` so it doesn't clash with the real session), and `xset s reset` counts as user input.
- **Screenshots without a window:** run Xvfb on :77 and use `dev/agenda-shot.sh` (real WebKitGTK rendering; set `WEBKIT_DISABLE_DMABUF_RENDERER=1`).
- **Global shortcut:** registered through X11 (tauri-plugin-global-shortcut). If i3 already binds the same keys, i3 wins; change it in Settings or bind `exec agenda --capture` in i3.
