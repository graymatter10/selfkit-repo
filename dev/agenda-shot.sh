#!/bin/bash
# Off-screen screenshots of the real Agenda app (no window on your desktop).
# Needs: Xvfb running on :77  (Xvfb :77 -screen 0 1400x1000x24 -nolisten tcp &)
#        cargo build -p agenda --features tauri/custom-protocol
#        cargo build -p agenda --example seed
# usage: dev/agenda-shot.sh <name> <calm_light|soft_dark|paper_mono> <WxH> "<AGENDA_DEMO params>" [--capture]
#   AGENDA_DEMO params: view=today|upcoming|calendar|board  select=<n>  quick=<text>  settings=1  done=1  toast=1
# Data dir: $AGENDA_SHOT_DATA (seed it first:  SELFKIT_DATA_DIR=... target/debug/examples/seed)
set -e
OUT=${AGENDA_SHOT_OUT:-/tmp/agenda-shots}; mkdir -p "$OUT"
T=${CARGO_TARGET_DIR:-/shared/selfkitextra/target}/debug
export DISPLAY=:77 SELFKIT_DATA_DIR=${AGENDA_SHOT_DATA:?set AGENDA_SHOT_DATA} WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1
xdpyinfo >/dev/null 2>&1 || { echo "Xvfb :77 not running"; exit 1; }
"$T/examples/seed" --theme "$2" >/dev/null
AGENDA_WINDOW_SIZE=$3 AGENDA_DEMO="$4" "$T/agenda" $5 >"$OUT/app.log" 2>&1 &
P=$!; sleep 5
TITLE='"Agenda"'; [ "$5" = "--capture" ] && TITLE='"New task"'
ID=$(xwininfo -root -tree | grep "$TITLE" | head -1 | awk '{print $1}')
import -window "$ID" "$OUT/$1.png"
kill $P; wait $P 2>/dev/null || true
echo "$OUT/$1.png"
