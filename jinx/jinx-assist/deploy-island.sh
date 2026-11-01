#!/usr/bin/env bash
# jinx/jinx-assist/deploy-island.sh — le da a Jinx la herramienta de la isla en el SERVIDOR (donde corre Hermes) y
# pone al día el servidor de presencia (que guarda las órdenes). Copia de seguridad de lo que reemplaza.
# Lo corre una persona desde el portátil; desde la nube no hay acceso al servidor.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="${JINX_SERVER:-server}"
stamp="$(date +%Y%m%d-%H%M%S)"
skill='/home/alan/.hermes/skills/devops/coucou-island'

python3 -m unittest discover -s "$here" -p 'test_island*.py' >/dev/null
python3 -m unittest discover -s "$here/../vga-panel/server" >/dev/null

# 1. La herramienta y su skill (Hermes los carga de ~/.hermes/skills).
ssh -o BatchMode=yes "$host" "mkdir -p '$skill'"
for f in jinx_island.py island_commands.py; do
  ssh -o BatchMode=yes "$host" "[ -e '$skill/$f' ] && cp -p '$skill/$f' '$skill/$f.bak-$stamp' || true"
  scp -q "$here/$f" "$host:$skill/$f"
done
ssh -o BatchMode=yes "$host" "[ -e '$skill/SKILL.md' ] && cp -p '$skill/SKILL.md' '$skill/SKILL.md.bak-$stamp' || true"
scp -q "$here/island/SKILL.md" "$host:$skill/SKILL.md"
ssh -o BatchMode=yes "$host" "chmod 0755 '$skill/jinx_island.py'"

# 2. El servidor de presencia con la cola de órdenes (reinicia su unidad).
bash "$here/../vga-panel/deploy.sh"

# Hermes detecta los SKILL.md solos (firma de archivos): no hace falta reindexar. Si Jinx no ve el skill, en su chat
# se recarga con /reload-skills (o se reinicia el gateway de Hermes). Aquí solo se comprueba que el archivo está.
ssh -o BatchMode=yes "$host" "head -3 '$skill/SKILL.md'" | sed 's/^/   skill: /'

echo "== comprobación: una orden sin PC conectado debe decirlo, no colgarse =="
ssh -o BatchMode=yes "$host" "python3 '$skill/jinx_island.py' state || true"
echo "ok: herramienta de la isla en $host:$skill (respaldos .bak-$stamp)"
echo "    Falta: Coucou del PC con esta versión, y en Ajustes → Jinx y Hub las apps que Jinx puede cerrar."
