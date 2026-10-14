#!/usr/bin/env bash
# scripts/jinx-assist/deploy-server.sh — copies the Hermes bridge to the server (new file only, backup if it existed).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="${JINX_HERMES_SSH:-server}"
stamp="$(date +%Y%m%d-%H%M%S)"
for f in jinx_hermes_chat.py coucou_pendientes.py; do
  dest="/home/alan/.hermes/scripts/$f"
  ssh -o BatchMode=yes "$host" "[ -e '$dest' ] && cp -p '$dest' '$dest.bak-$stamp' || true"
  scp -q "$here/server/$f" "$host:$dest"
done
dest="/home/alan/.hermes/scripts/jinx_hermes_chat.py"
echo '{"op":"history","n":1}' | ssh -o BatchMode=yes "$host" \
  "/home/alan/.hermes/hermes-agent/venv/bin/python $dest" | head -c 120; echo
echo "ok: puente de Hermes y gateway de pendientes desplegados en $host"
