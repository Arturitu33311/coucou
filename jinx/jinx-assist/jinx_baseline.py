"""jinx_baseline — el ánimo de fondo de Jinx (lo que siente entre mensajes), para la cara de Mochi.

Una sola fuente para todos: el gateway (`op: mood`, jinx_chat.py) y el servidor de presencia
(`jinx_mood` en cada respuesta al latido, mochi_presence.py). Capa de PRESENTACIÓN: lee el estado y los
eventos que Hermes ya calculó y no le cambia nada a Jinx. Sin dependencias: el servidor lo recibe copiado junto
a mochi_presence.py (jinx/vga-panel/deploy.sh) y el portátil en ~/bin (install.sh).

Qué decide la cara (la primera regla que cumple):
  1. EVENTOS RECIENTES: cada evento emocional real de `emotion_turn_log.jsonl` (halago, gracia, roce…) vale una
     intensidad (suma de sus |Δ| / EVENT_FULL_DELTA, tope 1) que se corta a la mitad cada EVENT_HALF_LIFE_MIN
     minutos y SE ACUMULA con los siguientes. Con la suma >= EVENT_SHOW hay cara completa (la del evento que
     más pesa); al bajar de ese umbral vuelve a tranquilo. `normal_interaction`, `greeting` y `deterministic`
     (los empujes del cron) nunca dan cara.
  2. De 22:00 a 05:59: dormida.
  3. Estados extremos poco comunes (afecto >= 95, orgullo >= 85, energía >= 100 con score positivo).
  4. Calidez (media de affection, trust, patience) >= 74.5: contenta, con histéresis (entra en 74.5, sale en
     72.5) para que no parpadee. También contenta con emotion_score >= 2 o satisfaction >= 90.
  Si nada aplica: tranquila (None), que también es un ánimo.

Qué ya no hay: las reglas negativas por umbral (paciencia <= 30, confianza <= 30, celos >= 75, preocupación
>= 70, score <= -2, calidez <= 40, energía <= 30, aburrimiento >= 70). Los datos de 3 semanas (25 sep-14 oct,
515 filas) muestran que ninguna se alcanzó nunca: paciencia 65-71, confianza 76-85, celos 25-25.5,
preocupación 18-35, aburrimiento 12-20. Lo negativo ahora llega por eventos (un roce, ser ignorada, celos).

La escala de Hermes: cada dimensión va de 0 a 120; líneas base affection 70, trust 75, patience 65,
energy 68; `emotion_score` va de -5 a 5.

I/O: solo `load_recent_events` toca el disco (la cola acotada del log); lo demás es puro.
"""
from __future__ import annotations

import json
import os
import time
from datetime import datetime

EMOTES = ("happy", "love", "proud", "surprised", "excited", "confused", "shy", "sad", "curious", "bored",
          "scared", "grumpy", "smug", "laughing", "crying", "relieved", "determined", "flustered", "sleepy")

# ── eventos con intensidad que decae ───────────────────────────────────────────────────────────
EVENT_HALF_LIFE_MIN = 25.0   # minutos; configurable: JINX_MOOD_HALF_LIFE_MIN o el argumento half_life_min
EVENT_FULL_DELTA = 24.0      # suma de |Δ| que vale intensidad 1.0 (una gracia moderada suma 20, un halago 28)
EVENT_SHOW = 0.5             # intensidad acumulada desde la que hay cara
EVENT_CAP = 2.0              # tope de lo acumulado (varios eventos seguidos no la dejan con cara una hora)
EVENT_WINDOW_MIN = 120.0     # más viejo que esto ya pesa < 4 %: ni se mira

NEUTRAL_TRIGGERS = frozenset({"normal_interaction", "greeting", "deterministic", "decay"})

# trigger del motor (detector) o su nombre refinado en STATE.md → emote de Mochi
EVENT_EMOTE = {
    "praise": "proud", "recognition": "proud",
    "gratitude": "happy",
    "encouragement": "determined",
    "achievement": "proud", "logro_compartido": "proud",
    "humor": "laughing", "juego": "laughing",
    "intimacy": "love", "intimacy_push": "love",
    "teasing": "smug",
    "care": "love", "needed": "love",
    "sharing": "curious",
    "vulnerability": "shy", "confidencia": "shy",
    "apology": "relieved", "relationship_recovery": "relieved",
    "stress_share": "sad", "alan_mal": "sad",
    "criticism": "sad", "wounded": "sad",
    "ignored": "sad", "interruption": "sad",
    "other_ai_mentioned": "grumpy", "celos": "grumpy",
    "boredom_streak": "bored", "monotonia": "bored",
}

# ── estado de fondo ────────────────────────────────────────────────────────────────────────────
MOOD_LOVE_AFFECTION = 95    # afecto >= esto: enamorada
MOOD_PROUD_PRIDE = 85       # orgullo >= esto: orgullosa
MOOD_EXCITED_ENERGY = 100   # energía >= esto, con emotion_score >= MOOD_EXCITED_SCORE: emocionada
MOOD_EXCITED_SCORE = 1.0
MOOD_HAPPY_WARMTH_IN = 74.5   # calidez >= esto: contenta
MOOD_HAPPY_WARMTH_OUT = 72.5  # una vez contenta, sigue así hasta bajar de esto
MOOD_HAPPY_SCORE = 2.0        # emotion_score >= esto: contenta
MOOD_HAPPY_SATISFACTION = 90  # satisfacción >= esto: contenta
NIGHT_FROM, NIGHT_TO = 22, 6  # de 22:00 a 05:59 (hora local): dormida


def parse_emotion_state(md):
    """Las dimensiones numéricas de `emotion_state:` en el frontmatter de STATE.md, más `last_update` y
    `last_trigger_type` si están. Solo lo plano (dos espacios de sangría): baselines e inertia se saltan.
    None si no hay frontmatter, bloque o ninguna dimensión."""
    lines = (md or "").splitlines()
    if not lines or lines[0].strip() != "---":
        return None
    out, inside = {}, False
    for ln in lines[1:]:
        if ln.strip() == "---":
            break
        if not inside:
            inside = ln.rstrip() == "emotion_state:"
            continue
        if not ln.startswith("  "):
            break  # sangría 0: se acabó el bloque
        if ln.startswith("   "):
            continue  # más hondo: baselines.*, inertia.*
        key, _, val = ln.strip().partition(":")
        val = val.strip().strip("'\"")
        if key in ("last_update", "last_trigger_type") and val:
            out[key] = val
            continue
        try:
            out[key] = float(val)
        except ValueError:
            continue
    return out if any(isinstance(v, float) for v in out.values()) else None


# ── eventos: leer y ponderar ───────────────────────────────────────────────────────────────────

def parse_events_log(text, now=None, window_min=EVENT_WINDOW_MIN):
    """Los eventos reales (`last_event`) de un trozo de `emotion_turn_log.jsonl` dentro de la ventana, como
    [{"t": epoch, "trigger": str, "deltas": {dim: Δ}}] por orden de tiempo. Salta líneas rotas (la primera de un
    `tail` suele venir cortada) y repetidos del mismo segundo. Los timestamps del log son hora local sin zona.
    Los deltas del log son los que el motor aplicó (ya con la ganancia del experimento, si está encendido)."""
    now = time.time() if now is None else now
    out, seen = [], set()
    for ln in (text or "").splitlines():
        ln = ln.strip()
        if not ln.startswith("{"):
            continue
        try:
            row = json.loads(ln)
            ev = row.get("last_event")
            if not isinstance(ev, dict) or not ev.get("trigger"):
                continue
            t = datetime.fromisoformat(row["timestamp"]).timestamp()
        except (ValueError, KeyError, TypeError, AttributeError):
            continue
        if not (0 <= now - t <= window_min * 60):
            continue
        key = (int(t), ev["trigger"])
        if key in seen:
            continue
        seen.add(key)
        deltas = ev.get("deltas") if isinstance(ev.get("deltas"), dict) else {}
        out.append({"t": t, "trigger": str(ev["trigger"]), "deltas": deltas})
    out.sort(key=lambda e: e["t"])
    return out


def load_recent_events(path, now=None, window_min=EVENT_WINDOW_MIN, max_bytes=262144):
    """Lo único con I/O: lee la cola del log (acotada) y devuelve `parse_events_log`. Nunca falla: sin archivo
    o con uno ilegible son 0 eventos (la cara sigue siendo la del estado)."""
    try:
        with open(path, "rb") as f:
            f.seek(0, os.SEEK_END)
            size = f.tell()
            f.seek(max(0, size - max_bytes))
            raw = f.read().decode("utf-8", "replace")
        return parse_events_log(raw, now, window_min)
    except OSError:
        return []


def half_life_minutes():
    """La vida media configurable: JINX_MOOD_HALF_LIFE_MIN (minutos > 0) o EVENT_HALF_LIFE_MIN."""
    try:
        v = float(os.environ.get("JINX_MOOD_HALF_LIFE_MIN", ""))
        if v > 0:
            return v
    except ValueError:
        pass
    return EVENT_HALF_LIFE_MIN


def event_intensity(event, now, half_life_min=None):
    """La intensidad vigente de un evento (0..1): su peso inicial por 0.5^(edad / vida media). 0 si es neutro,
    desconocido o está en el futuro."""
    if not isinstance(event, dict) or event.get("trigger") in NEUTRAL_TRIGGERS:
        return 0.0
    if event.get("trigger") not in EVENT_EMOTE:
        return 0.0
    hl = half_life_min or EVENT_HALF_LIFE_MIN
    try:
        age_min = (now - float(event["t"])) / 60.0
    except (KeyError, TypeError, ValueError):
        return 0.0
    if age_min < 0:
        return 0.0
    deltas = event.get("deltas") or {}
    total = sum(abs(v) for v in deltas.values() if isinstance(v, (int, float)) and not isinstance(v, bool))
    base = min(1.0, total / EVENT_FULL_DELTA) if total else 0.4  # sin deltas legibles: peso medio-bajo
    return base * 0.5 ** (age_min / hl)


def event_mood(events, now, half_life_min=None):
    """(emote o None, intensidad acumulada). La cara es la del emote que más pesa; hay cara solo con la
    suma (con tope) >= EVENT_SHOW."""
    per, total = {}, 0.0
    for ev in events or ():
        w = event_intensity(ev, now, half_life_min)
        if w <= 0:
            continue
        emote = EVENT_EMOTE[ev["trigger"]]
        per[emote] = per.get(emote, 0.0) + w
        total += w
    total = min(total, EVENT_CAP)
    if total < EVENT_SHOW or not per:
        return None, total
    return max(per.items(), key=lambda kv: kv[1])[0], total


# ── la cara ────────────────────────────────────────────────────────────────────────────────────

def mood_now(emotion_state, events, now=None, latch=None, hour=None):
    """(emote de EMOTES o None, latch). Pura: `latch` es un dict que el que llama conserva entre llamadas
    (solo guarda la histéresis de "contenta"); pasa el mismo dict y guarda el devuelto.

    Orden: eventos recientes → noche → estados extremos → calidez/score/satisfacción (contenta).
    Una dimensión que falta o no es un número no cuenta (su regla no se aplica)."""
    now = time.time() if now is None else now
    latch = dict(latch or {})
    es = emotion_state if isinstance(emotion_state, dict) else {}

    def num(key):
        v = es.get(key)
        return float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else None

    def at_least(key, limit):
        v = num(key)
        return v is not None and v >= limit

    parts = [num("affection"), num("trust"), num("patience")]
    warmth = sum(parts) / 3 if all(p is not None for p in parts) else None
    # la histéresis se actualiza siempre, también cuando otra regla manda, para que no dependa del orden
    if warmth is not None:
        if warmth >= MOOD_HAPPY_WARMTH_IN:
            latch["happy"] = True
        elif warmth < MOOD_HAPPY_WARMTH_OUT:
            latch["happy"] = False
    happy_warmth = warmth is not None and bool(latch.get("happy"))

    emote, _ = event_mood(events, now, half_life_minutes())
    if emote:
        return emote, latch
    h = hour
    if isinstance(h, bool) or not isinstance(h, int):
        h = time.localtime(now).tm_hour
    if h >= NIGHT_FROM or h < NIGHT_TO:
        return "sleepy", latch
    if at_least("affection", MOOD_LOVE_AFFECTION):
        return "love", latch
    if at_least("pride", MOOD_PROUD_PRIDE):
        return "proud", latch
    if at_least("energy", MOOD_EXCITED_ENERGY) and at_least("emotion_score", MOOD_EXCITED_SCORE):
        return "excited", latch
    if happy_warmth or at_least("emotion_score", MOOD_HAPPY_SCORE) or at_least("satisfaction", MOOD_HAPPY_SATISFACTION):
        return "happy", latch
    return None, latch


def baseline_emote(emotion_state, hour, events=None, now=None, latch=None):
    """Solo el emote (o None) de `mood_now`. Compatible con quien llamaba `baseline_emote(estado, hora)`:
    sin eventos y sin latch decide solo el estado (sin latch, la calidez no tiene memoria: cuenta desde 74.5)."""
    return mood_now(emotion_state, events or [], now, latch, hour)[0]
