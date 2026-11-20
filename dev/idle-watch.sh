#!/bin/bash
# Idle watcher: finds unused resources so the lead can give them work (tests,
# builds, phone checks) and spots bottlenecks. Prints one line per event; meant to
# run under the lead's Monitor so every line shows up in the main chat.
#
#   IDLE github   no workflow run in progress/queued for GH_IDLE_MIN minutes
#   IDLE oppo     phone connected, no test activity for PHONE_IDLE_MIN minutes
#                 (activity = locks/phone-<serial>.last touched by test helpers)
#   IDLE laptop   load < 1, no build/test process, CPU < 65 °C for LAPTOP_IDLE_MIN
#   IDLE agent    another session's branch (claude/*, ci/installers/*): nothing running,
#                 no push for AGENT_IDLE_MIN (20) min (lead then checks that session)
#   BOTTLENECK    runs queued (not started) > 5 min · RAM free < 1.5 GB ·
#                 phone disconnected · disk < 5 GB free
# While something stays idle the line repeats every REPEAT_MIN minutes.
# usage: dev/idle-watch.sh          (cheap: one gh call + one adb call per 2 min)
GH_IDLE_MIN=${GH_IDLE_MIN:-10}; PHONE_IDLE_MIN=${PHONE_IDLE_MIN:-5}; LAPTOP_IDLE_MIN=${LAPTOP_IDLE_MIN:-10}; REPEAT_MIN=${REPEAT_MIN:-15}
L=/shared/selfkitextra/locks; mkdir -p "$L"
REPO=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
source /shared/linuxelf/config/env.sh >/dev/null 2>&1
Z=$(dirname "$(grep -l x86_pkg_temp /sys/class/thermal/thermal_zone*/type | head -1)")/temp
declare -A since last_said
now() { date +%s; }
# state KEY BOOL MSG MIN : report when BOOL has been true for MIN minutes (repeat every REPEAT_MIN)
state() {
  local k=$1 on=$2 msg=$3 min=$4 t; t=$(now)
  if [ "$on" = 1 ]; then
    [ -z "${since[$k]}" ] && since[$k]=$t
    if [ $(( t - since[$k] )) -ge $(( min * 60 )) ] && [ $(( t - ${last_said[$k]:-0} )) -ge $(( REPEAT_MIN * 60 )) ]; then
      echo "$(date +%H:%M) $msg (for $(( (t - since[$k]) / 60 )) min)"; last_said[$k]=$t
    fi
  else
    [ -n "${last_said[$k]}" ] && echo "$(date +%H:%M) busy again: $k"
    unset "since[$k]" "last_said[$k]"
  fi
}
while :; do
  # GitHub
  runs=$(cd "$REPO" && gh run list -L 40 --json status -q '[.[] | select(.status=="in_progress")] | length' 2>/dev/null || echo "?")
  queued=$(cd "$REPO" && gh run list -L 40 --json status -q '[.[] | select(.status=="queued" or .status=="waiting" or .status=="pending")] | length' 2>/dev/null || echo 0)
  [ "$runs" != "?" ] && state github $([ "$runs" = 0 ] && [ "$queued" = 0 ] && echo 1 || echo 0) "IDLE github: no workflow running" "$GH_IDLE_MIN"
  state gh-queue $([ "${queued:-0}" -gt 0 ] 2>/dev/null && echo 1 || echo 0) "BOTTLENECK github: $queued run(s) queued, not starting" 5
  # Other agent sessions (their branches: claude/*, ci/installers/*): stalled when nothing
  # runs on the branch and its last push is older than AGENT_IDLE_MIN.
  for br in $(cd "$REPO" && git ls-remote --heads origin 'claude/*' 'ci/installers/*' 2>/dev/null | awk '{sub("refs/heads/","",$2); print $2}'); do
    busy=$(cd "$REPO" && gh run list --branch "$br" -L 5 --json status -q '[.[] | select(.status!="completed")] | length' 2>/dev/null || echo 1)
    pushed=$(cd "$REPO" && gh api "repos/{owner}/{repo}/branches/$br" -q .commit.commit.committer.date 2>/dev/null)
    age=$(( $(now) - $(date -d "${pushed:-now}" +%s) ))
    state "agent-$br" $([ "$busy" = 0 ] && [ $age -ge $(( ${AGENT_IDLE_MIN:-20} * 60 )) ] && echo 1 || echo 0) "IDLE agent branch $br: nothing running, last push $(( age / 60 )) min ago" 0
  done
  # Phones (every adb device)
  devs=$(adb devices 2>/dev/null | awk 'NR>1 && $2=="device" {print $1}')
  state phone-gone $([ -z "$devs" ] && echo 1 || echo 0) "BOTTLENECK phone: no phone connected" 2
  for d in $devs; do
    f=$L/phone-$d.last; [ -f "$f" ] || touch "$f"
    age=$(( $(now) - $(stat -c %Y "$f") ))
    state "phone-$d" $([ $age -ge $(( PHONE_IDLE_MIN * 60 )) ] && echo 1 || echo 0) "IDLE phone $d: no tests running" 0
  done
  # Laptop
  load=$(awk '{print $2}' /proc/loadavg); temp=$(( $(cat "$Z") / 1000 ))
  build=$(pgrep -x 'rustc|cargo|bun|node|java|gradle|cc1|ld' | head -1)
  idle=$(awk -v l="$load" 'BEGIN{print (l<1.0)?1:0}')
  state laptop $([ "$idle" = 1 ] && [ -z "$build" ] && [ "$temp" -lt 65 ] && echo 1 || echo 0) "IDLE laptop: load $load, ${temp}°C, no build" "$LAPTOP_IDLE_MIN"
  memfree=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo)
  state ram $([ "$memfree" -lt 1500 ] && echo 1 || echo 0) "BOTTLENECK laptop RAM: ${memfree} MB free" 0
  diskfree=$(df -BG --output=avail / | tail -1 | tr -dc 0-9)
  state disk $([ "$diskfree" -lt 5 ] && echo 1 || echo 0) "BOTTLENECK disk: ${diskfree} GB free on /" 0
  sleep 120
done
