#!/usr/bin/env bash
# Process-level sync scenarios: real selfkit-agent processes and `sksync` devices
# (one data folder each). Complements selfkit-appsync/tests/scenarios.rs.
#   E1 LAN only: two agents on one machine + a phone through agent 1
#   E2 kill -9 in the middle of a 10k push, then resume
#   E3 clock skew (faketime: one device 3 days behind, one 2 days ahead)
#   E4 switching transports with a real agent that also syncs to the server
#   E5 test-build auto-unlock (built-in test key, no passphrase)
# Needs: Postgres (SELFKIT_TEST_PG), jq, faketime. Usage: dev/sync-e2e.sh [E1 E2 ...]
# CI: .github/workflows/sync-e2e.yml.  Local: source dev/test.env first.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${SELFKIT_TEST_PG:?source dev/test.env}"
BIN=${SK_BIN:-$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory)/debug}
W=$(mktemp -d "${SK_WORK:-/tmp}/sk-e2e.XXXXXX")
LOG=$W/logs; mkdir -p "$LOG"
KEY=$(head -c 32 /dev/urandom | base64)
PIDS=()
cleanup() { for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true; }
trap cleanup EXIT
fail() { echo "FAIL: $*"; for f in "$LOG"/*.log; do echo "== $f"; tail -n 40 "$f"; done; exit 1; }
ok() { echo "  ok - $*"; }
sk() { "$BIN/sksync" "$@"; }
free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }
wait_for() { # wait_for SECS CMD... : retry until CMD succeeds
  local t=$1; shift; local end=$((SECONDS + t))
  until "$@"; do [ $SECONDS -lt $end ] || return 1; sleep 0.5; done
}
newdb() { # fresh database per scenario
  local db="sk_e2e_$1_$RANDOM$RANDOM"
  psql "$SELFKIT_TEST_PG" -qAtc "CREATE DATABASE $db" >/dev/null
  echo "${SELFKIT_TEST_PG%/*}/$db?sslmode=disable"
}
q() { psql "${1%%\?*}" -qAtc "$2"; }
DPORT=$(free_port)
start_agent() { # start_agent NAME DIR TCPPORT PEERS [PG_URL]
  local name=$1 dir=$2 port=$3 peers=$4 pg=${5:-}
  mkdir -p "$dir/run"
  env SELFKIT_DATA_DIR="$dir" XDG_RUNTIME_DIR="$dir/run" SELFKIT_DATA_KEY="$KEY" SELFKIT_DEVICE_NAME="$name" \
    SELFKIT_LAN_PORT="$port" SELFKIT_LAN_PEERS="$peers" SELFKIT_LAN_SECS=2 SELFKIT_LAN_DISCOVERY_PORT="$DPORT" \
    SELFKIT_BACKUP=0 SELFKIT_REMINDERS=0 SELFKIT_SYNC_SECS=2 ${pg:+SELFKIT_PG_URL="$pg"} RUST_LOG=info,selfkit_agent::tracker=error \
    "$BIN/selfkit-agent" run >"$LOG/$name.log" 2>&1 &
  PIDS+=($!)
}
count_is() { [ "$(sk --dir "$1" --app "$2" count "$3" 2>/dev/null)" = "$4" ]; }
title_is() { [ "$(sk --dir "$1" --app agenda get "$2" | jq -r .data.title)" = "$3" ]; }

E1() {
  echo "E1 LAN only: two agents on one runner + phone via agent 1"
  local a1=$W/e1/pc1 a2=$W/e1/pc2 ph=$W/e1/phone p1 p2
  p1=$(free_port); p2=$(free_port)
  start_agent "Linux-PC" "$a1" "$p1" "127.0.0.1:$p2"
  start_agent "Windows-PC" "$a2" "$p2" "127.0.0.1:$p1"
  sleep 1
  sk --dir "$a1" --app agenda put tasks '{"title":"from PC1","severity":3,"steps":[],"tags":[]}' >/dev/null
  sk --dir "$a2" --app hisab put loans '{"direction":"lent","person":"Karim","amount":5000,"currency":"BDT","repayments":[]}' >/dev/null
  wait_for 90 count_is "$a2" agenda tasks 1 || fail "PC2 did not get PC1's task over LAN"
  wait_for 90 count_is "$a1" hisab loans 1 || fail "PC1 did not get PC2's loan over LAN"
  ok "agent <-> agent over LAN (no server)"
  local t
  t=$(sk --dir "$ph" --app agenda --key "$KEY" --name Phone --mode lan --no-discovery --peer "127.0.0.1:$p1" put tasks '{"title":"from phone","steps":[],"tags":[]}')
  sk --dir "$ph" --app agenda sync | jq -e '.transport=="lan" and .report.pushed>=1' >/dev/null || fail "phone LAN pass"
  count_is "$ph" agenda tasks 2 || fail "phone did not receive PC1's task"
  wait_for 90 count_is "$a2" agenda tasks 2 || fail "phone's task did not hop PC1 -> PC2"
  ok "phone -> PC1 -> PC2 (one-shot app pass against a real agent)"
  sk --dir "$a2" --app agenda put tasks "{\"title\":\"edited on PC2\"}" "$t" >/dev/null
  wait_for 90 title_is "$a1" "$t" "edited on PC2" || fail "edit PC2 -> PC1"
  sk --dir "$ph" --app agenda sync >/dev/null
  title_is "$ph" "$t" "edited on PC2" || fail "edit did not reach the phone"
  sk --dir "$ph" --app agenda status | jq -e '.headline|startswith("Synced locally with Linux-PC")' >/dev/null || fail "status headline"
  ok "edit PC2 -> PC1 -> phone; status says 'Synced locally with Linux-PC'"
  # Wrong key: a stranger phone is refused and receives nothing.
  local bad=$W/e1/stranger
  sk --dir "$bad" --app agenda --key "$(head -c 32 /dev/urandom | base64)" --mode lan --no-discovery --peer "127.0.0.1:$p1" put tasks '{"title":"x"}' >/dev/null
  if sk --dir "$bad" --app agenda sync >"$W/e1/bad.json"; then fail "wrong key accepted"; fi
  jq -e '.error|contains("encryption key")' "$W/e1/bad.json" >/dev/null || fail "wrong-key message: $(cat "$W/e1/bad.json")"
  count_is "$bad" agenda tasks 1 || fail "stranger received data"
  ok "wrong key refused over LAN, nothing received"
  cleanup; PIDS=()
}

E2() {
  echo "E2 kill -9 mid-push (10k Hisab transactions), then resume"
  local pg a=$W/e2/a b=$W/e2/b pid
  pg=$(newdb e2)
  sk --dir "$a" --app hisab --key "$KEY" --pg "$pg" --mode server bulk transactions 10000 >/dev/null
  "$BIN/sksync" --dir "$a" --app hisab sync >"$LOG/e2-first.log" 2>&1 & pid=$!   # the binary itself, so kill -9 hits it
  wait_for 120 bash -c "[ \"\$(psql '${pg%%\?*}' -qAtc 'SELECT count(*) FROM sk_records' 2>/dev/null || echo 0)\" -ge 500 ]" || fail "push never started"
  kill -9 $pid; wait $pid 2>/dev/null || true
  local n; n=$(q "$pg" "SELECT count(*) FROM sk_records WHERE collection <> 'devices'")
  [ "$n" -lt 10000 ] || fail "push finished before the kill ($n)"
  [ -f "$a/sync/hisab.lock" ] || fail "expected a stale lock file after kill -9"
  ok "killed with $n/10000 rows on the server (stale lock left behind)"
  sk --dir "$a" --app hisab sync >"$LOG/e2-resume.log" || true
  jq -e '.ok and .ran and .report.conflicts==0' "$LOG/e2-resume.log" >/dev/null || fail "resume: $(cat "$LOG/e2-resume.log")"
  [ "$(q "$pg" "SELECT count(*) FROM sk_records WHERE collection <> 'devices'")" = 10000 ] || fail "server count after resume"
  sk --dir "$a" --app hisab status | jq -e '.pending==0 and .conflicts==0' >/dev/null || fail "pending after resume"
  sk --dir "$b" --app hisab --key "$KEY" --pg "$pg" --mode server sync | jq -e '.report.pulled==10000' >/dev/null || fail "second device pull"
  ok "resume took over the stale lock; 10000 rows exactly once; second device pulled 10000"
}

E3() {
  echo "E3 clock skew (faketime)"
  command -v faketime >/dev/null || fail "faketime not installed"
  local pg a=$W/e3/a c=$W/e3/c d=$W/e3/d t
  pg=$(newdb e3)
  t=$(sk --dir "$a" --app agenda --key "$KEY" --pg "$pg" --mode server put tasks '{"title":"created now"}')
  sk --dir "$a" --app agenda sync >/dev/null
  faketime -f "-3d" "$BIN/sksync" --dir "$c" --app agenda --key "$KEY" --pg "$pg" --mode server sync >/dev/null
  faketime -f "-3d" "$BIN/sksync" --dir "$c" --app agenda put tasks '{"title":"edited by a phone 3 days behind"}' "$t" >/dev/null
  faketime -f "-3d" "$BIN/sksync" --dir "$c" --app agenda sync >/dev/null
  sk --dir "$a" --app agenda sync | jq -e '.report.conflicts==0' >/dev/null || fail "skewed edit conflicted"
  title_is "$a" "$t" "edited by a phone 3 days behind" || fail "an edit with an older clock was ignored"
  ok "edit from a device 3 days behind wins (history, not timestamps)"
  faketime -f "+2d" "$BIN/sksync" --dir "$d" --app agenda --key "$KEY" --pg "$pg" --mode server sync >/dev/null
  faketime -f "+2d" "$BIN/sksync" --dir "$d" --app agenda put tasks '{"title":"edited by a PC 2 days ahead"}' "$t" >/dev/null
  faketime -f "+2d" "$BIN/sksync" --dir "$d" --app agenda sync >/dev/null
  sk --dir "$a" --app agenda sync >/dev/null
  sk --dir "$a" --app agenda put tasks '{"title":"final edit, correct clock"}' "$t" >/dev/null
  sk --dir "$a" --app agenda sync >/dev/null
  faketime -f "+2d" "$BIN/sksync" --dir "$d" --app agenda sync | jq -e '.report.conflicts==0' >/dev/null || fail "conflict on the fast device"
  faketime -f "-3d" "$BIN/sksync" --dir "$c" --app agenda sync >/dev/null
  for x in "$a" "$c" "$d"; do title_is "$x" "$t" "final edit, correct clock" || fail "$x did not converge"; done
  ok "later edit with a 'smaller' timestamp still wins everywhere"
  # LAN with a skewed phone (beacons would be ignored, direct sessions still work).
  local p1 pc=$W/e3/pc; p1=$(free_port)
  start_agent "PC" "$pc" "$p1" ""; sleep 1
  faketime -f "-3d" "$BIN/sksync" --dir "$c" --app agenda --mode lan --no-discovery --peer "127.0.0.1:$p1" sync | jq -e '.transport=="lan"' >/dev/null || fail "skewed LAN pass"
  title_is "$pc" "$t" "final edit, correct clock" || fail "skewed phone -> PC over LAN"
  ok "skewed phone syncs over LAN"
  cleanup; PIDS=()
}

E4() {
  echo "E4 transports switch mid-way with a real agent syncing to the server"
  local pg pc=$W/e4/pc a=$W/e4/a b=$W/e4/b p1 t
  pg=$(newdb e4); p1=$(free_port)
  start_agent "Home-PC" "$pc" "$p1" "" "$pg"; sleep 1
  t=$(sk --dir "$a" --app agenda --key "$KEY" --pg "$pg" --mode auto --no-discovery --peer "127.0.0.1:$p1" put tasks '{"title":"made at home"}')
  sk --dir "$a" --app agenda sync | jq -e '.transport=="lan"' >/dev/null || fail "auto should pick LAN at home"
  sk --dir "$b" --app agenda --key "$KEY" --pg "$pg" --mode auto --no-discovery put tasks '{"title":"made outside"}' >/dev/null
  wait_for 40 bash -c "'$BIN/sksync' --dir '$b' --app agenda sync >/dev/null; [ \"\$('$BIN/sksync' --dir '$b' --app agenda count tasks)\" = 2 ]" || fail "away phone never got the home task via agent+server"
  sk --dir "$b" --app agenda status | jq -e '.last_transport=="server"' >/dev/null || fail "away phone should use the server"
  ok "home phone -> LAN -> agent -> server -> away phone"
  sk --dir "$b" --app agenda put tasks '{"title":"edited outside"}' "$t" >/dev/null
  sk --dir "$b" --app agenda sync >/dev/null
  wait_for 40 bash -c "'$BIN/sksync' --dir '$a' --app agenda sync >/dev/null; [ \"\$('$BIN/sksync' --dir '$a' --app agenda get '$t' | jq -r .data.title)\" = 'edited outside' ]" || fail "edit outside -> server -> agent -> LAN -> home phone"
  ok "edit outside reaches the home phone over LAN"
  # Home phone leaves: PC unreachable, auto falls back to the server, no conflicts.
  kill "${PIDS[-1]}"; wait "${PIDS[-1]}" 2>/dev/null || true
  sk --dir "$a" --app agenda put tasks '{"title":"edited on the bus"}' "$t" >/dev/null
  sk --dir "$a" --app agenda sync >"$LOG/e4-fallback.json" || true
  jq -e '.transport=="server" and .report.conflicts==0' "$LOG/e4-fallback.json" >/dev/null || fail "fallback to server: $(cat "$LOG/e4-fallback.json")"
  sk --dir "$b" --app agenda sync >/dev/null
  title_is "$b" "$t" "edited on the bus" || fail "bus edit"
  [ "$(sk --dir "$a" --app agenda conflicts | jq length)" = 0 ] || fail "conflicts after switching"
  ok "auto falls back to the server when the PC is gone; no conflicts"
  cleanup; PIDS=()
}

E5() {
  echo "E5 test-build auto-unlock (built-in test key)"
  local pg a=$W/e5/a b=$W/e5/b
  pg=$(newdb e5)
  sk --dir "$a" --app dusk --test-key --pg "$pg" --mode server put sleep_log '{"night":"2026-11-15","kind":"main","quality":4,"note_before":"tea","note_after":null}' n-2026-11-15 >/dev/null
  sk --dir "$a" --app dusk --test-key status | jq -e '.unlocked and .test_key' >/dev/null || fail "test key not used"
  sk --dir "$a" --app dusk --test-key sync >/dev/null || fail "test-key sync"
  sk --dir "$b" --app dusk --test-key --pg "$pg" --mode server sync >/dev/null
  [ "$(sk --dir "$b" --app dusk --test-key get n-2026-11-15 | jq -r .data.note_before)" = tea ] || fail "second test device"
  if sk --dir "$W/e5/c" --app dusk --pg "$pg" --mode server sync >"$W/e5/c.json"; then fail "a build without the test key must stay locked"; fi
  jq -e '.error|startswith("Sync is locked")' "$W/e5/c.json" >/dev/null || fail "locked message"
  ok "test builds sync with no passphrase; normal builds stay locked"
}

ALL=(E1 E2 E3 E4 E5)
for s in "${@:-${ALL[@]}}"; do "$s"; done
echo "ALL PASSED ($W)"
