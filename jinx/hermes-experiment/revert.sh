#!/usr/bin/env bash
# revert.sh — deja el motor emocional de Hermes como estaba antes del experimento. EJECUTAR EN EL SERVIDOR.
#
#   bash ~/.hermes/scripts/persona_experiment_revert.sh [--restore-state] [--reload] [DIR_RESPALDO]
#
# Siempre:   restaura emotion_calculator.py, emotion_state_manager.py y zynara_worry.py del respaldo, quita
#            persona_experiment.py y el disparador, apaga el timer, aparta persona_experiment.json y borra
#            su estado (state/persona_experiment_presion.json).
# --restore-state  además devuelve STATE.md y SOUL.md a como estaban al instalar. OJO: los medidores vuelven a
#            los valores de ese día (una semana viejos); sin esta opción se quedan donde están y decaen solos.
# --reload   manda USR1 al gateway para que suelte el código cargado (si no, basta con el JSON apagado:
#            el motor ya devuelve el comportamiento original sin reiniciar).
# Si no se da DIR_RESPALDO usa el último de ~/.hermes/persona_experiment_backup_dir.
set -euo pipefail
H=${HERMES_HOME:-$HOME/.hermes}
PE=$H/plugins/persona-engine
restore_state=0; reload=0; B=""
for a in "$@"; do
  case "$a" in
    --restore-state) restore_state=1 ;;
    --reload) reload=1 ;;
    *) B=$a ;;
  esac
done
[ -n "$B" ] || B=$(cat "$H/persona_experiment_backup_dir")
[ -f "$B/plugins/persona-engine/emotion_calculator.py" ] || { echo "respaldo no válido: $B" >&2; exit 1; }
stamp=$(date +%Y%m%d-%H%M%S)

# 1. apagar primero: desde aquí el motor ya se comporta como antes aunque algo falle después
if [ -f "$H/persona_experiment.json" ]; then
  python3 - "$H/persona_experiment.json" <<'EOF'
import json, sys
p = sys.argv[1]
try:
    c = json.load(open(p))
except Exception:
    c = {}
c["enabled"] = False
json.dump(c, open(p, "w"), indent=1)
EOF
fi

# 2. código original
cp -a "$B/plugins/persona-engine/emotion_calculator.py" "$B/plugins/persona-engine/emotion_state_manager.py" "$PE/"
cp -a "$B/scripts/zynara_worry.py" "$H/scripts/zynara_worry.py"
rm -f "$PE/persona_experiment.py" "$H/scripts/persona_experiment_tripwire.py" "$H/state/persona_experiment_presion.json"
rm -rf "$PE/__pycache__"

# 3. disparador
if [ -z "${PX_NO_SYSTEMD:-}" ]; then
  systemctl --user disable --now persona-experiment-tripwire.timer 2>/dev/null || true
  rm -f "$HOME/.config/systemd/user/persona-experiment-tripwire.service" "$HOME/.config/systemd/user/persona-experiment-tripwire.timer"
  systemctl --user daemon-reload
fi

# 4. el JSON se aparta (queda como registro de qué se probó)
[ -f "$H/persona_experiment.json" ] && mv "$H/persona_experiment.json" "$H/persona_experiment.json.revertido-$stamp"

if [ "$restore_state" = 1 ]; then
  cp -a "$B/STATE.md" "$H/STATE.md"
  cp -a "$B/SOUL.md" "$H/SOUL.md"
  echo "STATE.md y SOUL.md restaurados del respaldo"
fi
if [ "$reload" = 1 ]; then
  kill -USR1 "$(systemctl show -p MainPID --value hermes-gateway)"
  echo "gateway: USR1 enviado (drena y reinicia)"
fi
echo "revertido desde $B"
mkdir -p "$H/logs"
echo "$(date -Iseconds) REVERTIDO desde $B (restore_state=$restore_state reload=$reload)" >> "$H/logs/persona_experiment.log"
