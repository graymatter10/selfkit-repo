#!/bin/bash
# Ask the lead session to warn the user on mobile before a heavy step
# (emulator boot, Gradle/APK build, Tauri bundle, Docker), then give them
# 90 s to turn the cooler up. If nobody acknowledges in 2 min, go ahead.
# usage: dev/notify-gate.sh "<what is about to run>"
Q=/shared/selfkitextra/locks/notify
mkdir -p "$Q/ack"
id="$(date +%s%N)-$$"
echo "$1" > "$Q/req-$id"
echo "[notify] asked lead to warn the user: $1" >&2
for i in $(seq 1 24); do [ -e "$Q/ack/$id" ] && break; sleep 5; done
rm -f "$Q/ack/$id" "$Q/req-$id"
echo "[notify] giving the user 90 s to start the cooler" >&2
sleep 90
