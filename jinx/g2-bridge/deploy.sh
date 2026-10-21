#!/usr/bin/env bash
# jinx/g2-bridge/deploy.sh — pone el puente de las gafas Even G2 en el SERVIDOR (loopback :8898) y le da HTTPS con
# Tailscale Serve. Lo corre una persona desde el portátil; desde la nube no hay acceso al servidor.
#   ./deploy.sh            copia, arranca y comprueba
# El gateway (jinx_chat.py) también debe estar al día: el origen "g2" y la op "deliver" son nuevos →
#   bash ../jinx-assist/deploy-gateway-server.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="${JINX_SERVER:-server}"
dest='$HOME/g2-bridge'
stamp="$(date +%Y%m%d-%H%M%S)"

python3 -m unittest discover -s "$here" -p 'test_*.py' >/dev/null
python3 -m unittest discover -s "$here/../jinx-assist" -p 'test_g2*.py' >/dev/null

ssh -o BatchMode=yes "$host" "mkdir -p $dest ~/.config/systemd/user"
for f in g2_bridge.py g2_stt.py; do
  ssh -o BatchMode=yes "$host" "[ -e $dest/$f ] && cp -p $dest/$f $dest/$f.bak-$stamp || true"
  scp -q "$here/$f" "$host:g2-bridge/$f"
done
scp -q "$here/hermes-jinx-g2.service" "$host:.config/systemd/user/hermes-jinx-g2.service"
ssh -o BatchMode=yes "$host" 'systemctl --user daemon-reload && systemctl --user enable --now hermes-jinx-g2.service && systemctl --user restart hermes-jinx-g2.service; sleep 2; systemctl --user is-active hermes-jinx-g2.service'

echo "== comprobaciones =="
ssh -o BatchMode=yes "$host" 'curl -s -m 5 http://127.0.0.1:8898/health; echo; ls -l ~/.config/jinx-g2/key'
cat <<MSG
Falta (una vez, a mano):
  1. HTTPS para las gafas:   ssh $host 'sudo tailscale serve --bg --https=8443 http://127.0.0.1:8898'
     (la dirección que sale, p. ej. https://server.tu-red.ts.net:8443, es JINX_G2_ORIGIN al empaquetar la app)
  2. La llave:               ssh $host 'cat ~/.config/jinx-g2/key'   (se escribe en el teléfono, nunca en el repo)
  3. Voz:                    pip install faster-whisper en el servidor, o JINX_G2_STT_CMD en la unidad.
MSG
