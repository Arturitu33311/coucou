#!/usr/bin/env bash
# apply.sh — instala el experimento del motor emocional en ~/.hermes. EJECUTAR EN EL SERVIDOR.
#
#   scp -r jinx/hermes-experiment server:/tmp/hermes-experiment && ssh server 'bash /tmp/hermes-experiment/apply.sh'
#
# Hace, en este orden (y se detiene en el primer fallo):
#   1. comprueba que el motor vivo coincide con lo que los parches esperan (--check, sin tocar nada);
#   2. respalda con fecha plugins/persona-engine/{emotion_calculator,emotion_state_manager}.py,
#      scripts/zynara_worry.py, STATE.md y SOUL.md en ~/.hermes/backups_experimento_<fecha>/;
#   3. parchea, instala persona_experiment.py y el disparador, escribe ~/.hermes/persona_experiment.json;
#   4. activa el timer del disparador (systemd --user, cada 15 min).
# NO reinicia el gateway: el motor ya cargado no ve el código nuevo hasta que se haga (ver el final).
# Deshacer: bash ~/.hermes/scripts/persona_experiment_revert.sh [--restore-state] [--reload]
# (PX_NO_SYSTEMD=1 omite el timer: para ensayar con un HERMES_HOME temporal)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
H=${HERMES_HOME:-$HOME/.hermes}
PE=$H/plugins/persona-engine
PY=$H/hermes-agent/venv/bin/python
stamp=$(date +%Y%m%d-%H%M%S)
B=$H/backups_experimento_$stamp

python3 "$here/apply_patches.py" --home "$H" --check

if grep -q persona_experiment "$PE/emotion_calculator.py" 2>/dev/null; then
  echo "parches ya aplicados: solo actualizo persona_experiment.py y el disparador"
else
  mkdir -p "$B/plugins/persona-engine" "$B/scripts"
  cp -a "$PE/emotion_calculator.py" "$PE/emotion_state_manager.py" "$B/plugins/persona-engine/"
  cp -a "$H/scripts/zynara_worry.py" "$B/scripts/"
  cp -a "$H/STATE.md" "$H/SOUL.md" "$B/"
  echo "$B" > "$H/persona_experiment_backup_dir"
  echo "respaldo en $B"
  python3 "$here/apply_patches.py" --home "$H"
fi

install -m 644 "$here/persona_experiment.py" "$PE/persona_experiment.py"
install -m 755 "$here/persona_experiment_tripwire.py" "$H/scripts/persona_experiment_tripwire.py"
install -m 755 "$here/revert.sh" "$H/scripts/persona_experiment_revert.sh"

if [ ! -f "$H/persona_experiment.json" ]; then
  PYTHONPATH="$PE" python3 - "$H/persona_experiment.json" <<'EOF'
import json, sys
from datetime import datetime, timedelta
import persona_experiment as px
cfg = dict(px.DEFAULTS, enabled=True, started_at=datetime.now().isoformat(timespec="seconds"),
           review_at=(datetime.now() + timedelta(days=7)).date().isoformat())
json.dump(cfg, open(sys.argv[1], "w"), indent=1, ensure_ascii=False)
print("flag escrito:", sys.argv[1])
EOF
fi

if [ -z "${PX_NO_SYSTEMD:-}" ]; then
U=$HOME/.config/systemd/user
mkdir -p "$U"
cat > "$U/persona-experiment-tripwire.service" <<EOF
[Unit]
Description=Vigila el experimento del motor emocional de Hermes y lo apaga si algo se sale de rango

[Service]
Type=oneshot
Environment=HERMES_HOME=$H
ExecStart=$PY $H/scripts/persona_experiment_tripwire.py
TimeoutStartSec=60
EOF
cat > "$U/persona-experiment-tripwire.timer" <<EOF
[Unit]
Description=Cada 15 min revisa los rangos del experimento emocional

[Timer]
OnBootSec=2min
OnUnitActiveSec=15min
AccuracySec=30s

[Install]
WantedBy=timers.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now persona-experiment-tripwire.timer
fi

echo
echo "instalado. Falta (cuando no haya un turno en curso) cargar el código nuevo en el gateway:"
echo "  kill -USR1 \$(systemctl show -p MainPID --value hermes-gateway)"
echo "Apagar al instante:  python3 -c \"import json;p='$H/persona_experiment.json';c=json.load(open(p));c['enabled']=False;json.dump(c,open(p,'w'),indent=1)\""
