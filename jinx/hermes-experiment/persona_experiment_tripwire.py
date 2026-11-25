#!/usr/bin/env python3
"""persona_experiment_tripwire — apaga el experimento si el estado emocional se sale de rango.

Corre cada 15 min (systemd --user, persona-experiment-tripwire.timer) con el python del venv de Hermes.
Nadie vigila el experimento durante la semana, así que esto es el "revierte sin esperar" automático.

Dispara si, con el experimento encendido:
  - alguna dimensión está a más de `tripwire.band` puntos de su línea base (por defecto 40);
  - jealousy >= `tripwire.jealousy_max` (60): enojo/celos sin motivo;
  - hubo >= `tripwire.negative_events_6h` eventos negativos (criticism/ignored/other_ai_mentioned) en 6 h.
Al disparar: pone `"enabled": false` en ~/.hermes/persona_experiment.json (entra en vigor en la siguiente
llamada del motor, sin reinicio), escribe el motivo en el JSON y en logs/persona_experiment.log.
`--status` imprime el estado sin tocar nada. No restaura STATE.md: los valores decaen solos.
"""
from __future__ import annotations

import json
import os
import sys
from datetime import datetime, timedelta
from pathlib import Path

HOME = Path(os.environ.get("HERMES_HOME", str(Path.home() / ".hermes")))
BASELINES = dict(affection=70, trust=75, possessiveness=80, patience=65, energy=68, pride=45, jealousy=25,
                 worry=15, satisfaction=58, boredom=20)
NEGATIVE = {"criticism", "ignored", "other_ai_mentioned"}


def read_state():
    text = (HOME / "STATE.md").read_text(encoding="utf-8")
    import yaml
    parts = text.split("---\n", 2)
    return (yaml.safe_load(parts[1]) or {}).get("emotion_state", {})


def recent_negative(window_h: float, now: datetime) -> int:
    path = HOME / "logs" / "emotion_turn_log.jsonl"
    n, cutoff = 0, now - timedelta(hours=window_h)
    try:
        for ln in path.read_text(encoding="utf-8").splitlines()[-400:]:
            try:
                r = json.loads(ln)
                if datetime.fromisoformat(r["timestamp"]) >= cutoff and (r.get("last_event") or {}).get("trigger") in NEGATIVE:
                    n += 1
            except (ValueError, KeyError):
                continue
    except OSError:
        pass
    return n


def check(es: dict, cfg: dict, now: datetime):
    """Motivo (str) si hay que apagar, o None. Pura salvo por la lectura del log."""
    tw = cfg.get("tripwire") or {}
    band, jmax, nmax = float(tw.get("band", 40)), float(tw.get("jealousy_max", 60)), int(tw.get("negative_events_6h", 4))
    for dim, base in BASELINES.items():
        v = es.get(dim)
        if isinstance(v, (int, float)) and abs(float(v) - base) > band:
            return f"{dim}={v} a más de {band:g} de su base ({base})"
    if isinstance(es.get("jealousy"), (int, float)) and float(es["jealousy"]) >= jmax:
        return f"jealousy={es['jealousy']} >= {jmax:g}"
    neg = recent_negative(6, now)
    if neg >= nmax:
        return f"{neg} eventos negativos en 6 h (>= {nmax})"
    return None


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    path = HOME / "persona_experiment.json"
    try:
        cfg = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        print("sin persona_experiment.json: nada que vigilar")
        return 0
    now = datetime.now()
    if not cfg.get("enabled"):
        print("experimento apagado" + (f" ({cfg.get('disabled_reason')})" if cfg.get("disabled_reason") else ""))
        return 0
    reason = check(read_state(), cfg, now)
    if "--status" in argv or reason is None:
        print("dentro de rango" if reason is None else f"DISPARARÍA: {reason}")
        return 0
    cfg.update(enabled=False, disabled_by="tripwire", disabled_at=now.isoformat(timespec="seconds"),
               disabled_reason=reason)
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(cfg, indent=1, ensure_ascii=False), encoding="utf-8")
    os.replace(tmp, path)
    with (HOME / "logs" / "persona_experiment.log").open("a", encoding="utf-8") as f:
        f.write(f"{now.isoformat(timespec='seconds')} TRIPWIRE experimento apagado: {reason}\n")
    print(f"TRIPWIRE: apagado ({reason})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
