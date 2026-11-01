#!/usr/bin/env python3
"""apply_patches — engancha persona_experiment al motor de Hermes con 5 cambios mínimos y guardados.

    python3 apply_patches.py --home ~/.hermes            # aplica (idempotente)
    python3 apply_patches.py --home ~/.hermes --check    # dice si ya está aplicado, sin tocar nada

Cada reemplazo exige que el texto original aparezca exactamente una vez; si el motor cambió (una
actualización de Hermes), PARA sin tocar nada y lo dice. Cada gancho vive dentro de `if _px is not None`
o de un try/except: un fallo del experimento nunca rompe el motor.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

MARK = "persona_experiment"

IMPORT_BLOCK = (
    "try:  # persona_experiment (experimento 2026-10-14): apagado o ausente = motor original\n"
    "    import persona_experiment as _px\n"
    "except Exception:\n"
    "    _px = None\n"
)

CALC = ("plugins/persona-engine/emotion_calculator.py", [
    ("from typing import Dict, List, Optional, Tuple\n",
     "from typing import Dict, List, Optional, Tuple\n" + IMPORT_BLOCK),
    ("            factor = min(1.0, rate * hours)\n"
     "            new_value = current_value + (baseline - current_value) * factor\n",
     "            factor = min(1.0, rate * hours)\n"
     "            new_value = current_value + (baseline - current_value) * factor\n"
     "            if _px is not None:  # persona_experiment: exponencial; apagado devuelve new_value tal cual\n"
     "                new_value = _px.decay_value(self._decay_rate, current_value, baseline, hours, new_value)\n"),
])

MGR = ("plugins/persona-engine/emotion_state_manager.py", [
    ("import json\n", "import json\n" + IMPORT_BLOCK),
    ("            event = force_event or self.detector.detect_emotion_event(messages)\n",
     "            event = force_event or self.detector.detect_emotion_event(messages)\n"
     "            _automated = False\n"
     "            if _px is not None and not force_event and event is not None and _px.is_automated(messages):\n"
     "                event, _automated = None, True  # persona_experiment: [Home Assistant]… no es habla de Alan\n"),
    ("            if not force_event:\n                user_msgs = [m for m in messages",
     "            if not force_event and not _automated:\n                user_msgs = [m for m in messages"),
    ("            neutral = event.trigger_type == \"normal_interaction\"\n",
     "            neutral = event.trigger_type == \"normal_interaction\"\n"
     "            if _px is not None:  # persona_experiment: deterministic = neutral; ganancia con topes\n"
     "                neutral, event = _px.shape_event(event, neutral, current_state, self.calculator.baselines)\n"),
    ("        result = \"\\n\" + \"\\n\".join(blocks) + \"\\n\"\n",
     "        result = \"\\n\" + \"\\n\".join(blocks) + \"\\n\"\n"
     "        if _px is not None:  # persona_experiment: línea de matiz (apagado = result intacto)\n"
     "            try:\n"
     "                result = _px.add_nuance(\n"
     "                    result, self._read_state()[\"frontmatter\"].get(\"emotion_state\", {}),\n"
     "                    self.calculator.baselines)\n"
     "            except Exception:\n"
     "                pass\n"),
])

ZW = ("scripts/zynara_worry.py", [
    ("            pendientes = re.findall(r'- \\[ \\]', texto)\n"
     "            if len(pendientes) >= PRESION_MIN_TAREAS:\n",
     "            pendientes = re.findall(r'- \\[ \\]', texto)\n"
     "            try:  # persona_experiment: solo tareas que vencen en <= PRESION_DIAS y una vez por día\n"
     "                sys.path.insert(0, str(HERMES_HOME / \"plugins\" / \"persona-engine\"))\n"
     "                import persona_experiment as _px\n"
     "                pendientes = _px.presion_tasks(texto, pendientes, PRESION_DIAS)\n"
     "                _claim = lambda: _px.presion_claim(dia_presupuesto(), \"--dry-run\" not in sys.argv)  # noqa: E731\n"
     "            except Exception:\n"
     "                _claim = lambda: True  # noqa: E731\n"
     "            if len(pendientes) >= PRESION_MIN_TAREAS and _claim():\n"),
])

TARGETS = [CALC, MGR, ZW]


def plan(home: Path):
    """[(path, new_text)] o lanza SystemExit con el motivo. No escribe."""
    out = []
    for rel, edits in TARGETS:
        path = home / rel
        if not path.exists():
            raise SystemExit(f"falta {path}")
        text = path.read_text(encoding="utf-8")
        if MARK in text:
            out.append((path, None))  # ya aplicado
            continue
        for old, new in edits:
            n = text.count(old)
            if n != 1:
                raise SystemExit(f"{rel}: el texto esperado aparece {n} veces (debe ser 1); el motor cambió:\n{old[:200]}")
            text = text.replace(old, new, 1)
        out.append((path, text))
    return out


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--home", default=str(Path.home() / ".hermes"))
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args(argv)
    todo = plan(Path(a.home))
    for path, text in todo:
        print(f"{'ya aplicado' if text is None else 'a parchear'}: {path}")
    if a.check:
        return 0
    for path, text in todo:
        if text is not None:
            path.write_text(text, encoding="utf-8")
    print("listo")
    return 0


if __name__ == "__main__":
    sys.exit(main())
