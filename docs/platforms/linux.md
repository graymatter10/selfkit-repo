# Linux (i3, X11)

- **Build output:** `/shared/selfkitextra/target` (set in `.cargo/config.toml`; root disk is tight).
- **Tauri deps:** webkit2gtk-4.1 installed (2.54.2).
- **Agent:** runs as systemd user service (`selfkit-agent/dist/selfkit-agent.service`) or `exec --no-startup-id selfkit-agent` in i3 config.
- **Tracking:** active window via `_NET_ACTIVE_WINDOW` + `WM_CLASS`, idle via X11 ScreenSaver extension (`x11rb`).
- **Screenshots without a window:** run Xvfb on :77 and use `dev/agenda-shot.sh` (real WebKitGTK rendering; set `WEBKIT_DISABLE_DMABUF_RENDERER=1`).
- **Global shortcut:** registered through X11 (tauri-plugin-global-shortcut). If i3 already binds the same keys, i3 wins; change it in Settings or bind `exec agenda --capture` in i3.
