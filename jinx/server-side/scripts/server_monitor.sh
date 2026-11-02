#!/usr/bin/env bash
# Hermes server health monitor — runs every 15 minutes via crontab.
# Checks: hermes-gateway (systemd), Docker containers, disk usage, getzynara.com.
# Sends Telegram alert if anything is wrong. Silent on all-green.

set -euo pipefail

LOG="/home/alan/.hermes/logs/server_monitor.log"
ALERT_STATE_DIR="/home/alan/.hermes/logs/monitor_state"
DISK_THRESHOLD=85

mkdir -p "$ALERT_STATE_DIR"

_send_telegram() {
    # Un solo emisor (notify_alan.sh): token fuera de argv, IP de Telegram fijada en los reintentos.
    # MOCHI_ROUTE=0: una alerta del servidor va directa a Telegram, no al mochi.
    MOCHI_ROUTE=0 NOTIFY_PARSE_MODE=HTML /home/alan/.hermes/scripts/notify_alan.sh "$1" > /dev/null 2>&1 || true
}

# Voz de Jinx: una frase distinta cada vez (sin red ni LLM: esta alerta debe
# salir aunque el gateway esté caído). Si el helper falla, la primera opción.
_voz() { python3 /home/alan/.hermes/scripts/voz_jinx.py variar "$@" 2>/dev/null || echo "$2"; }

_alert_flag() { echo "${ALERT_STATE_DIR}/${1}.alerted"; }

_alert() {
    local key="$1"
    local msg="$2"
    local flag
    flag="$(_alert_flag "$key")"
    # Only alert once per failure; clear flag on recovery
    if [[ ! -f "$flag" ]]; then
        touch "$flag"
        echo "[$(date -Iseconds)] ALERT [$key]: $msg" >> "$LOG"
        _send_telegram "$(_voz mon_alert "🔴 Se me cayó algo en la casa:" "🔴 Oye, algo se rompió:" "🔴 Tenemos un problema:" "🔴 Esto no me gustó nada:" "🔴 Ojo, falla en el servidor:")
$msg"
    fi
}

_recover() {
    local key="$1"
    local flag
    flag="$(_alert_flag "$key")"
    if [[ -f "$flag" ]]; then
        rm -f "$flag"
        echo "[$(date -Iseconds)] RECOVERED [$key]" >> "$LOG"
        _send_telegram "$(_voz mon_ok "✅ [$key] ya volvió." "✅ Ya quedó [$key], todo en orden." "✅ [$key] de vuelta." "✅ Ya se compuso [$key]. Sin drama." "✅ [$key] recuperado.")"
    fi
}

# ── 1. hermes-gateway (systemd) ──────────────────────────────────────────────
if systemctl is-active --quiet hermes-gateway 2>/dev/null; then
    _recover "hermes-gateway"
else
    _alert "hermes-gateway" "[hermes-gateway] Servicio systemd caído o inactivo."
fi

# ── 2. Docker containers ──────────────────────────────────────────────────────
for container in zynara-gateway zynara-webhook zynara_site; do
    status=$(docker inspect --format '{{.State.Status}}' "$container" 2>/dev/null || echo "missing")
    if [[ "$status" == "running" ]]; then
        _recover "docker-${container}"
    else
        _alert "docker-${container}" "[${container}] Contenedor Docker: estado '${status}'."
    fi
done

# ── 3. getzynara.com HTTP check ──────────────────────────────────────────────
http_code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "https://getzynara.com" 2>/dev/null || echo "000")
if [[ "$http_code" == "200" || "$http_code" == "301" || "$http_code" == "302" || "$http_code" == "304" ]]; then
    _recover "getzynara-http"
else
    _alert "getzynara-http" "[getzynara.com] No responde. HTTP ${http_code} (timeout 5s)."
fi

# ── 4. Disk usage ─────────────────────────────────────────────────────────────
disk_pct=$(df / --output=pcent | tail -1 | tr -d ' %')
if (( disk_pct >= DISK_THRESHOLD )); then
    _alert "disk-root" "[Disco /] Uso al ${disk_pct}% (umbral: ${DISK_THRESHOLD}%). Intervención requerida."
else
    _recover "disk-root"
fi

echo "[$(date -Iseconds)] monitor OK — gateway=$(systemctl is-active hermes-gateway 2>/dev/null || echo unknown) disk=${disk_pct}% http=${http_code}" >> "$LOG"
