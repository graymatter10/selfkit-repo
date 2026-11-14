#!/bin/bash
# Appends "HH:MM:SS <cpu-package °C> <load1>" once a minute to the temperature log.
Z=$(grep -l x86_pkg_temp /sys/class/thermal/thermal_zone*/type | head -1); Z=${Z%/type}/temp
LOG=/shared/selfkitextra/locks/temp.log
while :; do echo "$(date +%H:%M:%S) $(( $(cat $Z) / 1000 )) $(cut -d' ' -f1 /proc/loadavg)" >> "$LOG"; sleep 60; done
