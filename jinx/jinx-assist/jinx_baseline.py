"""jinx_baseline — el ánimo de fondo de Jinx (lo que siente entre mensajes), sacado de su `emotion_state`.

Una sola fuente para todos: el gateway (`op: mood`, jinx_chat.py) y el servidor de presencia
(`jinx_mood` en cada respuesta al latido, mochi_presence.py, que lo lee del STATE.md local del servidor).
Puro, sin dependencias: el servidor lo recibe copiado junto a mochi_presence.py (jinx/vga-panel/deploy.sh)
y el portátil en ~/bin (install.sh).

La escala de Hermes: cada dimensión va de 0 a 120; las líneas base conocidas son affection 70, trust 75,
patience 65, energy 68 (emotion_calculator.py). `emotion_score` va de -5 a 5. Un día normal
(afecto 70, confianza 76, paciencia 66, energía 83, celos 25, preocupación 32…) no tiene ánimo de fondo:
Mochi está tranquilo, y eso también es su ánimo.

OJO: los umbrales de las dimensiones sin línea base conocida (worry, jealousy, boredom, pride,
satisfaction) y los de emotion_score son un punto de partida; se calibran con valores reales.
"""
from __future__ import annotations

EMOTES = ("happy", "love", "proud", "surprised", "excited", "confused", "shy", "sad", "curious", "bored",
          "scared", "grumpy", "smug", "laughing", "crying", "relieved", "determined", "flustered", "sleepy")

MOOD_GRUMPY_PATIENCE = 30   # paciencia ≤ esto: gruñona
MOOD_GRUMPY_JEALOUSY = 75   # celos ≥ esto: gruñona
MOOD_SAD_TRUST = 30         # confianza ≤ esto: triste
MOOD_SAD_SCORE = -2.0       # emotion_score ≤ esto: triste
MOOD_SCARED_WORRY = 70      # preocupación ≥ esto: asustada
MOOD_SLEEPY_WARMTH = 40     # calidez ≤ esto: sin ganas
MOOD_SLEEPY_ENERGY = 30     # energía ≤ esto: sin ganas
MOOD_BORED_BOREDOM = 70     # aburrimiento ≥ esto: aburrida
MOOD_LOVE_AFFECTION = 95    # afecto ≥ esto: enamorada
MOOD_PROUD_PRIDE = 85       # orgullo ≥ esto: orgullosa
MOOD_EXCITED_ENERGY = 100   # energía ≥ esto, con emotion_score ≥ MOOD_EXCITED_SCORE: emocionada
MOOD_EXCITED_SCORE = 1.0
MOOD_HAPPY_WARMTH = 78      # calidez ≥ esto: contenta (la línea base de la calidez es ~70)
MOOD_HAPPY_SCORE = 2.0      # emotion_score ≥ esto: contenta
MOOD_HAPPY_SATISFACTION = 90  # satisfacción ≥ esto: contenta
NIGHT_FROM, NIGHT_TO = 22, 6  # de 22:00 a 05:59 (hora local): somnolienta


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


def baseline_emote(emotion_state, hour):
    """El ánimo de fondo de Jinx, un emote de EMOTES o None (tranquila). Pura.

    calidez = media de affection, trust y patience. Primera regla que cumple:
      paciencia baja → grumpy · confianza baja → sad · celos altos → grumpy · preocupación alta → scared ·
      emotion_score bajo → sad · calidez o energía bajas → sleepy · aburrimiento alto → bored ·
      afecto muy alto → love · orgullo alto → proud · energía muy alta y score positivo → excited ·
      calidez, score o satisfacción altos → happy · de 22 a 6 h → sleepy · si no, None.
    Una dimensión que falta o no es un número no cuenta (su regla no se aplica)."""
    if not isinstance(emotion_state, dict):
        return None

    def num(key):
        v = emotion_state.get(key)
        return float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else None

    def at_least(key, limit):
        v = num(key)
        return v is not None and v >= limit

    def at_most(key, limit):
        v = num(key)
        return v is not None and v <= limit

    parts = [num("affection"), num("trust"), num("patience")]
    warmth = sum(parts) / 3 if all(p is not None for p in parts) else None
    if at_most("patience", MOOD_GRUMPY_PATIENCE):
        return "grumpy"
    if at_most("trust", MOOD_SAD_TRUST):
        return "sad"
    if at_least("jealousy", MOOD_GRUMPY_JEALOUSY):
        return "grumpy"
    if at_least("worry", MOOD_SCARED_WORRY):
        return "scared"
    if at_most("emotion_score", MOOD_SAD_SCORE):
        return "sad"
    if (warmth is not None and warmth <= MOOD_SLEEPY_WARMTH) or at_most("energy", MOOD_SLEEPY_ENERGY):
        return "sleepy"
    if at_least("boredom", MOOD_BORED_BOREDOM):
        return "bored"
    if at_least("affection", MOOD_LOVE_AFFECTION):
        return "love"
    if at_least("pride", MOOD_PROUD_PRIDE):
        return "proud"
    if at_least("energy", MOOD_EXCITED_ENERGY) and at_least("emotion_score", MOOD_EXCITED_SCORE):
        return "excited"
    if ((warmth is not None and warmth >= MOOD_HAPPY_WARMTH) or at_least("emotion_score", MOOD_HAPPY_SCORE)
            or at_least("satisfaction", MOOD_HAPPY_SATISFACTION)):
        return "happy"
    if isinstance(hour, int) and not isinstance(hour, bool) and (hour >= NIGHT_FROM or hour < NIGHT_TO):
        return "sleepy"
    return None
