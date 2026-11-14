#!/bin/bash
# Disk guard: if free space on / or /shared drops below MIN_GB, pauses (SIGSTOP)
# all build/test processes so nothing writes more, and logs it. Never deletes
# anything. Resumes only when space is back above RESUME_GB (e.g. the user freed
# some). Log: /shared/selfkitextra/locks/disk.log
MIN_GB=${MIN_GB:-3}; RESUME_GB=${RESUME_GB:-5}
LOG=/shared/selfkitextra/locks/disk.log
PAT='^(rustc|cargo|cc|cc1|cc1plus|ld|ld\.lld|lld|mold|clang|bun|node|vite|java|gradle|kotlin|agenda|hisab|chrono|dusk|reflect|chromium|Xvfb|docker)$'
pids() { ps -u "$(id -u)" -o pid=,comm= | awk -v p="$PAT" '$2 ~ p {print $1}'; }
freegb() { df -BG --output=avail "$1" | tail -1 | tr -dc 0-9; }
log() { echo "$(date '+%F %T') $*" >> "$LOG"; }
paused=0
log "disk guard started (/ $(freegb /)G, /shared $(freegb /shared)G free)"
while :; do
  r=$(freegb /); s=$(freegb /shared); low=$(( r < s ? r : s ))
  if [ "$paused" = 0 ] && [ "$low" -lt "$MIN_GB" ]; then
    paused=1; for p in $(pids); do kill -STOP "$p" 2>/dev/null; done
    log "LOW DISK (/ ${r}G, /shared ${s}G): all build work paused, nothing deleted"
  elif [ "$paused" = 1 ]; then
    for p in $(pids); do kill -STOP "$p" 2>/dev/null; done   # catch newly started ones
    if [ "$low" -ge "$RESUME_GB" ]; then
      paused=0; for p in $(pids); do kill -CONT "$p" 2>/dev/null; done
      log "space back (/ ${r}G, /shared ${s}G): resumed"
    fi
  fi
  sleep 15
done
