#!/bin/bash
# Off-screen screenshot of the real Dusk app (WebKitGTK under Xvfb).
# usage: apps/dusk/scripts/shot.sh <name> <system|light|dark> <WxH> "<DUSK_DEMO params>" [--rate]
#   DUSK_DEMO params: tab=night|trends|connections|settings  back=<nights ago>  edit=adjust|nap  focus=bed|wake|note_before
# Env: DUSK_SHOT_DATA (seeded data dir, required), DUSK_SHOT_OUT (default /tmp/dusk-shots), DUSK_DISPLAY (default :84)
# Needs: cargo build -p dusk --features tauri/custom-protocol; cargo build -p dusk --example dusk_seed
set -e
OUT=${DUSK_SHOT_OUT:-/tmp/dusk-shots}; mkdir -p "$OUT"
T=${CARGO_TARGET_DIR:-/shared/selfkitextra/target}/debug
export DISPLAY=${DUSK_DISPLAY:-:84} SELFKIT_DATA_DIR=${DUSK_SHOT_DATA:?set DUSK_SHOT_DATA} WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1
xdpyinfo >/dev/null 2>&1 || { echo "X display $DISPLAY not running"; exit 1; }
"$T/examples/dusk_seed" --theme "$2" >/dev/null
DUSK_WINDOW_SIZE=$3 DUSK_DEMO="$4" "$T/dusk" $5 >"$OUT/app.log" 2>&1 &
P=$!; sleep ${DUSK_SHOT_WAIT:-5}
TITLE='"Dusk"'; [ "$5" = "--rate" ] && TITLE='"Rate sleep"'
ID=$(xwininfo -root -tree | grep "$TITLE" | head -1 | awk '{print $1}')
import -window "$ID" "$OUT/$1.png"
kill $P; wait $P 2>/dev/null || true
echo "$OUT/$1.png"
