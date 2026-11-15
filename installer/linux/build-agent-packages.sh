#!/bin/bash
# Package the Selfkit agent for Linux: a .deb (system-wide) and a tarball (per user, no root).
# usage: installer/linux/build-agent-packages.sh <selfkit-agent binary> <version> <out dir>
# Output: <out>/selfkit-agent_<version>_amd64.deb, <out>/selfkit-agent-<version>-linux-x86_64.tar.gz
# .deb contents: /usr/bin/selfkit-agent, systemd user unit /usr/lib/systemd/user/selfkit-agent.service
# (enabled for every user on install, like debhelper does), native-messaging host manifests for
# Chrome, Chromium and Firefox. Your data (~/.local/share/selfkit) is never part of the package,
# so remove and purge leave it alone.
# Docs: docs/platforms/linux.md.
set -euo pipefail
bin=$(realpath "$1"); ver=$2; out=$(realpath -m "$3")
here=$(cd "$(dirname "$0")" && pwd); root=$(cd "$here/../.." && pwd)
mkdir -p "$out"
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT

# Extension ids: keep in sync with selfkit-agent/src/install.rs.
chrome_id=$(sed -n 's/.*CHROME_EXTENSION_ID: &str = "\([a-p]*\)".*/\1/p' "$root/selfkit-agent/src/install.rs")
firefox_id=$(sed -n 's/.*FIREFOX_EXTENSION_ID: &str = "\([^"]*\)".*/\1/p' "$root/selfkit-agent/src/install.rs")
[ -n "$chrome_id" ] && [ -n "$firefox_id" ] || { echo "extension ids not found in install.rs"; exit 1; }
chrome_manifest() { cat <<EOF
{
  "name": "com.selfkit.agent",
  "description": "Selfkit agent: records browser time for Chrono",
  "path": "$1",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://$chrome_id/"]
}
EOF
}
firefox_manifest() { cat <<EOF
{
  "name": "com.selfkit.agent",
  "description": "Selfkit agent: records browser time for Chrono",
  "path": "$1",
  "type": "stdio",
  "allowed_extensions": ["$firefox_id"]
}
EOF
}
unit() { echo "# Selfkit agent, from the selfkit-agent package. Docs: /usr/share/doc/selfkit-agent/README"
         sed -e '/^#/d' -e "s|^ExecStart=.*|ExecStart=$1 run|" "$root/selfkit-agent/dist/selfkit-agent.service"; }

# ---------- .deb ----------
p=$work/deb
install -Dm755 "$bin" "$p/usr/bin/selfkit-agent"
mkdir -p "$p/usr/lib/systemd/user"
unit /usr/bin/selfkit-agent > "$p/usr/lib/systemd/user/selfkit-agent.service"
for d in etc/opt/chrome/native-messaging-hosts etc/chromium/native-messaging-hosts; do
  mkdir -p "$p/$d"; chrome_manifest /usr/bin/selfkit-agent > "$p/$d/com.selfkit.agent.json"
done
mkdir -p "$p/usr/lib/mozilla/native-messaging-hosts"
firefox_manifest /usr/bin/selfkit-agent > "$p/usr/lib/mozilla/native-messaging-hosts/com.selfkit.agent.json"
install -Dm644 "$root/selfkit-agent/icons/128x128.png" "$p/usr/share/icons/hicolor/128x128/apps/selfkit-agent.png"
mkdir -p "$p/usr/share/doc/selfkit-agent"
cat > "$p/usr/share/doc/selfkit-agent/README" <<'EOF'
Selfkit agent: background sync, activity tracking for Chrono, backups, Agenda reminders.

Autostart: the systemd user unit is enabled for every user and starts at your next login.
Start it now:     systemctl --user daemon-reload && systemctl --user start selfkit-agent
Turn it off:      systemctl --user disable --now selfkit-agent
i3 (the unit needs your X display), in ~/.config/i3/config:
  exec --no-startup-id systemctl --user import-environment DISPLAY XAUTHORITY && systemctl --user restart selfkit-agent
First setup:      selfkit-agent init      (then set-secret pg / set-secret s3 for sync)
Check:            selfkit-agent status    selfkit-agent health

Uninstall: sudo apt remove selfkit-agent. Your data in ~/.local/share/selfkit is kept.
Delete the agent's data too: rm -rf ~/.local/share/selfkit/{agent.json,health.json,agent-reminders.json,logs,backups}
EOF
size=$(du -sk "$p" | cut -f1)
deps=$(for l in $(ldd "$bin" | awk '/=> \//{print $3}'); do dpkg -S "$(realpath "$l")" 2>/dev/null || dpkg -S "$l" 2>/dev/null; done | cut -d: -f1 | sort -u | paste -sd, | sed 's/,/, /g')
mkdir -p "$p/DEBIAN"
cat > "$p/DEBIAN/control" <<EOF
Package: selfkit-agent
Version: $ver
Architecture: amd64
Maintainer: Selfkit <selfkit@localhost>
Installed-Size: $size
Depends: ${deps:-libc6}
Recommends: libnotify-bin
Section: utils
Priority: optional
Description: Selfkit agent (background sync, tracking, backups, reminders)
 Runs in the background for the Selfkit apps: records which app is in front for
 Chrono (app names only), syncs every app database, makes encrypted backups and
 shows Agenda reminders. Fully offline unless you configure your own server.
EOF
cat > "$p/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = configure ] && command -v systemctl >/dev/null 2>&1; then
  systemctl --global enable selfkit-agent.service >/dev/null 2>&1 || true
  echo "selfkit-agent: starts at your next login. Start now: systemctl --user daemon-reload && systemctl --user start selfkit-agent"
fi
exit 0
EOF
cat > "$p/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = remove ]; then
  if command -v systemctl >/dev/null 2>&1; then
    systemctl --global disable selfkit-agent.service >/dev/null 2>&1 || true
  fi
  # The binary is about to go away: stop running agents (data is not touched).
  pkill -x selfkit-agent >/dev/null 2>&1 || true
fi
exit 0
EOF
chmod 755 "$p/DEBIAN/postinst" "$p/DEBIAN/prerm"
dpkg-deb --root-owner-group --build "$p" "$out/selfkit-agent_${ver}_amd64.deb"

# ---------- tarball (per user, no root) ----------
t=$work/selfkit-agent-$ver
mkdir -p "$t"
install -m755 "$bin" "$t/selfkit-agent"
cp "$root/selfkit-agent/dist/selfkit-agent.service" "$t/"
cat > "$t/install.sh" <<'EOF'
#!/bin/sh
# Per-user install: ~/.local/bin/selfkit-agent, systemd user unit (enabled + started),
# browser native-messaging host. Data stays in ~/.local/share/selfkit.
set -e
cd "$(dirname "$0")"
mkdir -p "$HOME/.local/bin"
install -m755 selfkit-agent "$HOME/.local/bin/selfkit-agent"
"$HOME/.local/bin/selfkit-agent" install-native-host
if command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
  "$HOME/.local/bin/selfkit-agent" install-service --enable
else
  "$HOME/.local/bin/selfkit-agent" install-service
  echo "No systemd user session: start it from your session instead, e.g. i3: exec --no-startup-id ~/.local/bin/selfkit-agent run"
fi
echo "Installed. First setup: selfkit-agent init"
EOF
cat > "$t/uninstall.sh" <<'EOF'
#!/bin/sh
# Per-user uninstall. Keeps your data (~/.local/share/selfkit); pass --delete-data to delete
# the agent's own files there too (app data is never touched).
set -e
a="$HOME/.local/bin/selfkit-agent"
if [ -x "$a" ]; then
  "$a" install-service --enable --uninstall || true
  "$a" install-native-host --uninstall || true
fi
pkill -x selfkit-agent 2>/dev/null || true
rm -f "$a"
if [ "${1:-}" = --delete-data ]; then
  d="${SELFKIT_DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/selfkit}"
  rm -rf "$d/agent.json" "$d/health.json" "$d/agent-reminders.json" "$d/logs" "$d/backups"
fi
echo "Uninstalled."
EOF
chmod 755 "$t/install.sh" "$t/uninstall.sh"
tar -C "$work" -czf "$out/selfkit-agent-$ver-linux-x86_64.tar.gz" "selfkit-agent-$ver"
ls -la "$out"
