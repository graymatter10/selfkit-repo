#!/bin/bash
# Exclusive lease on the Android emulator for parallel agents (spans many commands).
# usage: dev/emu-lease.sh acquire <owner>   # waits; then starts the emulator if not running
#        dev/emu-lease.sh release <owner>   # stops the emulator and frees the lease
#        dev/emu-lease.sh status
# Leases older than 75 min are treated as stale. Never run the emulator with Windows/Wine.
L=/shared/selfkitextra/locks/emulator.d
source /shared/linuxelf/config/env.sh >/dev/null 2>&1
case "$1" in
  acquire)
    echo "[emu] DISABLED by the user: test on the phone only (dev/phone-lease.sh acquire <app>)"; exit 1
    ;;
  acquire-disabled)
    while ! mkdir "$L" 2>/dev/null; do
      age=$(( $(date +%s) - $(stat -c %Y "$L" 2>/dev/null || date +%s) ))
      [ "$age" -gt 4500 ] && { echo "[emu] stale lease ($(cat $L/owner)), taking it"; rm -rf "$L"; continue; }
      echo "[emu] busy: $(cat $L/owner 2>/dev/null) for ${age}s; waiting"; sleep 30
    done
    echo "$2" > "$L/owner"
    while [ "$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo)" -lt 4000 ]; do echo "[emu] waiting for 4 GB free RAM"; sleep 15; done
    if ! adb devices | grep -q "^emulator"; then
      "$(dirname "$0")/notify-gate.sh" "Android emulator boot for $2"
      nohup emulator -avd Medium_Phone_API_37.0 -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect -memory 2048 -no-snapshot-save >/shared/selfkitextra/locks/emulator.log 2>&1 &
      adb -s emulator-5554 wait-for-device; until [ "$(adb -s emulator-5554 shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 3; done
    fi
    echo "[emu] lease held by $2; emulator ready. Use: export ANDROID_SERIAL=emulator-5554" ;;
  release)
    [ "$(cat $L/owner 2>/dev/null)" = "$2" ] || { echo "[emu] not your lease"; exit 1; }
    adb -s emulator-5554 emu kill >/dev/null 2>&1; sleep 3; rm -rf "$L"; echo "[emu] released, emulator stopped" ;;
  status) cat "$L/owner" 2>/dev/null || echo free ;;
esac
