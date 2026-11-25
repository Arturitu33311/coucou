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
# Datos del gateway: privados (0700). La tarjeta núcleo (JINX-CORE.md) la genera el portátil con jinx-core-build
# y es lo que sirve `op: core`; se copia aquí si existe (instantánea: vuelve a correr este script para refrescarla).
ssh -o BatchMode=yes "$host" 'umask 077; mkdir -p "$HOME/jinx-emergencia/telegram"; chmod 700 "$HOME/jinx-emergencia"'
core="$HOME/jinx-emergencia/JINX-CORE.md"
if [ -r "$core" ]; then
  ssh -o BatchMode=yes "$host" '[ -e "$HOME/jinx-emergencia/JINX-CORE.md" ] && cp -p "$HOME/jinx-emergencia/JINX-CORE.md" "$HOME/jinx-emergencia/JINX-CORE.md.bak-'"$stamp"'" || true'
  scp -q "$core" "$host:jinx-emergencia/JINX-CORE.md"
  ssh -o BatchMode=yes "$host" 'chmod 600 "$HOME/jinx-emergencia/JINX-CORE.md"'
fi
# En el servidor Hermes es local: jinx_chat lo detecta solo (default_hermes_host), sin ssh a sí mismo; los
# secretos salen de la bóveda del servidor (hermes-secrets), nunca se copian.
echo '{"op":"history","n":1}' | ssh -o BatchMode=yes "$host" bin/jinx-chat | head -c 160; echo
echo "ok: gateway instalado en $host:~/bin (respaldos .bak-$stamp). Falta:"
echo "    el hilo (jinx-chat --sync-to-server desde el portátil) y server-dispatch en las llaves de los teléfonos."
