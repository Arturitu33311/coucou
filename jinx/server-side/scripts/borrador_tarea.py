#!/usr/bin/env python3
"""borrador_tarea.py — deja un BORRADOR de texto para una tarea de Teams y avisa a Alan.

Lo llama tareas_detector_rapido.py cuando aparece una tarea nueva, o a mano:
    python3 borrador_tarea.py <assignment_id> [--force]

Nunca entrega nada (tarea_entregar sigue pidiendo el visto bueno de Alan) y no inventa fuentes: lo que Alan debe
buscar, medir o decidir queda marcado entre corchetes. Si la tarea no pide producir nada (webinar, asistir,
revisar), no hace nada.
"""
from __future__ import annotations

import importlib.util
import json
import os
import re
import subprocess
import sys
import unicodedata
from datetime import datetime
from pathlib import Path

HERMES = Path(os.environ.get("HERMES_HOME", str(Path.home() / ".hermes")))
OUT_DIR = HERMES / "state" / "borradores"
NOTIFY = HERMES / "scripts" / "notify_alan.sh"
MIN_INSTRUCCIONES = 120   # menos que esto casi nunca es un trabajo que redactar
TELEGRAM_CAP = 3800


def _load(path: Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.path.insert(0, str(path.parent))
    spec.loader.exec_module(mod)
    return mod


def _slug(s: str) -> str:
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")[:40] or "tarea"


def _prompt(d: dict) -> str:
    return (
        "Eres un asistente de estudio. Alan es estudiante de preparatoria y necesita un BORRADOR para esta tarea de Teams. "
        "Redáctalo en español, siguiendo las instrucciones al pie de la letra (estructura, número de partes, formato).\n\n"
        f"CLASE: {d.get('clase')}\nTAREA: {d.get('titulo')}\nVENCE: {d.get('vence')}\n\nINSTRUCCIONES:\n{d.get('instrucciones')}\n\n"
        "REGLAS:\n"
        "- Usa SOLO lo que dicen las instrucciones. No inventes fuentes, enlaces, cifras ni citas: donde hagan falta, escribe "
        "[BUSCAR: qué buscar] y Alan lo completa.\n"
        "- Lo que pida libro, archivo adjunto o experiencia personal que no tienes, déjalo como [ALAN: qué falta].\n"
        "- Es un punto de partida para que Alan lo revise y lo haga suyo; no lo presentes como trabajo terminado.\n"
        "- Si la tarea NO pide producir un trabajo (asistir a un webinar, leer, un aviso), responde solo: NO_APLICA\n"
        "- Responde solo con el borrador, sin explicaciones."
    )


MODELOS = ("deepseek/deepseek-v4-flash", "qwen/qwen3-235b-a22b-2507")  # primero el de config.yaml; el 2.o de respaldo


def generar(d: dict) -> str | None:
    """Un modelo directo (OpenRouter, como escuela_scan.py): el agente de Hermes tiene la personalidad de Jinx
    y contesta conversando en vez de redactar."""
    import urllib.error
    import urllib.request
    tt = _load(HERMES / "plugins" / "teams-tareas" / "__init__.py", "teams_tareas")
    key = tt._secret("OPENROUTER_API_KEY") or ""
    if not key:
        print("[borrador] sin OPENROUTER_API_KEY", file=sys.stderr)
        return None
    for modelo in MODELOS:
        body = json.dumps({"model": modelo, "messages": [{"role": "user", "content": _prompt(d)}],
                           "temperature": 0.4, "max_tokens": 6000, "reasoning": {"enabled": False}}).encode()
        req = urllib.request.Request("https://openrouter.ai/api/v1/chat/completions", data=body, method="POST",
                                     headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=150) as r:
                txt = (((json.loads(r.read()).get("choices") or [{}])[0].get("message") or {}).get("content") or "").strip()
            if txt:
                return txt
        except (urllib.error.URLError, OSError, ValueError) as e:
            print(f"[borrador] {modelo}: {str(e)[:100]}", file=sys.stderr)
    return None


def crear(aid: str, force: bool = False, notificar: bool = True) -> str:
    """Devuelve 'ok:<ruta>', 'ya', 'no_aplica' o 'error:<motivo>'."""
    tt = _load(HERMES / "plugins" / "teams-tareas" / "__init__.py", "teams_tareas")
    try:
        d = json.loads(tt.tarea_detalle({"ref": aid}))
    except Exception as e:  # noqa: BLE001
        return f"error:detalle {e}"
    if not d or d.get("error"):
        return f"error:{(d or {}).get('error', 'sin detalle')}"
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    existente = next(OUT_DIR.glob(f"{aid[:8]}-*.md"), None)
    if existente and not force:
        return "ya"
    instr = (d.get("instrucciones") or "").strip()
    if len(instr) < MIN_INSTRUCCIONES or "youtube.com" in instr.lower() or "youtu.be" in instr.lower():
        return "no_aplica"
    texto = generar(d)
    if not texto:
        return "error:el modelo no devolvió texto"
    if texto.strip().upper().startswith("NO_APLICA"):
        return "no_aplica"
    primera, _, resto = texto.partition("\n")
    if primera.lower().startswith(("aquí tienes", "aqui tienes", "claro", "por supuesto")) and resto.strip():
        texto = resto.strip()   # la frase de cortesía del modelo no es parte del borrador
    ruta = OUT_DIR / f"{aid[:8]}-{_slug(d.get('titulo', ''))}.md"
    ruta.write_text(
        f"# Borrador — {d.get('titulo')} ({d.get('clase')})\n"
        f"_Vence {d.get('vence')} · generado {datetime.now().isoformat(timespec='minutes')} · "
        f"NO entregado: revísalo y complétalo antes de usarlo._\n\n{texto}\n",
        encoding="utf-8",
    )
    os.chmod(ruta, 0o600)
    if notificar:
        cuerpo = texto if len(texto) <= TELEGRAM_CAP else texto[:TELEGRAM_CAP] + "\n[…recortado; completo en el server]"
        msg = (f"📝 Ya te dejé un borrador de «{d.get('titulo')}» ({d.get('clase')}). Es un punto de partida: "
               f"lo que dice [BUSCAR] o [ALAN] lo completas tú. Completo en el server: {ruta}\n\n{cuerpo}")
        subprocess.run([str(NOTIFY), msg], env=dict(os.environ, MOCHI_ROUTE="0"),
                       capture_output=True, text=True, timeout=120)
    return f"ok:{ruta}"


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:] if not a.startswith("--")]
    if not args:
        print(__doc__)
        return 2
    res = crear(args[0], force="--force" in argv, notificar="--no-notify" not in argv)
    print(res)
    return 0 if res.startswith(("ok", "ya", "no_aplica")) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
