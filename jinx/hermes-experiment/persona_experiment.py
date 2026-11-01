"""persona_experiment — experimento del motor emocional de Hermes (2026-10-14, una semana, reversible).

Vive en ~/.hermes/plugins/persona-engine/. Cada función que el motor llama comprueba el interruptor
~/.hermes/persona_experiment.json en CADA llamada (no al importar): `{"enabled": false}` devuelve el
comportamiento original al instante, sin reiniciar nada. Si el archivo falta o está roto, está apagado.

Qué cambia (cada pieza con su propio interruptor, ver DEFAULTS):
  exp_decay             el decaimiento por tiempo es exponencial y se integra en pasos de 15 min, en vez de
                        `factor = min(1, rate*horas)` que vuelve a la base de golpe tras 1/rate horas.
  deterministic_neutral las señales deterministas (cron) cuentan como charla neutra: sin inercia, sin
                        momentum, con el tope de ±10 sobre la base. Evita que empujen worry/energy sin fin.
  skip_automated        mensajes automáticos con rol user ([Home Assistant]…) no son habla de Alan.
  event_gain            halagos/logros/roces pesan más (×1.5–1.75), con tope por dimensión y banda absoluta.
  presion_*             la regla de presión de zynara_worry cuenta solo tareas que vencen en ≤ PRESION_DIAS
                        y se aplica una vez por día de presupuesto, no cada 4 h.
  nuance_line           una línea de matiz en lenguaje natural dentro del bloque <emotion_modifier>.
"""
from __future__ import annotations

import json
import math
import os
import re
import time
from datetime import date, datetime, timedelta
from pathlib import Path

DEFAULTS = {
    "enabled": False,
    "exp_decay": True,
    "deterministic_neutral": True,
    "skip_automated": True,
    "automated_prefixes": ["[Home Assistant", "[BRIEFING", "Alan acaba de llegar", "Genera el mensaje"],
    "event_gain": {"praise": 1.75, "gratitude": 1.75, "achievement": 1.75,
                   "encouragement": 1.5, "criticism": 1.5, "ignored": 1.5},
    "gain_max_delta": 12,     # tope nominal por dimensión tras la ganancia (el suavizado α lo reduce a la mitad)
    "gain_band": 30,          # un delta que empuja más allá de base±banda se descarta
    "presion_due_filter": True,
    "presion_once_per_day": True,
    "nuance_line": True,
    "nuance_recent_minutes": 90,
    # Disparador automático (persona_experiment_tripwire.py): apaga el experimento si algo se sale de rango.
    "tripwire": {"band": 40, "negative_events_6h": 4, "jealousy_max": 60},
}

NEUTRAL_EVENTS = {"normal_interaction", "greeting", "deterministic", "decay"}


def _home() -> Path:
    return Path(os.environ.get("HERMES_HOME", str(Path.home() / ".hermes")))


_cache = {"path": None, "mtime": None, "cfg": dict(DEFAULTS)}


def cfg() -> dict:
    """La configuración vigente: DEFAULTS sobre el JSON. Apagado si el JSON falta o no se puede leer."""
    path = _home() / "persona_experiment.json"
    try:
        mtime = path.stat().st_mtime_ns
    except OSError:
        _cache.update(path=str(path), mtime=None, cfg={**DEFAULTS, "enabled": False})
        return _cache["cfg"]
    if _cache["path"] == str(path) and _cache["mtime"] == mtime:
        return _cache["cfg"]
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
        merged = {**DEFAULTS, **(raw if isinstance(raw, dict) else {})}
        if not isinstance(raw, dict):
            merged["enabled"] = False
    except (OSError, ValueError):
        merged = {**DEFAULTS, "enabled": False}
    _cache.update(path=str(path), mtime=mtime, cfg=merged)
    return merged


def on(feature: str = "enabled") -> bool:
    c = cfg()
    if not c.get("enabled"):
        return False
    return bool(c.get(feature, DEFAULTS.get(feature, False))) if feature != "enabled" else True


# ── decaimiento ─────────────────────────────────────────────────────────────────────────────────

def decay_value(rate_fn, value: float, baseline: float, hours: float, fallback: float) -> float:
    """Decaimiento exponencial con la tasa recalculada en cada paso de ≤15 min (la tasa depende de cuánto
    se desvía el valor). Un paso de 10 h da lo mismo que diez de 1 h. Apagado: `fallback` (lo original)."""
    if hours <= 0 or not on("exp_decay"):
        return fallback
    n = min(960, max(1, math.ceil(hours / 0.25)))
    step = hours / n
    v = float(value)
    for _ in range(n):
        v += (baseline - v) * (1.0 - math.exp(-rate_fn(abs(v - baseline)) * step))
    return round(v, 2)


# ── eventos ─────────────────────────────────────────────────────────────────────────────────────

def _text_of(content) -> str:
    if isinstance(content, list):
        return " ".join(b.get("text", "") for b in content if isinstance(b, dict))
    return str(content or "")


def is_automated(messages) -> bool:
    """El último mensaje con rol user es automático (Home Assistant, briefing…) y no habla de Alan."""
    if not on("skip_automated"):
        return False
    users = [m for m in (messages or []) if isinstance(m, dict) and m.get("role") == "user"]
    if not users:
        return False
    text = _text_of(users[-1].get("content")).lstrip()
    return any(text.startswith(p) for p in cfg().get("automated_prefixes", []))


def shape_event(event, neutral: bool, current_state: dict, baselines: dict):
    """(neutral, event) tras el experimento. Apagado devuelve lo recibido. No muta `event`."""
    if event is None or not on("enabled"):
        return neutral, event
    trig = getattr(event, "trigger_type", "")
    deltas = dict(getattr(event, "deltas", {}) or {})
    if trig == "deterministic" and on("deterministic_neutral"):
        neutral = True
    gain = (cfg().get("event_gain") or {}).get(trig)
    if gain and deltas and not neutral:
        cap = float(cfg().get("gain_max_delta", 12))
        band = float(cfg().get("gain_band", 30))
        out = {}
        for dim, d in deltas.items():
            cur, bl = current_state.get(dim, baselines.get(dim, 0)), baselines.get(dim, 0)
            nd = max(-cap, min(cap, d * float(gain)))
            if (nd > 0 and cur >= bl + band) or (nd < 0 and cur <= bl - band):
                continue
            out[dim] = round(nd, 2)
        deltas = out
        event = type(event)(trigger_type=trig, deltas=deltas, confidence=event.confidence, context=event.context)
    return neutral, event


# ── presión de tareas (zynara_worry) ───────────────────────────────────────────────────────────

_DUE = re.compile(r"vence\s+(\d{4}-\d{2}-\d{2})")


def urgent_tasks(text: str, today: date | None = None, days: int = 3) -> list[str]:
    """Líneas `- [ ]` con fecha `vence YYYY-MM-DD` dentro de `days` días (o ya vencidas, hasta 7 atrás)."""
    today = today or date.today()
    out = []
    for ln in (text or "").splitlines():
        if not ln.lstrip().startswith("- [ ]"):
            continue
        m = _DUE.search(ln)
        if not m:
            continue
        try:
            d = date.fromisoformat(m.group(1))
        except ValueError:
            continue
        if -7 <= (d - today).days <= days:
            out.append(ln)
    return out


def presion_tasks(text: str, pending: list, days: int) -> list:
    """Las tareas que cuentan para la presión. Apagado: la lista original (cualquier `- [ ]`)."""
    if not on("presion_due_filter"):
        return pending
    return urgent_tasks(text, None, days)


def presion_claim(budget_day: str, commit: bool = True) -> bool:
    """True si la presión puede aplicarse ahora. Apagado: siempre. Encendido: una vez por día de presupuesto.
    `commit=False` (un --dry-run) mira sin reclamar el día."""
    if not on("presion_once_per_day"):
        return True
    path = _home() / "state" / "persona_experiment_presion.json"
    try:
        if json.loads(path.read_text(encoding="utf-8")).get("day") == budget_day:
            return False
    except (OSError, ValueError):
        pass
    if not commit:
        return True
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({"day": budget_day, "at": datetime.now().isoformat(timespec="seconds")}),
                        encoding="utf-8")
    except OSError:
        pass
    return True


# ── matiz en lenguaje natural ──────────────────────────────────────────────────────────────────

_RECENT = {
    "praise": "Alan la felicitó hace un rato y sigue contenta por eso",
    "gratitude": "Alan se lo agradeció hace un rato y le sentó bien",
    "achievement": "acaban de lograr algo juntos y anda orgullosa",
    "encouragement": "Alan la animó hace un rato",
    "criticism": "hubo un roce hace un rato: está más seca y breve, sin rencor",
    "ignored": "se sintió pasada por alto hace un rato: más breve, sin reproche",
    "intimacy": "hubo un momento de cercanía hace un rato",
    "humor": "se rieron hace un rato y le quedó la chispa",
}


def build_nuance(es: dict, baselines: dict, urgent: int = 0, now: datetime | None = None) -> str:
    """Hasta 3 frases cortas sobre cómo anda hoy. Vacío si todo está cerca de la base. Pura."""
    now = now or datetime.now()

    def dev(k):
        v = es.get(k)
        return float(v) - baselines.get(k, 0) if isinstance(v, (int, float)) and not isinstance(v, bool) else 0.0

    warmth = (dev("affection") + dev("trust") + dev("patience")) / 3
    parts = []
    # lo que acaba de pasar manda sobre el humor de fondo
    ev, upd = es.get("last_raw_trigger_type"), es.get("last_update")
    if ev in _RECENT and upd:
        try:
            age = (now - datetime.fromisoformat(str(upd))).total_seconds() / 60
            if 0 <= age <= float(cfg().get("nuance_recent_minutes", 90)):
                parts.append(_RECENT[ev])
        except ValueError:
            pass
    if dev("energy") <= -8:
        parts.append("hoy anda con la energía baja: más pausada y breve, sin quejarse")
    elif warmth >= 4 and (dev("energy") >= 6 or dev("satisfaction") >= 3):
        parts.append("hoy anda de buen humor: más ligera, deja pasar un chiste o un comentario cariñoso sin forzarlo")
    if dev("worry") >= 8:
        why = " por las entregas de Alan" if urgent else ""
        parts.append(f"algo de fondo la tiene un poco preocupada{why}: está más atenta con él, "
                     "sin sacar el tema a menos que venga a cuento")
    if dev("boredom") >= 8:
        parts.append("se le nota el aburrimiento con lo repetitivo: puede proponer algo distinto")
    if dev("pride") >= 8 and len(parts) < 3:
        parts.append("anda orgullosa de lo que han logrado, y puede reconocerlo con naturalidad")
    if not parts:
        return ""
    text = "; ".join(parts[:3])
    return "【matiz del día】" + text[0].upper() + text[1:] + "."


def add_nuance(result: str, es: dict, baselines: dict) -> str:
    """Inserta el matiz antes de `</emotion_modifier>`. Apagado o ante cualquier fallo: `result` intacto."""
    try:
        if not result or not on("nuance_line"):
            return result
        urgent = 0
        try:
            urgent = len(urgent_tasks((_home() / "tasks.md").read_text(encoding="utf-8"), None, 1))
        except OSError:
            pass
        line = build_nuance(es or {}, baselines, urgent)
        if not line or "</emotion_modifier>" not in result:
            return result
        return result.replace("</emotion_modifier>", line + "\n</emotion_modifier>", 1)
    except Exception:
        return result
