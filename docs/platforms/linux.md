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

## Packages (built on GitHub Actions)
`.github/workflows/linux.yml` (Ubuntu 22.04, so the AppImages run on older glibc too; push to `test/**`, by hand, or from `release.yml`; artifact `linux-installers`). Local build of the same files: `installer/linux/build-local.sh [out-dir] [app...]` (default `/shared/selfkitextra/installers/linux`; binaries then need this machine's glibc or newer; the agent .deb is packed in a Debian container when `dpkg-deb` is missing).

| File | Install | Remove |
| --- | --- | --- |
| `<App>_<version>_amd64.deb` (5) | `sudo apt install ./Agenda_0.4.2_amd64.deb` | `sudo apt remove agenda` |
| `<App>_<version>_amd64.AppImage` (5) | `chmod +x` and run (no install) | delete the file |
| `selfkit-agent_<version>_amd64.deb` | `sudo apt install ./selfkit-agent_0.1.0_amd64.deb` | `sudo apt remove selfkit-agent` |
| `selfkit-agent-<version>-linux-x86_64.tar.gz` | unpack, `./install.sh` (per user, no root) | `./uninstall.sh [--delete-data]` |

- **Agent .deb** (`installer/linux/build-agent-packages.sh`): `/usr/bin/selfkit-agent`, systemd user unit `/usr/lib/systemd/user/selfkit-agent.service` enabled for every user on install (`systemctl --global enable`, as debhelper does; starts at the next login; now: `systemctl --user daemon-reload && systemctl --user start selfkit-agent`), native-messaging host manifests in `/etc/opt/chrome/`, `/etc/chromium/` and `/usr/lib/mozilla/native-messaging-hosts/`. Remove disables the unit and stops running agents. i3 still needs the `import-environment` line above.
- **Agent tarball:** `install.sh` = copy to `~/.local/bin`, `install-native-host`, `install-service --enable` (or prints the i3 `exec` line when there is no systemd user session).
- **Your data is never in a package**, so `apt remove` and `apt purge` leave `~/.local/share/selfkit` alone (default dpkg behaviour). Delete it yourself:
  - everything: `rm -rf ~/.local/share/selfkit`
  - one app `<a>`: `rm -rf ~/.local/share/selfkit/{<a>.db,<a>.db-wal,<a>.db-shm,<a>-photos,files/<a>,backups/<a>,sync/state-<a>.json}` (Hisab also `exports/`)
  - agent only: `rm -rf ~/.local/share/selfkit/{agent.json,health.json,agent-reminders.json,logs,backups}`
  - WebView caches: `~/.local/share/com.selfkit.<app>`, `~/.cache/com.selfkit.<app>`.
- **AppImage fix (2026-11-15):** only Hisab's AppImage came out because Tauri empties `target/release/bundle/appimage/` at the start of every AppImage build, and the workflow built the apps one after another into the same target dir (Hisab was the last app). The workflow now copies each app's `.deb` and `.AppImage` to `dist/` right after its build.
- **Local check without root** (`installer/linux/test-local.sh [dist]`): unpacks each .deb (bsdtar), checks binary/.desktop/icons and that no maintainer script touches user data, and starts every app binary and AppImage in a throw-away sandbox (temp HOME/XDG dirs/`SELFKIT_DATA_DIR`, own Xvfb + D-Bus session, LAN off) where it must stay up and create `<app>.db`; agent .deb contents/unit/manifests/scripts and an unpacked agent answering `status`; tarball contents. Your real data, keychain and running apps are never touched.
- **CI test** (`installer/linux/test-packages.sh`): each app .deb installs, has binary/.desktop/icon, starts under Xvfb for 12 s and creates `<app>.db`, then remove + purge keep `~/.local/share/selfkit`; each AppImage starts under Xvfb (`APPIMAGE_EXTRACT_AND_RUN=1`); agent .deb files, global unit enable, `status` answers, remove disables and stops it and keeps data; tarball contents.
