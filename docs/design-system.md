# Design system

> **2026-11-15: superseded.** There is no mandatory Selfkit look any more. Each app is designed for its own job (non-distracting), mocked up and approved before coding — see DECISIONS.md. The notes below describe the first Agenda build and the shared plumbing in `ui/`; they are history, not rules.

Status: **built** in `ui/` with Agenda (2026-11-15). Reference: [PLANNING.md § UI principles](PLANNING.md) and the mockups artifact linked there.

- **Package `ui/` (`@selfkit/ui`):** `src/theme.css` (tokens for A/B/D as CSS variables on `html[data-theme]`, mapped to Tailwind 4 colors `bg surface sidebar line ink muted key ring accent accent-soft accent-text danger warn …`, radii `sm card sheet pill check`, fonts `sans num`; `.sk-meta` = uppercase mono in Paper & Mono), `src/theme.ts` (applyTheme with System mode), `src/ipc.ts`, `src/icons.ts`, components `Icon Checkbox Chip IconButton Toast(+showToast/undo) Sheet`. Fonts bundled via @fontsource (Inter Variable, IBM Plex Sans/Mono). Base resets live in `@layer base` so utilities win.
- **Theme accents:** the accent comes from the theme (A blue #2F5BD3, B mint #5EC2A8, D rust #A84B24), not the app; per-app colors are used only in icons.
- **Layout:** split layout — summary card on top, recent entries, fixed input panel at the bottom.
- **Themes:** A · Calm Light, B · Soft Dark, D · Paper & Mono. Mode System (pick light: A/D, dark: B) or fixed. Shared settings record, syncs.
- **Motion:** only `transform`/`opacity`, < 200 ms, no bounce.
- **Rules:** opens to main action; no badges/feeds; undo instead of confirm; virtual scrolling for long lists; bundled fonts (`@fontsource`).
- **Quick-capture shortcuts (desktop):** a global shortcut opens a small popup window that does the task by itself (add expense, add task, rate sleep, pause tracking) and closes after saving; the main app is not opened. Every shortcut is changeable in Settings (`Settings.shortcuts`, clash-checked by `Settings::set_shortcut`). Implement with Tauri's global-shortcut plugin; on i3 also document the equivalent `bindsym … exec` line as a fallback.
- **Icons:** one family: rounded square, soft diagonal gradient in the app's single accent, white line glyph. Chrono = time ring (teal), Hisab = wallet (sage), Agenda = checkbox (slate blue), Reflect = rising bars (ochre), Dusk = crescent + horizon (violet), Agent = core in an open orbit (graphite; monochrome tray icons for light/dark bars). Source of truth: `ui/icons/build_icons.py`, which writes the SVGs (`ui/icons/svg/`, plus Android adaptive foreground and monochrome layers) and every size into `apps/<app>/icons/` and `selfkit-agent/icons/`. Re-run after any change: `python3 ui/icons/build_icons.py`. Preview: `ui/icons/preview.png`. Known gap: monochrome layers draw cut-outs (Hisab's coin) as solid; fine for tinting, revisit if it looks off.
- **Departures:** none yet. Record any app-specific layout change here with its reason.
