# Design system

Status: not started (built with Tickd, step 3). Reference: [PLANNING.md § UI principles](PLANNING.md) and the mockups artifact linked there.

- **Layout:** split layout — summary card on top, recent entries, fixed input panel at the bottom.
- **Themes:** A · Calm Light, B · Soft Dark, D · Paper & Mono. Mode System (pick light: A/D, dark: B) or fixed. Shared settings record, syncs.
- **Motion:** only `transform`/`opacity`, < 200 ms, no bounce.
- **Rules:** opens to main action; no badges/feeds; undo instead of confirm; virtual scrolling for long lists; bundled fonts (`@fontsource`).
- **Quick-capture shortcuts (desktop):** a global shortcut opens a small popup window that does the task by itself (add expense, add task, rate sleep, pause tracking) and closes after saving; the main app is not opened. Every shortcut is changeable in Settings (`Settings.shortcuts`, clash-checked by `Settings::set_shortcut`). Implement with Tauri's global-shortcut plugin; on i3 also document the equivalent `bindsym … exec` line as a fallback.
- **Icons:** one family: rounded square, soft diagonal gradient in the app's single accent, white line glyph. Chrono = time ring (teal), Hisab = wallet (sage), Tickd = checkbox (slate blue), Reflect = rising bars (ochre), Dusk = crescent + horizon (violet), Agent = core in an open orbit (graphite; monochrome tray icons for light/dark bars). Source of truth: `ui/icons/build_icons.py`, which writes the SVGs (`ui/icons/svg/`, plus Android adaptive foreground and monochrome layers) and every size into `apps/<app>/icons/` and `selfkit-agent/icons/`. Re-run after any change: `python3 ui/icons/build_icons.py`. Preview: `ui/icons/preview.png`. Known gap: monochrome layers draw cut-outs (Hisab's coin) as solid; fine for tinting, revisit if it looks off.
- **Departures:** none yet. Record any app-specific layout change here with its reason.
