#!/usr/bin/env bash
# scripts/jinx-assist/install.sh — copies the laptop side to ~/bin, keeping a backup of what was there.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
stamp="$(date +%Y%m%d-%H%M%S)"
mkdir -p "$HOME/bin"
for f in $("$here/manifest.sh"); do
  if [ -e "$HOME/bin/$f" ]; then cp -p "$HOME/bin/$f" "$HOME/bin/$f.bak-$stamp"; fi
  install -m 0755 "$here/$f" "$HOME/bin/$f"
done
# health guard timer (user units; enabling is idempotent)
mkdir -p "$HOME/.config/systemd/user"
install -m 0644 "$here/systemd/jinx-health-guard.service" "$here/systemd/jinx-health-guard.timer" \
  "$here/systemd/jinx-voice-alert.service" "$HOME/.config/systemd/user/"
systemctl --user daemon-reload
systemctl --user enable --now jinx-health-guard.timer
# Voice alerts: on by default, but silent unless the headphones are on the audio relay (see jinx-voice-alert).
# `touch ~/.config/jinx/voice-alerts.off` silences them; `jinx-voice-alert --check` shows what it would say.
systemctl --user enable --now jinx-voice-alert.service
python3 "$HOME/bin/jinx-chat" </dev/null | head -1
echo "ok: instalado en ~/bin (backups .bak-$stamp)"
