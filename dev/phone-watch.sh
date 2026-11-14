#!/bin/bash
# Phone scheduler watchdog: keeps the user's test phones busy.
# Every minute, for each leased phone: if its screen hasn't changed (or it sits on
# the launcher) for IDLE_MIN minutes and the holder didn't mark it busy
# (dev/phone-lease.sh hold <owner> <minutes>), the lease is reclaimed so the next
# waiting agent gets the phone. Unplugged phones are released. Every reclaim is
# appended to $D/watch/reclaimed.log for the lead to tell the agent.
# Also keeps phone screens awake (wake key every 4 min).
# usage: dev/phone-watch.sh            (runs until killed; cheap: one screenshot hash per leased phone per minute)
IDLE_MIN=${IDLE_MIN:-6}
D=/shared/selfkitextra/locks; W=$D/watch; mkdir -p "$W"
source /shared/linuxelf/config/env.sh >/dev/null 2>&1
log() { echo "$(date +%H:%M:%S) $*" >> "$W/watch.log"; }
tick=0
while :; do
  now=$(date +%s)
  connected=$(adb devices | awk 'NR>1 && $2=="device" {print $1}')
  # Keep every phone's screen on (they have a lock screen and no "Stay awake"
  # option): a wake key every 4 min counts as activity, opens nothing.
  if [ $((tick % 4)) -eq 0 ]; then
    for s in $connected; do adb -s "$s" shell input keyevent KEYCODE_WAKEUP >/dev/null 2>&1; done
  fi
  tick=$((tick + 1))
  for L in "$D"/phone-*.d; do
    [ -d "$L" ] || continue
    s=${L##*/phone-}; s=${s%.d}; owner=$(cat "$L/owner" 2>/dev/null)
    if ! grep -qx "$s" <<<"$connected"; then
      rm -rf "$L"; log "released $s ($owner): phone disconnected"
      echo "$(date +%H:%M:%S) $s $owner disconnected" >> "$W/reclaimed.log"; continue
    fi
    hold=$(cat "$L/hold_until" 2>/dev/null || echo 0)
    focus=$(adb -s "$s" shell dumpsys window 2>/dev/null | grep -m1 mCurrentFocus | sed 's/.* u0 //; s/}.*//')
    shot=$(adb -s "$s" exec-out screencap 2>/dev/null | md5sum | cut -c1-12)
    sig="$focus|$shot"
    if [ "$sig" != "$(cat "$W/$s.sig" 2>/dev/null)" ] || [ "$owner" != "$(cat "$W/$s.owner" 2>/dev/null)" ]; then
      echo "$sig" > "$W/$s.sig"; echo "$now" > "$W/$s.since"; echo "$owner" > "$W/$s.owner"
      continue
    fi
    since=$(cat "$W/$s.since" 2>/dev/null || echo "$now")
    idle=$(( (now - since) / 60 ))
    [ "$now" -lt "$hold" ] && continue
    if [ "$idle" -ge "$IDLE_MIN" ]; then
      rm -rf "$L"; rm -f "$W/$s.sig" "$W/$s.since" "$W/$s.owner"
      log "reclaimed $s from $owner: idle ${idle} min (focus: $focus)"
      echo "$(date +%H:%M:%S) $s $owner idle-${idle}min" >> "$W/reclaimed.log"
    fi
  done
  sleep 60
done
