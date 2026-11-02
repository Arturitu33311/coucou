#!/usr/bin/env bash
# Installs the debug APK on a phone with Play Store as the installer, so banking apps (Nu…) do not refuse to
# run beside the island (docs/BANK-APPS.md). A plain `adb install` leaves the installer null: never use it.
#
#   scripts/install-phone.sh [adb-serial] [--build] [--fix-a11y]
#     adb-serial   e.g. 100.118.153.16:36415 (wireless adb). Omit it when exactly one device is attached.
#     --build      compile the APK first (also done automatically when there is none)
#     --fix-a11y   re-enable the island's accessibility service if the install dropped it
#
# Exit codes: 0 installed and verified, 1 install failed, 2 installed but the installer is NOT the store
# (the phone would break bank apps), 3 installed, accessibility service is off.
set -euo pipefail

PKG=com.agupta07505.smartisland
SVC="$PKG/$PKG.service.SmartIslandOverlayService"
root="$(cd "$(dirname "$0")/.." && pwd)"
apk="$root/app/build/outputs/apk/debug/app-debug.apk"

serial="" build=0 fix=0
for a in "$@"; do
  case "$a" in
    --build) build=1 ;;
    --fix-a11y) fix=1 ;;
    -*) echo "unknown option $a" >&2; exit 1 ;;
    *) serial="$a" ;;
  esac
done

if [ -n "$serial" ] && [[ "$serial" == *:* ]]; then
  adb connect "$serial" >/dev/null 2>&1 || true
fi
if [ -z "$serial" ]; then
  mapfile -t devs < <(adb devices | awk 'NR>1 && $2=="device" {print $1}')
  if [ "${#devs[@]}" -ne 1 ]; then
    echo "${#devs[@]} devices attached; pass the serial (ip:port from Wireless debugging)." >&2
    echo "Tip: adb mdns services   lists phones advertising wireless debugging on this network." >&2
    exit 1
  fi
  serial="${devs[0]}"
fi
A=(adb -s "$serial")
"${A[@]}" get-state >/dev/null 2>&1 || { echo "device $serial is not reachable" >&2; exit 1; }

if [ "$build" = 1 ] || [ ! -f "$apk" ]; then
  jdk="${JAVA_HOME:-$HOME/Android/jdk21}"
  echo "building the debug APK (JAVA_HOME=$jdk)…"
  (cd "$root" && JAVA_HOME="$jdk" ./gradlew :app:assembleDebug -q)
fi
[ -f "$apk" ] || { echo "no APK at $apk" >&2; exit 1; }

echo "installing on $serial with the store as installer…"
"${A[@]}" install -r -i com.android.vending "$apk" || exit 1
sleep 4   # right after `install -r` the services are still restarting

inst="$("${A[@]}" shell pm list packages --user 0 -i "$PKG" | tr -d '\r')"
echo "$inst"
case "$inst" in
  *installer=com.android.vending*) echo "✔ installer is the store" ;;
  *) echo "✘ installer is NOT the store: bank apps will refuse to open. Reinstall with this script." >&2; exit 2 ;;
esac

enabled="$("${A[@]}" shell settings get secure enabled_accessibility_services | tr -d '\r')"
case "$enabled" in
  *"$SVC"*) echo "✔ accessibility service enabled" ;;
  *)
    if [ "$fix" = 1 ]; then
      new="$SVC"; [ -n "$enabled" ] && [ "$enabled" != null ] && new="$enabled:$SVC"
      "${A[@]}" shell settings put secure enabled_accessibility_services "$new"
      "${A[@]}" shell settings put secure accessibility_enabled 1
      echo "✔ accessibility service re-enabled"
    else
      echo "✘ accessibility service is off (the island will not show). Re-run with --fix-a11y." >&2
      exit 3
    fi ;;
esac
echo "done: $serial"
