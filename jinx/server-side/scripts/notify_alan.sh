#!/bin/bash
# notify_alan.sh — única pieza de entrega de alertas proactivas a Alan.
#
# Reemplaza al endpoint muerto http://localhost:8000/api/messages/send, que
# NUNCA existió en este Hermes: el api_server sólo expone /v1/* y /api/sessions/*
# (ver gateway/platforms/api_server.py). Los canales que lo llamaban llevaban
# meses fallando en silencio.
#
# Usa el mismo camino probado de noche_guardian.send_telegram: Telegram Bot API
# directo, con el token desde VaultKnox (nunca hardcodeado). No depende del
# gateway, así que sigue funcionando aunque hermes-gateway esté caído — que es
# justo cuando más importa que una alerta llegue.
#
# Uso:   notify_alan.sh "texto del mensaje"
#        echo "texto" | notify_alan.sh
# Env:   NOTIFY_PARSE_MODE=Markdown   (por defecto: texto plano)
# Salida: imprime el HTTP code en stdout. Sale 0 sólo si Telegram confirmó ok.

set -uo pipefail

CHAT_ID="${NOTIFY_CHAT_ID:-6528281557}"
SECRETS="/home/alan/.hermes/hermes-agent/venv/bin/hermes-secrets"

MSG="${1:-}"
[ -z "$MSG" ] && MSG="$(cat)"
if [ -z "$MSG" ]; then
    echo "000" ; echo "notify_alan: mensaje vacío" >&2 ; exit 2
fi

TOKEN="$("$SECRETS" get TELEGRAM_BOT_TOKEN 2>/dev/null)"

# MOCHI-FIRST: al mochi activo (PC o panel) en vez de Telegram; Telegram
# queda como fallback si el mochi falla. Solo texto, sin voz.
if [ "${MOCHI_ROUTE:-1}" = 1 ]; then
  KIND="info"
  case "$MSG" in *⚠️*|*❌*|*🚨*|*⚠*) KIND="warn" ;; esac
  if OUT="$(printf "%s" "$MSG" | timeout 15 /home/alan/.hermes/scripts/mochi_route.py --kind "$KIND" --source notify 2>/dev/null)"; then
    echo "200 (mochi:$OUT)"
    exit 0
  fi
  echo "notify_alan: mochi falló, caigo a Telegram" >&2
fi
if [ -z "$TOKEN" ]; then
    echo "000" ; echo "notify_alan: no pude leer TELEGRAM_BOT_TOKEN de VaultKnox" >&2 ; exit 3
fi

# El payload se arma con python para escapar el texto correctamente: los
# mensajes traen saltos de línea, comillas y emojis que rompen un JSON a mano.
PAYLOAD=$(MSG="$MSG" CHAT_ID="$CHAT_ID" PM="${NOTIFY_PARSE_MODE:-}" python3 -c '
import json, os
p = {"chat_id": int(os.environ["CHAT_ID"]), "text": os.environ["MSG"]}
if os.environ.get("PM"):
    p["parse_mode"] = os.environ["PM"]
print(json.dumps(p))
')

BODY_FILE="$(mktemp)"
trap 'rm -f "$BODY_FILE"' EXIT

# REINTENTOS — no son decorativos. Medido en este server el 2026-07-22:
# api.telegram.org sólo resuelve/responde por IPv6 (IPv4 falla el 100 % de las
# veces), y ese IPv6 se cae de forma intermitente: 2 de cada 10 peticiones
# devuelven 000 (conexión abortada). Sin reintentos, 1 de cada 5 alertas
# proactivas se perdía — y las que más importan (guardián nocturno, saldo
# agotado) no admiten esa tasa de pérdida.
# Se reintenta sólo lo que puede mejorar solo: fallo de conexión (000), 5xx y
# 429. Un 4xx es un mensaje mal formado y reintentarlo daría igual.
INTENTOS=3
HTTP=000
usados=0

for intento in $(seq 1 "$INTENTOS"); do
    usados=$intento
    # Desde el 2.o intento se fija la IP v4 que responde (.99): la IP del DNS y .220 estan en
    # blackhole en esta red (2026-10-14, ver hermes_telegram_retry.py); la ruta normal daba timeout.
    PIN=()
    [ "$intento" -ge 2 ] && PIN=(--resolve api.telegram.org:443:149.154.167.99)
    # URL (lleva el token) y cuerpo por stdin (-K -): no aparecen en `ps`
    HTTP=$(printf 'url = "https://api.telegram.org/bot%s/sendMessage"\n' "$TOKEN" |
        curl -s --max-time 20 "${PIN[@]}" -o "$BODY_FILE" -w '%{http_code}' \
        -K - -X POST -H "Content-Type: application/json" --data-binary "$PAYLOAD")

    case "$HTTP" in
        000|429|5??)
            if [ "$intento" -lt "$INTENTOS" ]; then
                echo "notify_alan: intento $intento devolvió $HTTP; reintento" >&2
                sleep $((intento * 2))
                continue
            fi
            ;;
    esac
    break
done

# Telegram puede devolver 200 con ok:false; el HTTP code solo no basta.
OK=$(python3 -c '
import json, sys
try:
    print("1" if json.load(open(sys.argv[1])).get("ok") else "0")
except Exception:
    print("0")
' "$BODY_FILE")

echo "$HTTP"
if [ "$OK" = "1" ]; then
    exit 0
fi
echo "notify_alan: falló tras $usados intento(s) (HTTP $HTTP): $(head -c 300 "$BODY_FILE")" >&2
exit 1
