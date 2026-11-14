#!/bin/bash
# Off-screen screenshots of the real Hisab app (no window on your desktop).
# Needs: Xvfb on $HISAB_DISPLAY (default :81), and
#   cargo build -p hisab --features tauri/custom-protocol --bin hisab --example seed
# usage: apps/hisab/scripts/shot.sh <name> <light|dark> <WxH> "<HISAB_DEMO params>" [--capture]
#   HISAB_DEMO: page=history,insights  sheet=loan|newloan|tx|split|category|export|goal|recurring|account  i=<n>
#               amount=18.75 cat=Food kind=expense save=1
# Data dir: $HISAB_SHOT_DATA (seed once: SELFKIT_DATA_DIR=... target/debug/examples/seed)
set -e
OUT=${HISAB_SHOT_OUT:-/tmp/hisab-shots}; mkdir -p "$OUT"
T=${CARGO_TARGET_DIR:-/shared/selfkitextra/target}/debug
export DISPLAY=${HISAB_DISPLAY:-:81} SELFKIT_DATA_DIR=${HISAB_SHOT_DATA:?set HISAB_SHOT_DATA} WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1
xdpyinfo >/dev/null 2>&1 || { echo "Xvfb $DISPLAY not running"; exit 1; }
"$T/examples/seed" --theme "$2" >/dev/null
HISAB_WINDOW_SIZE=$3 HISAB_DEMO="$4" "$T/hisab" $5 >"$OUT/app.log" 2>&1 &
P=$!; sleep ${HISAB_SHOT_WAIT:-6}
TITLE='"Hisab"'; [ "$5" = "--capture" ] && TITLE='"Add expense"'
ID=$(xwininfo -root -tree | grep "$TITLE" | head -1 | awk '{print $1}')
import -window "$ID" "$OUT/$1.png"
kill $P; wait $P 2>/dev/null || true
echo "$OUT/$1.png"
