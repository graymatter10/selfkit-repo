#!/bin/bash
# Thermal guard: keeps the laptop cool while agents build.
# Reads the CPU package temperature every 2 s. If it stays above HOT (70 °C) for
# more than HOT_SECS (30 s), or ever exceeds CRIT (80 °C), it throttles build
# work by pausing it in short cycles (SIGSTOP 3 s / SIGCONT 1 s) until the
# temperature falls below COOL (55 °C). Build processes also always run at the
# lowest priority. Only the user's build tools are touched (rustc, cargo, cc,
# ld, bun/node/vite, java/gradle, webkit test apps); never the desktop, Claude,
# adb or system processes.
# usage: dev/thermal-guard.sh     (runs until killed; log: /shared/selfkitextra/locks/thermal.log)
HOT=${HOT:-65}; CRIT=${CRIT:-70}; COOL=${COOL:-55}; HOT_SECS=${HOT_SECS:-10}
LOG=/shared/selfkitextra/locks/thermal.log
zone=$(grep -l x86_pkg_temp /sys/class/thermal/thermal_zone*/type 2>/dev/null | head -1)
zone=${zone%/type}/temp
[ -r "$zone" ] || { echo "no x86_pkg_temp sensor"; exit 1; }
PAT='^(rustc|cargo|cc|cc1|cc1plus|c\+\+|ld|ld\.lld|lld|mold|clang|bun|node|vite|java|gradle|kotlin|agenda|hisab|chrono|dusk|reflect|chromium|WebKitWebProces|WebKitNetworkPr)$'
temp() { echo $(( $(cat "$zone") / 1000 )); }
pids() { ps -u "$(id -u)" -o pid=,comm= | awk -v p="$PAT" '$2 ~ p {print $1}'; }
log() { echo "$(date +%H:%M:%S) $*" >> "$LOG"; }
hot_since=0; throttling=0
trap 'for p in $(pids); do kill -CONT "$p" 2>/dev/null; done; exit 0' INT TERM EXIT
while :; do
  t=$(temp); now=$(date +%s)
  for p in $(pids); do renice -n 19 -p "$p" >/dev/null 2>&1; done
  if [ "$throttling" = 0 ]; then
    if [ "$t" -ge "$HOT" ]; then
      [ "$hot_since" = 0 ] && hot_since=$now
      if [ $((now - hot_since)) -ge "$HOT_SECS" ] || [ "$t" -ge "$CRIT" ]; then
        throttling=1; log "throttle ON at ${t}°C"
      fi
    else
      hot_since=0
    fi
  fi
  if [ "$throttling" = 1 ]; then
    if [ "$t" -lt "$COOL" ]; then
      throttling=0; hot_since=0; for p in $(pids); do kill -CONT "$p" 2>/dev/null; done
      log "throttle OFF at ${t}°C"; sleep 2; continue
    fi
    list=$(pids)
    for p in $list; do kill -STOP "$p" 2>/dev/null; done
    sleep 6
    for p in $list; do kill -CONT "$p" 2>/dev/null; done
    sleep 1
  else
    sleep 2
  fi
done
