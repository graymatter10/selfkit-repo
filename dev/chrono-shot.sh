#!/bin/bash
# Off-screen screenshots of the real Chrono app (no window on your desktop).
# Needs: Xvfb on $CHRONO_DISPLAY (default :82):  Xvfb :82 -screen 0 1400x1000x24 -nolisten tcp &
#        cargo build -p chrono-app --features tauri/custom-protocol --bin chrono --example seed
# usage: dev/chrono-shot.sh <name> <light|dark> <WxH> "<CHRONO_DEMO params>"
#   CHRONO_DEMO params: view=today|timeline|trends|report  day=-1  week=-7  settings=1  sources=1  note=1  setup=1|2  pause=1  scroll=<px>
# Data dir: $CHRONO_SHOT_DATA (seed it first: SELFKIT_DATA_DIR=... target/debug/examples/seed)
set -e
OUT=${CHRONO_SHOT_OUT:-/tmp/chrono-shots}; mkdir -p "$OUT"
T=${CARGO_TARGET_DIR:-/shared/selfkitextra/target}/debug
export DISPLAY=${CHRONO_DISPLAY:-:82} SELFKIT_DATA_DIR=${CHRONO_SHOT_DATA:?set CHRONO_SHOT_DATA} WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1
xdpyinfo >/dev/null 2>&1 || { echo "Xvfb $DISPLAY not running"; exit 1; }
CHRONO_WINDOW_SIZE=$3 CHRONO_DEMO="theme=$2&$4" "$T/chrono" >"$OUT/app.log" 2>&1 &
P=$!; sleep ${CHRONO_SHOT_WAIT:-6}
ID=$(xwininfo -root -tree | grep '"Chrono"' | head -1 | awk '{print $1}')
import -window "$ID" "$OUT/$1.png"
kill $P; wait $P 2>/dev/null || true
echo "$OUT/$1.png"
