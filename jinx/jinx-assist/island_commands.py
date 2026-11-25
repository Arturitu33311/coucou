"""island_commands — lo que Jinx puede pedirle a la isla del PC, y nada más.

UNA sola fuente de las reglas: la usa el servidor de presencia (mochi_presence.py: valida antes de encolar), la
herramienta de Jinx (jinx_island.py: valida antes de pedir) y, con la misma lista escrita en Rust, el PC (island_cmd.rs:
valida otra vez antes de ejecutar). Tres capas porque lo que Jinx lee (webs, Telegram, notas) no es de fiar: un texto
malicioso podría intentar que pida algo. Por eso la lista es cerrada y de cosas inofensivas.

Lo que NUNCA hay aquí: aprobar o negar permisos de agentes, enviar correos, tocar claves o ajustes, abrir direcciones o
archivos, ejecutar comandos. `close_app` pide cerrar la ventana (el programa pregunta si hay algo sin guardar) y solo
de las apps que Alan permitió en Ajustes (Jinx y Hub).

Sin dependencias: el servidor lo recibe copiado al lado de mochi_presence.py (deploy.sh).
"""
from __future__ import annotations

# Los mismos 19 emotes que jinx_thread.EMOTES y windows/src/core/jinx-mood.ts (un test lo comprueba).
EMOTES = ("happy", "love", "proud", "surprised", "excited", "confused", "shy", "sad", "curious", "bored",
          "scared", "grumpy", "smug", "laughing", "crying", "relieved", "determined", "flustered", "sleepy")

# Las herramientas del Hub que se pueden abrir (windows/src/hub/tools.ts).
HUB_TOOLS = ("today", "system", "server", "pomodoro", "tasks", "workspaces", "notes", "clipboard",
             "calendar", "weather", "quick", "shelf")

MUSIC_OPS = ("play_pause", "next", "previous", "set_volume", "search")
PILL_OPS = ("list", "add", "remove", "set_main")

ACTIONS = ("state", "music", "pills", "mochi", "hub", "open_app", "close_app")

NAME_MAX = 60
QUERY_MAX = 100
NAMES_MAX = 4


def _text(value, limit):
    """Una línea corta de texto sin caracteres de control, o None."""
    if not isinstance(value, str):
        return None
    clean = " ".join("".join(c if c.isprintable() else " " for c in value).split())
    return clean[:limit] if clean else None


def validate(action, args):
    """(True, args_limpios) o (False, "motivo"). Solo pasan las claves conocidas, con tipo y tamaño acotados."""
    if action not in ACTIONS:
        return False, "acción desconocida: %s (las que hay: %s)" % (_text(action, 30), ", ".join(ACTIONS))
    args = args if isinstance(args, dict) else {}

    if action == "state":
        return True, {}

    if action == "music":
        op = args.get("action")
        if op not in MUSIC_OPS:
            return False, "music necesita action: %s" % ", ".join(MUSIC_OPS)
        if op == "set_volume":
            v = args.get("volume")
            if isinstance(v, bool) or not isinstance(v, (int, float)):
                return False, "set_volume necesita volume (0-100)"
            return True, {"action": op, "volume": float(max(0.0, min(100.0, float(v))))}
        if op == "search":
            q = _text(args.get("query"), QUERY_MAX)
            return (True, {"action": op, "query": q}) if q else (False, "search necesita query")
        return True, {"action": op}

    if action == "pills":
        op = args.get("action", "list")
        if op not in PILL_OPS:
            return False, "pills necesita action: %s" % ", ".join(PILL_OPS)
        if op == "list":
            return True, {"action": op}
        raw = args.get("names")
        raw = [raw] if isinstance(raw, str) else raw
        names = [n for n in (_text(x, NAME_MAX) for x in (raw if isinstance(raw, list) else [])) if n][:NAMES_MAX]
        if not names:
            return False, "%s necesita names (nombres de píldoras)" % op
        if op == "set_main":
            names = names[:1]
        return True, {"action": op, "names": names}

    if action == "mochi":
        emote = args.get("emote")
        if emote not in EMOTES:
            return False, "mochi necesita emote: %s" % ", ".join(EMOTES)
        seconds = args.get("seconds", 3)
        seconds = 3.0 if isinstance(seconds, bool) or not isinstance(seconds, (int, float)) else float(seconds)
        return True, {"emote": emote, "seconds": max(1.0, min(8.0, seconds))}

    if action == "hub":
        tool = args.get("tool")
        if tool not in HUB_TOOLS:
            return False, "hub necesita tool: %s" % ", ".join(HUB_TOOLS)
        return True, {"tool": tool}

    # open_app / close_app: el nombre de una app instalada. Nada más: ni argumentos, ni rutas, ni comandos.
    name = _text(args.get("name"), NAME_MAX)
    if not name:
        return False, "%s necesita name (el nombre de la app)" % action
    return True, {"name": name}
