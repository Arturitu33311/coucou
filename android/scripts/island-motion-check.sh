#!/usr/bin/env bash
# Opens the island on a connected phone or emulator, records the screen while it opens and folds, and
# checks the frames: the island must never vanish and never swing past where it rests (see
# island_motion_check.py). Run it before installing a build that touches the overlay window.
#
#   scripts/island-motion-check.sh [adb-serial] [seconds]
#
# Needs adb, ffmpeg and python3 with Pillow. The tap that opens the island is on Mochi's bubble, at
# TAP_X / TAP_Y (fractions of the screen; defaults 0.70 / 0.032 suit the default layout).
# It touches the screen of the device it runs on, so do not point it at a phone someone is using.
set -euo pipefail

serial="${1:-}"
secs="${2:-40}"
adb_cmd=(adb)
[ -n "$serial" ] && adb_cmd=(adb -s "$serial")

size="$("${adb_cmd[@]}" shell wm size | tail -1 | sed 's/.*: //' | tr -d '\r')"
w="${size%x*}"
h="${size#*x}"
tap_x=$(python3 -c "print(int($w * ${TAP_X:-0.70}))")
tap_y=$(python3 -c "print(int($h * ${TAP_Y:-0.032}))")

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

"${adb_cmd[@]}" shell "screenrecord --size $((w / 2))x$((h / 2)) --time-limit $secs /sdcard/island-motion.mp4" &
rec=$!
sleep 3
"${adb_cmd[@]}" shell input tap "$tap_x" "$tap_y"
# The island folds by itself after its auto-collapse time; the recording ends on its own.
wait "$rec" || true

"${adb_cmd[@]}" pull /sdcard/island-motion.mp4 "$tmp/r.mp4" >/dev/null
mkdir "$tmp/f"
ffmpeg -loglevel error -i "$tmp/r.mp4" -vsync passthrough -vf "crop=iw:ih*0.12:0:0" "$tmp/f/f%04d.png" 2>/dev/null
python3 -I "$(dirname "$0")/island_motion_check.py" "$tmp/f"
