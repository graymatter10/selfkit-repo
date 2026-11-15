#!/bin/bash
# Build the Linux packages on this PC (same steps as .github/workflows/linux.yml).
# usage: installer/linux/build-local.sh [out-dir] [app...]   (default out: /shared/selfkitextra/installers/linux)
# Note: binaries link against this machine's glibc; packages for other distros come from CI
# (Ubuntu 22.04). The agent .deb needs dpkg-deb (or Docker: it then runs in debian:bookworm-slim).
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd); cd "$root"
out=$(realpath -m "${1:-/shared/selfkitextra/installers/linux}"); shift || true
apps=("$@"); [ ${#apps[@]} -gt 0 ] || apps=(agenda chrono dusk hisab reflect)
target=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
mkdir -p "$out"
for a in "${apps[@]}"; do
  echo "== $a" >&2
  (cd "apps/$a" && "$root/dev/heavy.sh" bundle 3000 nice -n 10 bun tauri build --bundles deb,appimage)
  # Copy right away: Tauri empties bundle/appimage at the start of every AppImage build.
  cp "$target"/release/bundle/deb/*.deb "$target"/release/bundle/appimage/*.AppImage "$out/"
done
echo "== selfkit-agent" >&2
nice -n 10 cargo build --release -p selfkit-agent
ver=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
if command -v dpkg-deb >/dev/null; then
  installer/linux/build-agent-packages.sh "$target/release/selfkit-agent" "$ver" "$out"
else
  tmp=$(mktemp -d); cp "$target/release/selfkit-agent" "$tmp/"
  docker run --rm -u "$(id -u):$(id -g)" -v "$root:/src:ro" -v "$tmp:/bin-in:ro" -v "$out:/out" debian:bookworm-slim \
    bash -c 'command -v dpkg-deb >/dev/null || { echo "no dpkg-deb in image"; exit 1; }; /src/installer/linux/build-agent-packages.sh /bin-in/selfkit-agent '"$ver"' /out'
  rm -rf "$tmp"
fi
ls -la "$out"
