#!/bin/bash
# Exclusive lease on one of the user's real phones (USB debugging) for parallel
# agents. Several phones can be attached; each agent gets a free one.
# usage: dev/phone-lease.sh acquire <owner> [serial]  # waits for a free phone; prints its serial
#        dev/phone-lease.sh hold <owner> <minutes>   # keep it while the screen is legitimately still
#        dev/phone-lease.sh release <owner>
#        dev/phone-lease.sh status
# While holding it, run every adb command with ANDROID_SERIAL=<serial> (or adb -s).
# These are the user's personal phones: never uninstall apps, clear app data,
# change system settings, or touch other apps. Install with `adb install -r`
# only. Delete test items and files you create. Leases older than 75 min are
# treated as stale.
D=/shared/selfkitextra/locks
source /shared/linuxelf/config/env.sh >/dev/null 2>&1
phones() { adb devices | awk 'NR>1 && $2=="device" && $1 !~ /^emulator/ {print $1}'; }
case "$1" in
  acquire)
    want=$3
    while :; do
      list=$(phones); [ -n "$want" ] && list=$want
      [ -n "$list" ] || { echo "[phone] no phone connected"; exit 1; }
      for s in $list; do
        L="$D/phone-$s.d"
        if [ -d "$L" ]; then
          age=$(( $(date +%s) - $(stat -c %Y "$L" 2>/dev/null || date +%s) ))
          [ "$age" -gt 4500 ] && { echo "[phone] stale lease on $s ($(cat $L/owner)), taking it"; rm -rf "$L"; }
        fi
        if mkdir "$L" 2>/dev/null; then
          echo "$2" > "$L/owner"
          model=$(adb -s "$s" shell getprop ro.product.model | tr -d '\r')
          size=$(adb -s "$s" shell wm size | tr -d '\r' | awk '{print $3}')
          echo "[phone] lease held by $2 on $s ($model, $size). Use: export ANDROID_SERIAL=$s"
          exit 0
        fi
      done
      echo "[phone] all phones busy: $(for s in $list; do printf '%s=%s ' "$s" "$(cat $D/phone-$s.d/owner 2>/dev/null)"; done); waiting"
      sleep 30
    done ;;
  release)
    for L in "$D"/phone-*.d; do
      [ -d "$L" ] && [ "$(cat "$L/owner" 2>/dev/null)" = "$2" ] && { rm -rf "$L"; echo "[phone] released ${L##*/phone-}" | sed 's/\.d$//'; exit 0; }
    done
    echo "[phone] $2 holds no phone lease"; exit 1 ;;
  hold)
    # Mark your phone busy for N minutes (e.g. waiting for a reminder) so the
    # watchdog (dev/phone-watch.sh) doesn't reclaim it while the screen is still.
    for L in "$D"/phone-*.d; do
      [ -d "$L" ] && [ "$(cat "$L/owner" 2>/dev/null)" = "$2" ] && { echo $(( $(date +%s) + ${3:-10} * 60 )) > "$L/hold_until"; echo "[phone] held ${3:-10} min"; exit 0; }
    done
    echo "[phone] $2 holds no phone lease"; exit 1 ;;
  status)
    for s in $(phones); do
      echo "$s $(adb -s "$s" shell getprop ro.product.model | tr -d '\r'): $(cat "$D/phone-$s.d/owner" 2>/dev/null || echo free)"
    done ;;
esac
