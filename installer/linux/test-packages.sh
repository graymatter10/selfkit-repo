#!/bin/bash
# End-to-end test of the Linux packages on a clean Ubuntu machine (CI runner, needs sudo + xvfb).
# usage: installer/linux/test-packages.sh <dist dir>
# Per app .deb: install, binary + .desktop entry present, the app starts under Xvfb and
# creates its database, remove + purge, the binary is gone and ~/.local/share/selfkit is
# untouched. Per AppImage: starts under Xvfb (extract-and-run, no FUSE needed).
# Agent .deb: files, user unit enabled globally, agent runs and answers `status`, remove
# disables the unit and keeps data. Agent tarball: expected files.
# Docs: docs/platforms/linux.md.
set -uo pipefail
dist=$(realpath "$1")
data="${XDG_DATA_HOME:-$HOME/.local/share}/selfkit"
fails=0
check() { if eval "$1"; then echo "  ok   $2"; else echo "::error::FAIL $2"; fails=$((fails+1)); return 1; fi; }
export WEBKIT_DISABLE_DMABUF_RENDERER=1 NO_AT_BRIDGE=1
# Run a GUI binary for a few seconds under Xvfb; success = still running when the timeout hits.
# A D-Bus session bus like a real desktop has (the single-instance plugin uses it).
runs() { timeout -k 5 "$2" xvfb-run -a -s "-screen 0 1280x800x24" dbus-run-session -- "$1" >/tmp/run.log 2>&1; local rc=$?; [ $rc -eq 124 ] || { echo "  (exit $rc)"; tail -20 /tmp/run.log; }; [ $rc -eq 124 ]; }

mkdir -p "$data"; echo keep > "$data/canary-keep.txt"

for deb in "$dist"/*.deb; do
  pkg=$(dpkg-deb -f "$deb" Package)
  [ "$pkg" = selfkit-agent ] && continue
  echo "== $pkg ($(basename "$deb"))"
  sudo apt-get install -y -qq "$deb" >/dev/null 2>&1
  check "dpkg -s $pkg >/dev/null 2>&1" "installed"
  bin=$(dpkg -L "$pkg" | grep -m1 '^/usr/bin/')
  check "[ -x '$bin' ]" "binary $bin"
  check "dpkg -L $pkg | grep -q '\.desktop$'" ".desktop entry"
  check "dpkg -L $pkg | grep -q 'icons/hicolor'" "icon"
  check "runs '$bin' 12" "app keeps running under Xvfb"
  check "[ -f '$data/$pkg.db' ]" "app created $data/$pkg.db" || { tail -20 /tmp/run.log; ls -la "$data"; }
  sudo apt-get remove -y -qq "$pkg" >/dev/null 2>&1
  sudo dpkg --purge "$pkg" >/dev/null 2>&1
  check "[ ! -e '$bin' ]" "removed + purged"
  check "[ -f '$data/canary-keep.txt' ]" "data in $data kept"
done

for ai in "$dist"/*.AppImage; do
  echo "== $(basename "$ai")"
  chmod +x "$ai"
  check "APPIMAGE_EXTRACT_AND_RUN=1 runs '$ai' 15" "AppImage keeps running under Xvfb"
done

echo "== selfkit-agent .deb"
deb=$(ls "$dist"/selfkit-agent_*_amd64.deb | head -1)
sudo apt-get install -y -qq "$deb" >/dev/null 2>&1
check "[ -x /usr/bin/selfkit-agent ]" "binary"
check "[ -f /usr/lib/systemd/user/selfkit-agent.service ]" "systemd user unit"
check "grep -q 'ExecStart=/usr/bin/selfkit-agent run' /usr/lib/systemd/user/selfkit-agent.service" "unit runs /usr/bin/selfkit-agent"
check "[ -L /etc/systemd/user/default.target.wants/selfkit-agent.service ]" "unit enabled for every user"
check "[ -f /etc/opt/chrome/native-messaging-hosts/com.selfkit.agent.json ]" "Chrome native host manifest"
check "[ -f /usr/lib/mozilla/native-messaging-hosts/com.selfkit.agent.json ]" "Firefox native host manifest"
check "grep -q '\"path\": \"/usr/bin/selfkit-agent\"' /etc/opt/chrome/native-messaging-hosts/com.selfkit.agent.json" "manifest path"
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/tmp/xdg-$UID}; mkdir -p "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
SELFKIT_BACKUP=0 /usr/bin/selfkit-agent run >/tmp/agent.log 2>&1 &
apid=$!
ok=0; for i in $(seq 1 20); do /usr/bin/selfkit-agent status 2>/dev/null | grep -q '"ok": true' && { ok=1; break; }; sleep 1; done
check "[ $ok = 1 ]" "agent runs and answers status"
[ $ok = 1 ] || tail -20 /tmp/agent.log
echo '{}' > "$data/agent.json"
sudo apt-get remove -y -qq selfkit-agent >/dev/null 2>&1
check "[ ! -e /usr/bin/selfkit-agent ]" "removed"
check "[ ! -e /etc/systemd/user/default.target.wants/selfkit-agent.service ]" "unit disabled"
sleep 1
check "! kill -0 $apid 2>/dev/null" "running agent stopped on remove"
check "[ -f '$data/agent.json' ] && [ -f '$data/canary-keep.txt' ]" "data kept"
sudo dpkg --purge selfkit-agent >/dev/null 2>&1

echo "== selfkit-agent tarball"
tgz=$(ls "$dist"/selfkit-agent-*-linux-x86_64.tar.gz | head -1)
list=$(tar -tzf "$tgz")
for f in selfkit-agent install.sh uninstall.sh selfkit-agent.service; do
  check "grep -q '/$f\$' <<<\"\$list\"" "tarball has $f"
done

[ $fails -eq 0 ] || { echo "$fails check(s) failed"; exit 1; }
echo "All package checks passed."
