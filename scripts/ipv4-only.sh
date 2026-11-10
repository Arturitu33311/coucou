#!/bin/bash
# Turns IPv6 off on the active Ethernet and Wi-Fi connections, for networks whose IPv6 has no route out.
# There, spotifyd's Spotify Connect link ("dealer") picks Spotify's IPv6 address, fails with "No route to
# host" and never retries over IPv4, so the headless "Coucou" device never shows up and Coucou falls back
# to the Spotify app. Other programs wait long timeouts on IPv6 as well.
#
#   scripts/ipv4-only.sh          turn IPv6 off (asks for the admin password once) and restart spotifyd
#   scripts/ipv4-only.sh --undo   put IPv6 back to automatic
#
# The network drops for a couple of seconds while each connection comes back up.
set -euo pipefail

method=disabled
[ "${1:-}" = "--undo" ] && method=auto

mapfile -t uuids < <(nmcli -t -f UUID,TYPE connection show --active | awk -F: '$2 ~ /ethernet|wireless/ {print $1}')
if [ "${#uuids[@]}" -eq 0 ]; then
  echo "No active Ethernet or Wi-Fi connection." >&2
  exit 1
fi

for u in "${uuids[@]}"; do
  echo "$(nmcli -g connection.id connection show "$u"): IPv6 -> $method"
done

cmds=""
for u in "${uuids[@]}"; do
  cmds+="nmcli connection modify $u ipv6.method $method && nmcli connection up $u >/dev/null && "
done
cmds+="true"

# pkexec shows GNOME's password dialog; in a terminal without one, sudo asks there.
if command -v pkexec >/dev/null && [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
  pkexec sh -c "$cmds"
else
  sudo sh -c "$cmds"
fi

if systemctl --user is-enabled coucou-spotifyd >/dev/null 2>&1 || systemctl --user is-active coucou-spotifyd >/dev/null 2>&1; then
  systemctl --user restart coucou-spotifyd
  echo "spotifyd restarted."
fi
echo "Done."
