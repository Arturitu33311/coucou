#!/usr/bin/env bash
# scripts/jinx-assist/install.sh — copies the laptop side to ~/bin, keeping a backup of what was there.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
stamp="$(date +%Y%m%d-%H%M%S)"
mkdir -p "$HOME/bin"
for f in jinx_thread.py jinx_chat.py jinx-chat coucou-dispatch; do
  if [ -e "$HOME/bin/$f" ]; then cp -p "$HOME/bin/$f" "$HOME/bin/$f.bak-$stamp"; fi
  install -m 0755 "$here/$f" "$HOME/bin/$f"
done
python3 "$HOME/bin/jinx-chat" </dev/null | head -1
echo "ok: instalado en ~/bin (backups .bak-$stamp)"
