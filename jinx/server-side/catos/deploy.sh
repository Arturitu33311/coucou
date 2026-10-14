#!/usr/bin/env bash
# jinx/server-side/catos/deploy.sh — publica Cat OS en el server con respaldo y reversión automática si no responde.
# Los archivos se dejan en un directorio privado (mktemp -d, 0700) del propio usuario, no en /tmp con nombre fijo.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="${JINX_SERVER:-server}"
stamp="$(date +%Y%m%d-%H%M%S)"
python3 -m unittest discover -s "$here" -p 'test_*.py' >/dev/null || { echo "pruebas fallan: no se publica"; exit 1; }

stage="$(ssh -o BatchMode=yes "$host" 'mkdir -p "$HOME/.cache" && mktemp -d "$HOME/.cache/catos-deploy.XXXXXXXX"')" \
  || { echo "no pude crear el directorio de staging"; exit 1; }
case "$stage" in /home/*/.cache/catos-deploy.*) ;; *) echo "staging inesperado: $stage"; exit 1;; esac

scp -q "$here/catos_server.py" "$host:$stage/catos_server.py"
scp -q "$here/dashboard.html" "$host:$stage/dashboard.html"
ssh -o BatchMode=yes "$host" bash -s -- "$stamp" "$stage" <<'REMOTE'
set -u
S="$1"; ST="$2"; D="$HOME/.hermes/catos"
trap 'rm -rf "$ST"' EXIT
cp -p "$D/catos_server.py" "$D/catos_server.py.bak-$S-seguridad"
cp -p "$D/dashboard.html" "$D/dashboard.html.bak-$S-seguridad"
cp "$ST/catos_server.py" "$D/catos_server.py"; cp "$ST/dashboard.html" "$D/dashboard.html"
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
REMOTE
