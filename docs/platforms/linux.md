# Linux (i3, X11)

- **Build output:** `/shared/selfkitextra/target` (set in `.cargo/config.toml`; root disk is tight).
- **Tauri deps (needs sudo):** `sudo pacman -S --needed webkit2gtk-4.1 libappindicator-gtk3` — **not installed yet**.
- **Agent:** runs as systemd user service (`selfkit-agent/dist/selfkit-agent.service`) or `exec --no-startup-id selfkit-agent` in i3 config.
- **Tracking:** active window via `_NET_ACTIVE_WINDOW` + `WM_CLASS`, idle via X11 ScreenSaver extension (`x11rb`).
