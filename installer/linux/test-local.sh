#!/bin/bash
# Check the Linux packages on a dev PC WITHOUT root and without touching your real setup.
# usage: installer/linux/test-local.sh [dist dir]   (default /shared/selfkitextra/installers/linux)
# Every program runs in a throw-away sandbox: temp HOME / XDG dirs / SELFKIT_DATA_DIR, its own
# Xvfb display and D-Bus session (so it can't reach your keychain, your running apps or agent),
# LAN sync off. Per app .deb: unpack (bsdtar), check binary/.desktop/icon, no maintainer script
# that touches user data; start the unpacked binary for 12 s, it must stay up and create
# <app>.db. Per AppImage: same start check. Agent .deb + tarball: contents, unit, manifests,
# maintainer scripts; the unpacked agent runs and answers `status`.
# The real install/remove test (apt) runs in CI: installer/linux/test-packages.sh.
# Docs: docs/platforms/linux.md.
set -uo pipefail
dist=$(realpath "${1:-/shared/selfkitextra/installers/linux}")
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
fails=0
check() { if eval "$1"; then echo "  ok   $2"; else echo "  FAIL $2"; fails=$((fails+1)); return 1; fi; }
sandbox() { # sandbox <name>: fresh HOME etc. for one program
  local s="$work/sb-$1"; rm -rf "$s"; mkdir -p "$s/home" "$s/data" "$s/run"; chmod 700 "$s/run"; echo "$s"; }
# runbox <sandbox> <secs> <cmd...>: run under its own Xvfb + D-Bus; success = still running at the timeout.
runbox() {
  local s=$1 t=$2; shift 2
  env -i PATH="$PATH" HOME="$s/home" XDG_DATA_HOME="$s/home/.local/share" XDG_CONFIG_HOME="$s/home/.config" \
    XDG_CACHE_HOME="$s/home/.cache" XDG_RUNTIME_DIR="$s/run" SELFKIT_DATA_DIR="$s/data" SELFKIT_LAN=0 SELFKIT_BACKUP=0 \
    SELFKIT_REMINDERS=0 WEBKIT_DISABLE_DMABUF_RENDERER=1 NO_AT_BRIDGE=1 APPIMAGE_EXTRACT_AND_RUN=1 TMPDIR="$s/run" \
    timeout -k 5 "$t" xvfb-run -a -s "-screen 0 1280x800x24 -nolisten tcp" dbus-run-session -- "$@" >"$s/run.log" 2>&1
  local rc=$?; [ $rc -eq 124 ] || { echo "  (exit $rc)"; tail -15 "$s/run.log"; }; [ $rc -eq 124 ]
}
unpack() { mkdir -p "$2/data" "$2/control"; (cd "$2" && bsdtar -xf "$1" && bsdtar -xf data.tar.* -C data && bsdtar -xf control.tar.* -C control); }

for deb in "$dist"/*.deb; do
  case $(basename "$deb") in selfkit-agent_*) continue ;; esac
  x="$work/$(basename "$deb" .deb)"; unpack "$deb" "$x"
  pkg=$(sed -n 's/^Package: //p' "$x/control/control")
  echo "== $pkg ($(basename "$deb"))"
  bin=$(ls "$x"/data/usr/bin/* 2>/dev/null | head -1)
  check "[ -x '$bin' ]" "binary usr/bin/$(basename "$bin")"
  check "ls '$x'/data/usr/share/applications/*.desktop >/dev/null 2>&1" ".desktop entry"
  check "ls -d '$x'/data/usr/share/icons/hicolor >/dev/null 2>&1" "icons"
  check "! grep -rqs -e '\.local/share' -e 'selfkit' '$x'/control/postrm '$x'/control/prerm" "no maintainer script touches user data"
  check "! find '$x/data' -path '*home*' | grep -q ." "nothing installed under /home"
  s=$(sandbox "$pkg")
  check "runbox '$s' 12 '$bin'" "app keeps running (sandbox, 12 s)"
  check "[ -f '$s/data/$pkg.db' ]" "app created $pkg.db" || ls -la "$s/data"
done

for ai in "$dist"/*.AppImage; do
  echo "== $(basename "$ai")"
  chmod +x "$ai"
  app=$(basename "$ai" | cut -d_ -f1 | tr '[:upper:]' '[:lower:]')
  s=$(sandbox "ai-$app")
  check "runbox '$s' 20 '$ai'" "AppImage keeps running (sandbox, 20 s)"
  check "[ -f '$s/data/$app.db' ]" "AppImage app created $app.db"
done

echo "== selfkit-agent .deb"
deb=$(ls "$dist"/selfkit-agent_*_amd64.deb 2>/dev/null | head -1)
if check "[ -n '$deb' ]" "agent .deb present"; then
  x="$work/agent-deb"; unpack "$deb" "$x"
  check "[ -x '$x/data/usr/bin/selfkit-agent' ]" "binary"
  check "grep -q 'ExecStart=/usr/bin/selfkit-agent run' '$x/data/usr/lib/systemd/user/selfkit-agent.service'" "systemd user unit"
  check "grep -q '\"path\": \"/usr/bin/selfkit-agent\"' '$x/data/etc/opt/chrome/native-messaging-hosts/com.selfkit.agent.json'" "Chrome manifest"
  check "[ -f '$x/data/etc/chromium/native-messaging-hosts/com.selfkit.agent.json' ]" "Chromium manifest"
  check "grep -q 'chrono@selfkit' '$x/data/usr/lib/mozilla/native-messaging-hosts/com.selfkit.agent.json'" "Firefox manifest"
  check "grep -q 'systemctl --global enable' '$x/control/postinst'" "postinst enables the unit for every user"
  check "grep -q 'systemctl --global disable' '$x/control/prerm' && ! grep -q 'rm ' '$x/control/prerm'" "prerm disables the unit, deletes nothing"
  s=$(sandbox agent)
  (runbox "$s" 15 "$x/data/usr/bin/selfkit-agent" run) &
  ok=0; for i in $(seq 1 12); do sleep 1
    XDG_RUNTIME_DIR="$s/run" SELFKIT_DATA_DIR="$s/data" "$x/data/usr/bin/selfkit-agent" status 2>/dev/null | grep -q '"ok": true' && { ok=1; break; }; done
  check "[ $ok = 1 ]" "unpacked agent runs and answers status (sandbox)"
  wait
fi

echo "== selfkit-agent tarball"
tgz=$(ls "$dist"/selfkit-agent-*-linux-x86_64.tar.gz 2>/dev/null | head -1)
if check "[ -n '$tgz' ]" "tarball present"; then
  list=$(tar -tzf "$tgz")
  for f in selfkit-agent install.sh uninstall.sh selfkit-agent.service; do check "grep -q '/$f\$' <<<\"\$list\"" "tarball has $f"; done
fi

[ $fails -eq 0 ] || { echo "$fails check(s) failed"; exit 1; }
echo "All local package checks passed."
