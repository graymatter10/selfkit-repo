#!/bin/bash
# Run a heavy command (Gradle/APK build, bun install, Tauri bundle) one at a time
# across parallel agents, and only when enough RAM is free.
# usage: dev/heavy.sh <lock-name> <min-free-MB> <command...>
#   dev/heavy.sh gradle 4500 bun tauri android build --apk --target aarch64
#   dev/heavy.sh bun 1500 bun install
# Cargo needs no wrapper (it already locks the shared target dir); use -j 6.
set -e
name=$1; need=$2; shift 2
mkdir -p /shared/selfkitextra/locks
exec 9>"/shared/selfkitextra/locks/$name.lock"
echo "[heavy] waiting for lock '$name'..." >&2
flock 9
while :; do
  free=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo)
  disk=$(df -m --output=avail / | tail -1)
  [ "$free" -ge "$need" ] && [ "$disk" -ge 6000 ] && break
  echo "[heavy] $name: ${free} MB RAM free (need $need), ${disk} MB disk; waiting" >&2
  sleep 15
done
case "$name" in bun) ;; *) "$(dirname "$0")/notify-gate.sh" "$name: $*" ;; esac
echo "[heavy] $name: running with ${free} MB free" >&2
"$@"
