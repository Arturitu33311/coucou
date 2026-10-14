#!/usr/bin/env bash
# Installs the debug APK on the phone with Play Store as the installer, so banking apps do not refuse to
# run beside the island (see docs/BANK-APPS.md). A plain `adb install` puts the installer back to null.
#
#   scripts/install-phone.sh [adb-serial]      e.g. 100.78.102.31:36415 for wireless adb
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
apk="$root/app/build/outputs/apk/debug/app-debug.apk"
[ -f "$apk" ] || { echo "no APK at $apk (run ./gradlew :app:assembleDebug)" >&2; exit 1; }

adb_args=()
[ $# -ge 1 ] && adb_args=(-s "$1")

adb "${adb_args[@]}" install -r -i com.android.vending "$apk"

echo -n "installer now: "
adb "${adb_args[@]}" shell pm list packages --user 0 -i com.agupta07505.smartisland
echo "If the island shows no overlay, re-add it in settings secure enabled_accessibility_services (docs/BANK-APPS.md)."
