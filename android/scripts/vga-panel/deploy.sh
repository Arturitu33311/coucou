#!/usr/bin/env bash
# scripts/vga-panel/deploy.sh — despliega el panel VGA (Mochi grande, estilo Emerald Glass, sin Zynara) en el server.
# Copia de seguridad de cada archivo reemplazado (<archivo>.bak-<fecha>-mochi3) y reinicio de las dos unidades.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="${JINX_SERVER:-server}"
dest="/home/alan/.hermes/vga_panel"
stamp="$(date +%Y%m%d-%H%M%S)"

python3 "$here/build.py"
python3 -m unittest discover -s "$here/server" -p 'test_*.py' >/dev/null
node --test "$here/test_mochi_logic.js" >/dev/null

for f in panel.html panel_server.py mochi_presence.py; do
  ssh -o BatchMode=yes "$host" "[ -e '$dest/$f' ] && cp -p '$dest/$f' '$dest/$f.bak-$stamp-mochi3' || true"
done
scp -q "$here/dist/panel.html" "$host:$dest/panel.html"
scp -q "$here/server/panel_server.py" "$host:$dest/panel_server.py"
scp -q "$here/server/mochi_presence.py" "$host:$dest/mochi_presence.py"

ssh -o BatchMode=yes "$host" 'systemctl --user restart hermes-mochi-presence.service hermes-vga-panel.service; sleep 3;
  systemctl --user is-active hermes-mochi-presence.service hermes-vga-panel.service'
echo "== comprobaciones =="
ssh -o BatchMode=yes "$host" 'curl -s -m 20 http://127.0.0.1:8896/api/status | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(\"claves:\", sorted(d))
print(\"sin zynara:\", \"zynara\" not in json.dumps(d).lower())
"; curl -s -m 5 http://127.0.0.1:8897/api/presence/state | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(\"presencia: dispositivos\", d.get(\"devices\"), \"activo\", d.get(\"active_device\"))
"'
echo "ok: panel desplegado en $host:$dest (respaldos .bak-$stamp-mochi3)"
