# Browser extension (Chrono for Selfkit)

Status: **working in Chromium 14x and Firefox 160** (tested live, see Testing). One Manifest V3 codebase in `extension/`. It records the domain of the active tab and sends it to the local agent through native messaging. Nothing goes over the network.

## What it records
- One session = one continuous stretch of one **domain** in the active tab of the **focused, non-private** window while the user is **active**. Domain = the URL's hostname, lowercased, without port, trailing dot or leading `www.`; only `http(s)` pages. Paths, queries and titles are never read or stored.
- Not tracked: private/incognito windows and tabs (checked on both window and tab), unfocused browser (focus moving to another program, `onFocusChanged(WINDOW_ID_NONE)`, closes the session at once — L3-1), idle or locked (`chrome.idle`, default threshold 120 s, or Chrono's "Time away" `idle_minutes` from the native host reply — L3-2; going idle back-dates the end by the threshold), non-web pages, Chrono `exclude_domains` (plus subdomains), while paused.
- If no heartbeat came for 150 s (browser closed, laptop asleep), the session ends at the last heartbeat. Sessions under 1 s are dropped.
- Pause: the popup pauses this browser for 15 min, 1 hour or until resumed. Chrono's `paused_until` and the agent's `pause` are applied too: the agent reports them in every reply, and the open session closes right away. The toolbar badge shows "off" while paused.

## Data: `chrono.db` → `web_sessions` (written by the agent's native host)
`{domain, browser: "chrome"|"firefox", device, start, end, tz_offset}`. `start`/`end` are UTC ms. `tz_offset` is in minutes east of UTC at the session start, computed by the extension. `device` = the agent's device id. Chromium-based browsers report `chrome`. The record id is the extension's session UUID, so re-sending a session (it is sent while still open, then again when it closes) updates the same row instead of creating a duplicate. Rows deleted in Chrono are never brought back.

Host-side validation (`native_host::write_sessions`): plausible hostname (normalized again), id 8–64 characters `[A-Za-z0-9-]`, 1 s ≤ length ≤ 24 h, end ≤ now + 5 min, Chrono excludes. While Chrono or the agent is paused, sessions that ended in the last 2 min are dropped. An id that already belongs to another collection is never overwritten.

## Native messaging
- Host name `com.selfkit.agent`, binary = `selfkit-agent` (it detects that a browser launched it). Install: `selfkit-agent install-native-host` writes
  - Linux: `~/.config/google-chrome/NativeMessagingHosts/`, `~/.config/chromium/NativeMessagingHosts/`, `~/.mozilla/native-messaging-hosts/` (`com.selfkit.agent.json`). `--chrome-profile DIR` also writes into `DIR/NativeMessagingHosts/` (a custom `--user-data-dir`). `--path` sets the binary path written into the manifest (default: the running binary, so run it from its installed place, e.g. `~/.local/bin`).
  - Windows (`#[cfg(windows)]`, not compiled yet): manifests in `%LOCALAPPDATA%\Selfkit\native-host\`, registered under `HKCU\Software\{Google\Chrome,Chromium,Microsoft\Edge,Mozilla}\NativeMessagingHosts\com.selfkit.agent` (default value = manifest path) with `reg add`.
  - `--uninstall` removes them all.
- Allowed callers: Chrome id `ijiimbgfojojhjlchgbkkhhodifbnami` (fixed by the public `key` in the manifest; add more with `--extension-id`), Firefox add-on id `chrono@selfkit`.
- Protocol: u32 length (native byte order) + UTF-8 JSON, both ways. Incoming messages are limited to 1 MiB. The extension calls `runtime.sendNativeMessage` once a minute (the browser starts the host, sends one message, the host replies and exits on EOF).
  - Request `{"type":"sessions","browser":"chrome","sessions":[{"id","domain","start","end","tz_offset"}]}` (≤ 500 per message) or `{"type":"status"}`.
  - Reply `{"ok":true,"paused":bool,"paused_until":ms|null,"exclude_domains":[…],"agent_running":bool,"device":"…","written":n,"unchanged":n,"dropped":n}`, or `{"ok":false,"error":"…"}`.
- The host works without the agent running. It writes `chrono.db` directly (SQLite WAL, 5 s busy timeout) and asks the agent's socket only whether it is paused (400 ms timeout).

## Buffering
Finished sessions wait in `storage.local.queue` until the agent confirms them. A confirmation is matched by id **and** end, so a session that closed while its open version was being sent is still sent again. The queue is capped at 5000 sessions (the oldest are dropped). All state lives in `storage.local`, because the Chrome service worker is stopped when idle: `cur`, `queue`, `pausedUntil`, `hostPaused`, `excludes`, `idleState`, `idleSecs`, `status`, `today` (local per-domain totals for the popup), `last` (last delivery result), `ackedLive`.

## Popup (design approved 2026-11-15)
320 px. Header with a status dot (accent = tracking, red = agent not reachable). A card with the current domain and "Tracking for N min", or the reason it isn't tracking (Paused / Paused in Chrono / Idle / Private window — never tracked / Browser not in front / Not a web page / Excluded site). Pause buttons 15 min · 1 hour · Until resumed, or one Resume button. "Today in this browser": top 5 domains with thin bars, from the local totals plus the open session. Footer: "Saved to Chrono · 14:02", or in red "Selfkit agent not reachable · N sessions kept until it is". Light/dark follow the system. Accent teal #2b7a6f / #5fb3a5 (from the Chrono icon).

## Install (personal use)
- Chromium/Chrome: `chrome://extensions` → Developer mode → Load unpacked → `extension/`. The `key` keeps the id fixed. Then run `selfkit-agent install-native-host`.
- Firefox: unsigned add-ons load only temporarily (`about:debugging` → Load Temporary Add-on → `extension/manifest.json`), or permanently in Developer Edition/Nightly with `xpinstall.signatures.required=false`, or after signing as unlisted on AMO. `data_collection_permissions: none` is declared (data only goes to a local app).
- Private windows: Chrome doesn't run extensions in incognito unless you allow it. Firefox likewise. Even when allowed, the extension ignores private windows.

## Testing
- `bun test extension/test`: 7 logic tests with a fake WebExtension API (domains, tab switches, private/unfocused, idle back-dating, suspend gap, buffering + ack race, Chrono pause/excludes, popup pause).
- `cargo test -p selfkit-agent native_host`: framing, validation, upsert, pause rule, launch detection.
- Live (2026-11-15, Xvfb + i3): Chromium with `--load-extension` (needs `--disable-features=DisableLoadExtensionCommandLineSwitch` on new Chromium) driven over CDP; Firefox 160 with the add-on installed temporarily over WebDriver BiDi (`webExtension.install`). Results: correct domains in `web_sessions` (www/port/path/query stripped), no overlapping sessions, idle end matches the last input, incognito window shows "private" and wrote nothing, Chrono exclude and pause and agent pause all took effect within a minute, sessions buffered while the host manifest was missing and delivered once it was back.
- Note: toggling incognito access on a command-line-loaded extension disables it ("unsupportedDeveloperExtension") until the browser restarts. That only affects testing.

## Code map
- `extension/manifest.json`: MV3; `background.service_worker` (Chrome) + `background.scripts` (Firefox ≥ 121) point to the same file; `key` fixes the Chrome id; `browser_specific_settings.gecko`.
- `extension/background.js`: `createTracker(api)`: state machine (`evaluate`), delivery (`flush`), popup messages, event wiring (`install`). Exports for tests under `module.exports`.
- `extension/popup.html|css|js`: popup.
- `extension/icons/`: from the Chrono app icon.
- `extension/test/background.test.js`: bun tests.
- Agent side: `selfkit-agent/src/native_host.rs`, `install.rs`, `chrono_settings.rs`.

## Shipped with the agent (L3-3)
The extension folder (without `test/`) is installed with the agent: Linux `.deb` → `/usr/share/selfkit-agent/extension`, tarball → `extension/`, Windows → `%LOCALAPPDATA%\Programs\Selfkit Agent\extension` (agent.nsi). Chrono → Devices and sources → How to add points there; the stable Chrome id comes from `key` in manifest.json.

## Fixes 2026-11-21 (L3-7…L3-10)
- The popup only looks (`state` doesn't run `evaluate`): opening it took focus and split the session, then showed "Browser not in front" (L3-7).
- A Chrono pause with an end (`paused_until` from the reply → `hostPausedUntil`) expires locally, also while the native host can't be reached (L3-8).
- "Today in this browser" leaves out excluded sites and time the host would drop while paused (L3-9).
- Chrono's "Never track" stores sites like the agent: lowercase, no `www.`, no port (`settings.rs` `norm_domain`, Settings `addExclusion` — L3-10).
- Focus (C52, also L3-1/L3-6): `onFocusChanged(WINDOW_ID_NONE)` sets `blurred`, which holds until a browser window is focused again or the user switches tabs; while blurred no session runs, whatever `getLastFocused()` says (on X11 it keeps reporting the browser focused). The alarm runs every 30 s, so a missed focus change costs at most half a minute. Exclude entries are normalised before matching (`www.`, port, scheme — C79).
- Program in front (L3-6, decided 2026-11-21, no new browser permissions): the native host asks the running agent for its tracker's current foreground app and adds `browser_foreground: true|false|null` to every reply (`native_host::browser_foreground`: Firefox ↔ firefox/librewolf/…, Chrome ↔ chrom*/msedge/brave/…). `false` closes the open session at its last tracked moment and keeps `notInFront` until a reply says `true`; replies come with every flush (alarm every 30 s). `null` = can't tell (agent not running, idle/locked, **Wayland**, where the tracker sees no windows): Firefox then uses a 60 s idle threshold (unless Chrono sets `idle_minutes`), Chromium keeps its focus events. Limits: without a running agent on X11/Windows, Firefox on Linux can still over-count while another app is in front for up to the idle threshold.
- Tests: `bun test extension/test` now also runs in CI (`ci.yml`).
