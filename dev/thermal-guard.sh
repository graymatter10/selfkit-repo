#!/bin/bash
# Thermal guard: keeps the laptop cool while agents build.
# Reads the CPU package temperature every 2 s. If it stays above HOT (70 °C) for
# more than HOT_SECS (30 s), or ever exceeds CRIT (80 °C), it throttles build
# work by pausing it in short cycles (SIGSTOP 3 s / SIGCONT 1 s) until the
# temperature falls below COOL (55 °C). Build processes also always run at the
# lowest priority. Only the user's build tools are touched (rustc, cargo, cc,
# ld, bun/node/vite, java/gradle, webkit test apps); never the desktop, Claude,
# adb or system processes.
# Hard limit (user rule): the CPU must not stay at or above LIMIT (70 °C) for more
# than LIMIT_SECS (40 s). Stage 1 at LIMIT_SECS: stop (SIGTERM) all build/test work
# and report it. Stage 2, still >= LIMIT for another LIMIT_SECS: save uncommitted
# work as a local snapshot ref and power off. Every event also goes to the notify
# queue so the lead reports it in the main chat.
# usage: dev/thermal-guard.sh     (runs until killed; log: /shared/selfkitextra/locks/thermal.log)
HOT=${HOT:-65}; CRIT=${CRIT:-70}; COOL=${COOL:-55}; HOT_SECS=${HOT_SECS:-10}
LIMIT=${LIMIT:-70}; LIMIT_SECS=${LIMIT_SECS:-40}
Q=/shared/selfkitextra/locks/notify
REPO=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
notify() { mkdir -p "$Q"; echo "THERMAL: $*" > "$Q/req-$(date +%s%N)-thermal"; log "$*"; }
over_since=0; stage=0
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
  if [ "$t" -ge "$LIMIT" ]; then
    [ "$over_since" = 0 ] && over_since=$now
    over=$((now - over_since))
    if [ "$stage" = 0 ] && [ "$over" -ge "$LIMIT_SECS" ]; then
      stage=1; notify "${t}°C for ${over}s (limit ${LIMIT}°C/${LIMIT_SECS}s): stopping all build and test work"
      for p in $(pids); do kill -CONT "$p" 2>/dev/null; kill -TERM "$p" 2>/dev/null; done
      over_since=$now
    elif [ "$stage" = 1 ] && [ "$over" -ge "$LIMIT_SECS" ]; then
      notify "still ${t}°C ${LIMIT_SECS}s after stopping builds: saving work and powering off"
      cd "$REPO" && idx=$(mktemp) && GIT_INDEX_FILE=$idx git read-tree HEAD && GIT_INDEX_FILE=$idx git add -A \
        && tree=$(GIT_INDEX_FILE=$idx git write-tree) && rm -f "$idx" \
        && c=$(GIT_AUTHOR_NAME="no name" GIT_AUTHOR_EMAIL="noname@noemail.com" GIT_COMMITTER_NAME="no name" GIT_COMMITTER_EMAIL="noname@noemail.com" git commit-tree "$tree" -p HEAD -m "Thermal snapshot $(date '+%F %T')") \
        && git update-ref "refs/wip/thermal-$(date +%Y%m%d-%H%M%S)" "$c" && log "snapshot $c"
      sync; sleep 3
      systemctl poweroff || busctl call org.freedesktop.login1 /org/freedesktop/login1 org.freedesktop.login1.Manager PowerOff b false
      exit 0
    fi
  else
    [ "$stage" = 1 ] && [ "$over_since" != 0 ] && notify "back to ${t}°C after stopping builds"
    over_since=0; stage=0
  fi
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
