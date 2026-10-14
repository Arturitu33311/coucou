#!/usr/bin/env bash
# jinx/jinx-assist/deploy-gateway-server.sh — instala el gateway de Jinx (jinx-chat) en el SERVIDOR, su sitio
# principal; el del portátil (install.sh) queda de respaldo. Copia a ~/bin del servidor con respaldo de lo que
# hubiera. NO toca authorized_keys (eso lo hace una persona: ver server-dispatch) ni mueve el hilo.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="${JINX_SERVER:-server}"
stamp="$(date +%Y%m%d-%H%M%S)"
python3 -m unittest discover -s "$here" >/dev/null
ssh -o BatchMode=yes "$host" 'mkdir -p "$HOME/bin"'
for f in jinx_thread.py jinx_baseline.py jinx_chat.py jinx-chat server-dispatch; do
  ssh -o BatchMode=yes "$host" "[ -e \"\$HOME/bin/$f\" ] && cp -p \"\$HOME/bin/$f\" \"\$HOME/bin/$f.bak-$stamp\" || true"
  scp -q "$here/$f" "$host:bin/$f"
  ssh -o BatchMode=yes "$host" "chmod 0755 \"\$HOME/bin/$f\""
done
# En el servidor Hermes es local: jinx_chat lo detecta solo (default_hermes_host), sin ssh a sí mismo.
echo '{"op":"history","n":1}' | ssh -o BatchMode=yes "$host" bin/jinx-chat | head -c 160; echo
echo "ok: gateway instalado en $host:~/bin (respaldos .bak-$stamp). Falta:"
echo "    el hilo y los secretos (ver docs/MONOREPO.md, 'Where things run'), y server-dispatch en las llaves de los teléfonos."
