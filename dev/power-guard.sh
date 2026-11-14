#!/bin/bash
# Power guard: protects work during a power cut.
# - Charger unplugged: logs it and writes a request into the notify queue so the
#   lead posts a warning in chat (the router may be down too, so the lead may go
#   offline; local work continues on battery).
# - Battery below LOW (60 %) while on battery: saves all uncommitted work as a
#   local snapshot ref (works offline) and, if the network is up, pushes it to
#   origin as wip/power-<time>; stops build tools cleanly; then powers off.
# - Charger back before that: logs it and carries on.
# usage: dev/power-guard.sh    (runs until killed; log: /shared/selfkitextra/locks/power.log)
LOW=${LOW:-60}
AC=/sys/class/power_supply/ADP1/online
BAT=/sys/class/power_supply/BAT1/capacity
LOG=/shared/selfkitextra/locks/power.log
Q=/shared/selfkitextra/locks/notify
REPO=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
log() { echo "$(date '+%F %T') $*" >> "$LOG"; }
snapshot() {
  cd "$REPO" || return
  idx=$(mktemp); GIT_INDEX_FILE=$idx git read-tree HEAD; GIT_INDEX_FILE=$idx git add -A
  tree=$(GIT_INDEX_FILE=$idx git write-tree); rm -f "$idx"
  c=$(GIT_AUTHOR_NAME="no name" GIT_AUTHOR_EMAIL="noname@noemail.com" GIT_COMMITTER_NAME="no name" GIT_COMMITTER_EMAIL="noname@noemail.com" \
      git commit-tree "$tree" -p HEAD -m "Power-cut snapshot $(date '+%F %T') (battery low)")
  name="wip/power-$(date +%Y%m%d-%H%M%S)"
  git update-ref "refs/$name" "$c" && log "local snapshot refs/$name = $c"
  if timeout 60 env GIT_SSH_COMMAND="ssh -o BatchMode=yes -o ConnectTimeout=10" git push -q origin "${c}:refs/heads/${name}" 2>>"$LOG"; then
    log "pushed $name to origin"
  else
    log "push failed (offline?); snapshot kept locally as refs/$name"
  fi
}
was_ac=$(cat "$AC")
log "power guard started (AC=$was_ac, battery=$(cat "$BAT")%)"
while :; do
  ac=$(cat "$AC"); bat=$(cat "$BAT")
  if [ "$ac" != "$was_ac" ]; then
    if [ "$ac" = 0 ]; then
      log "charger unplugged at ${bat}%"
      mkdir -p "$Q"; echo "POWER: charger unplugged, running on battery (${bat}%). Will save work and shut down below ${LOW}%." > "$Q/req-$(date +%s%N)-power"
    else
      log "charger back at ${bat}%"
    fi
    was_ac=$ac
  fi
  if [ "$ac" = 0 ] && [ "$bat" -lt "$LOW" ]; then
    log "battery ${bat}% < ${LOW}% on battery: saving work and shutting down"
    snapshot
    pkill -TERM -u "$(id -u)" -x -f 'rustc|cargo|bun|node|java|Xvfb' 2>/dev/null
    for p in $(pgrep -u "$(id -u)" -x 'rustc|cargo|java|Xvfb|bun'); do kill -TERM "$p" 2>/dev/null; done
    sync; sleep 5
    log "powering off"
    systemctl poweroff || busctl call org.freedesktop.login1 /org/freedesktop/login1 org.freedesktop.login1.Manager PowerOff b false
    exit 0
  fi
  sleep 20
done
