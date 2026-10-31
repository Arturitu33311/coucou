#!/usr/bin/env bash
# Install the Coucou Linux wallpaper in one go:
#   1. copies server.py + wallpaper.html to ~/.local/share/coucou-wallpaper/app
#   2. installs and starts the user service (and enables it at login)
#   3. switches Hidamari's DAILY profile to the web page (a dated backup is kept)
#   4. checks that everything answers
# Safe to run again. To undo: ./install.sh --uninstall
# Options: --no-lyrics   do not look up synced lyrics on lrclib.net (everything stays local)
#          --no-profile  leave Hidamari's profiles alone (only install and start the service)
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$HOME/.local/share/coucou-wallpaper/app"
UNIT_DIR="$HOME/.config/systemd/user"
UNIT="$UNIT_DIR/coucou-wallpaper.service"
HID="$HOME/.var/app/io.github.jeffshee.Hidamari/config/hidamari"
PORT=8765
URL="http://127.0.0.1:$PORT/"
LYRICS=1
PROFILE=1
STAMP="$(date +%Y%m%d-%H%M%S)"

ok()   { printf '  \033[32mOK\033[0m  %s\n' "$*"; }
fail() { printf '  \033[31mFALLO\033[0m %s\n' "$*"; FAILED=1; }
FAILED=0

set_profile_web() {   # $1 = config file: MODE_WEBPAGE with $URL on every screen
  python3 -I - "$1" "$URL" <<'PY'
import json, sys
path, url = sys.argv[1], sys.argv[2]
j = json.load(open(path, encoding="utf-8"))
j["mode"] = "MODE_WEBPAGE"
for k in j["data_source"]:
    j["data_source"][k] = url
json.dump(j, open(path, "w", encoding="utf-8"), indent=3, ensure_ascii=False)
PY
}

for a in "$@"; do
  case "$a" in
    --no-lyrics) LYRICS=0 ;;
    --no-profile) PROFILE=0 ;;
    --uninstall)
      echo "Desinstalando…"
      systemctl --user disable --now coucou-wallpaper 2>/dev/null
      rm -f "$UNIT"; systemctl --user daemon-reload
      # the OLDEST backup is the original (later ones, if any, were made from an already-switched file)
      oldest="$(ls -1tr "$HID"/config-daily.json.bak-coucou-* 2>/dev/null | head -1)"
      if [ -n "$oldest" ]; then cp "$oldest" "$HID/config-daily.json"; echo "  config-daily.json restaurado desde $oldest"; fi
      echo "Listo. (Los archivos de $APP se dejan; borra esa carpeta si quieres.)"
      exit 0 ;;
    *) echo "Opción desconocida: $a"; exit 2 ;;
  esac
done

echo "1/4 Copiando archivos a $APP"
mkdir -p "$APP" "$UNIT_DIR"
cp "$HERE/server.py" "$HERE/wallpaper.html" "$APP/" && ok "server.py y wallpaper.html" || fail "no se pudieron copiar"
[ -f "$HOME/.local/share/coucou-wallpaper/car/car_cut.png" ] && ok "imagen del auto encontrada" \
  || echo "  (sin auto en ~/.local/share/coucou-wallpaper/car/car_cut.png: se usará el diseño sin auto)"

echo "2/4 Instalando el servicio"
EXTRA=""; [ "$LYRICS" = 1 ] && EXTRA=" --lyrics"
cat > "$UNIT" <<EOF
[Unit]
Description=Coucou Linux wallpaper (local server for the animated wallpaper page shown by Hidamari)

[Service]
ExecStart=/usr/bin/python3 %h/.local/share/coucou-wallpaper/app/server.py --port $PORT$EXTRA
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
EOF
# a server started by hand (not by the service) would hold the port: stop it
if ! systemctl --user is-active --quiet coucou-wallpaper; then
  pkill -f "coucou-wallpaper/app/server.py --port $PORT" 2>/dev/null && sleep 1
fi
systemctl --user daemon-reload
systemctl --user enable coucou-wallpaper >/dev/null 2>&1
systemctl --user restart coucou-wallpaper
sleep 2
systemctl --user is-active --quiet coucou-wallpaper && ok "servicio activo y habilitado al iniciar sesión" || fail "el servicio no arrancó (mira: journalctl --user -u coucou-wallpaper)"

echo "3/4 Perfil de Hidamari"
if [ "$PROFILE" = 1 ]; then
  if [ -f "$HID/config-daily.json" ]; then
    if grep -q '"MODE_WEBPAGE"' "$HID/config-daily.json"; then
      ok "config-daily.json ya apunta a la página web (no se hace otra copia: la del original se conserva)"
    else
      cp "$HID/config-daily.json" "$HID/config-daily.json.bak-coucou-$STAMP"
      set_profile_web "$HID/config-daily.json" && ok "config-daily.json -> página web (copia: config-daily.json.bak-coucou-$STAMP)" || fail "no se pudo editar config-daily.json"
    fi
    [ -f "$HID/config.json" ] && set_profile_web "$HID/config.json" && ok "config.json (la sesión actual) también"
  else
    fail "no encuentro $HID/config-daily.json"
  fi
  echo "     (work y school se dejan con su video; el perfil game usa el de daily)"
else
  echo "  omitido (--no-profile)"
fi

echo "4/4 Verificando"
page="$(curl -s -m 5 -o /dev/null -w '%{http_code}' "$URL")"
[ "$page" = 200 ] && ok "la página responde en $URL" || fail "la página no responde (código $page)"
tok="$(curl -s -m 5 "$URL" | grep -o "TOKEN = *'[^']*'" | cut -d"'" -f2)"
if [ -n "$tok" ]; then
  code="$(curl -s -m 8 -o /dev/null -w '%{http_code}' "${URL}state?k=$tok")"
  [ "$code" = 200 ] && ok "el servidor lee el reproductor, la batería y la red" || fail "/state respondió $code"
  [ "$(curl -s -m 5 -o /dev/null -w '%{http_code}' "${URL}state")" = 403 ] && ok "sin la clave el servidor rechaza (403)" || fail "debería rechazar sin clave"
else
  fail "la página no trae su clave"
fi
if grep -q "cursor: none" "$APP/wallpaper.html"; then fail "la página oculta el cursor"; else ok "el cursor sigue siendo el del escritorio"; fi
grep -q "/control" "$APP/server.py" && grep -q "function control" "$APP/wallpaper.html" && ok "controles de reproducción presentes" || fail "faltan los controles"
if [ -n "${tok:-}" ]; then
  [ "$(curl -s -m 5 -X POST -o /dev/null -w '%{http_code}' "${URL}control?a=next")" = 403 ] && ok "los controles rechazan sin clave (403)" || fail "los controles deberían rechazar sin clave"
fi

echo
if [ "$FAILED" = 0 ]; then
  echo "Todo bien. Para ver el fondo ahora (sin cerrar sesión):"
  echo "  flatpak kill io.github.jeffshee.Hidamari; flatpak run io.github.jeffshee.Hidamari -b &"
  echo "En el próximo inicio de sesión (perfil daily) se aplica solo."
else
  echo "Hubo fallos (arriba). El servicio y los archivos pueden estar a medias: puedes volver a ejecutar el script."
  exit 1
fi
