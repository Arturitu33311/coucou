#!/usr/bin/env bash
# jinx/server-side/catos/deploy.sh — publica Cat OS en el server con respaldo y reversión automática si no responde.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="${JINX_SERVER:-server}"
stamp="$(date +%Y%m%d-%H%M%S)"
python3 -m unittest discover -s "$here" -p 'test_*.py' >/dev/null || { echo "pruebas fallan: no se publica"; exit 1; }
scp -q "$here/catos_server.py" "$host:/tmp/catos_server_new.py"
scp -q "$here/dashboard.html" "$host:/tmp/catos_dash_new.html"
ssh -o BatchMode=yes "$host" bash -s -- "$stamp" <<'REMOTE'
set -u
S="$1"; D="$HOME/.hermes/catos"
cp -p "$D/catos_server.py" "$D/catos_server.py.bak-$S-seguridad"
cp -p "$D/dashboard.html" "$D/dashboard.html.bak-$S-seguridad"
cp /tmp/catos_server_new.py "$D/catos_server.py"; cp /tmp/catos_dash_new.html "$D/dashboard.html"
systemctl --user restart hermes-catos.service; sleep 4
ok=$(curl -s -m 60 -o /dev/null -w "%{http_code}" http://100.92.254.125:8899/api/status)
bad=$(curl -s -m 10 -o /dev/null -w "%{http_code}" -H "Host: evil.example" http://100.92.254.125:8899/api/status)
post=$(curl -s -m 10 -o /dev/null -w "%{http_code}" -X POST -H "Origin: https://evil.example" -H "Content-Type: text/plain" -d '{"mode":"normal"}' http://100.92.254.125:8899/api/busy)
echo "status=$ok host-ajeno=$bad post-otro-origen=$post"
if [ "$ok" = "200" ] && [ "$bad" = "421" ] && [ "$post" = "403" ]; then echo OK
else
  echo "FALLO: se restaura el respaldo"
  cp -p "$D/catos_server.py.bak-$S-seguridad" "$D/catos_server.py"; cp -p "$D/dashboard.html.bak-$S-seguridad" "$D/dashboard.html"
  systemctl --user restart hermes-catos.service
fi
rm -f /tmp/catos_server_new.py /tmp/catos_dash_new.html
REMOTE
