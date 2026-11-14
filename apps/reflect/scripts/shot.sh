#!/bin/bash
# Off-screen screenshot of the real Reflect app (WebKitGTK under Xvfb).
# usage: apps/reflect/scripts/shot.sh <name> <system|light|dark> <WxH> "<REFLECT_DEMO params>"
#   REFLECT_DEMO params: tab=today|streaks|connections|week|month|health|settings  offset=<-n>  open=<connection id>  adding=1  scroll=<px>
# Env: REFLECT_SHOT_DATA (seeded data dir, required), REFLECT_SHOT_OUT (default scratch), REFLECT_DISPLAY (default :88)
# Needs: cargo build -p reflect --features tauri/custom-protocol; cargo build -p reflect --example reflect_seed
set -e
OUT=${REFLECT_SHOT_OUT:-/tmp/reflect-shots}; mkdir -p "$OUT"
T=${CARGO_TARGET_DIR:-/shared/selfkitextra/target}/debug
export DISPLAY=${REFLECT_DISPLAY:-:88} SELFKIT_DATA_DIR=${REFLECT_SHOT_DATA:?set REFLECT_SHOT_DATA} WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1
xdpyinfo >/dev/null 2>&1 || { echo "X display $DISPLAY not running"; exit 1; }
"$T/examples/reflect_seed" --theme "$2" --theme-only >/dev/null
REFLECT_WINDOW_SIZE=$3 REFLECT_DEMO="$4" REFLECT_NOW="${REFLECT_NOW:-}" "$T/reflect" >"$OUT/app.log" 2>&1 &
P=$!; sleep ${REFLECT_SHOT_WAIT:-10}
ID=$(xwininfo -root -tree | grep '"Reflect"' | head -1 | awk '{print $1}')
import -window "$ID" "$OUT/$1.png"
kill $P; wait $P 2>/dev/null || true
echo "$OUT/$1.png"
