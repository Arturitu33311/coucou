#!/usr/bin/env python3
"""jinx_island — la herramienta con la que Jinx pide cosas a la isla de Coucou en el PC de Alan.

Uso (lo corre Jinx con su terminal, ver island/SKILL.md):
    python3 jinx_island.py <acción> ['<json con los argumentos>']
    python3 jinx_island.py music '{"action":"next"}'
    python3 jinx_island.py state

Valida con island_commands (la lista cerrada de lo permitido), pone la orden en el servidor de presencia (que la
valida otra vez), espera a que el PC conteste y escribe UNA línea JSON: {"ok","message","data"?}. Sale con 0 si la
orden se hizo y 1 si no. Sin dependencias; la llave del servidor se lee de ~/.config/mochi-presence/key (en el propio
servidor basta con el loopback) y nunca se imprime.
"""
import json
import os
import sys
import urllib.error
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import island_commands  # noqa: E402

# (JINX_PRESENCE_URL ya es otra cosa: la dirección completa de los mensajes, en jinx_chat.py)
BASE = os.environ.get("JINX_ISLAND_URL", "http://127.0.0.1:8897").rstrip("/")
KEY_PATH = os.path.expanduser("~/.config/mochi-presence/key")
# La isla contesta en menos de 6 s (pregunta a la página y espera); la espera larga cubre un PC lento.
RESULT_WAIT = 20


def _key():
    try:
        with open(KEY_PATH, encoding="utf-8") as f:
            return f.read().strip()
    except OSError:
        return ""


def _call(method, path, body=None, timeout=30, opener=urllib.request.urlopen):
    """(código, json). Nunca lanza: un servidor caído es un resultado más."""
    headers = {"Content-Type": "application/json"}
    key = _key()
    if key:
        headers["X-Mochi-Key"] = key
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(BASE + path, data=data, method=method, headers=headers)
    try:
        with opener(req, timeout=timeout) as r:
            return r.status, json.loads(r.read() or b"{}")
    except urllib.error.HTTPError as e:
        try:
            return e.code, json.loads(e.read() or b"{}")
        except ValueError:
            return e.code, {}
    except Exception as e:  # noqa: BLE001 - sin red, sin servidor, JSON roto
        return 0, {"error": "no pude hablar con el servidor de presencia (%s)" % type(e).__name__}


def run(action, args, call=_call):
    """Una orden de principio a fin: (ok, salida). `call` se puede cambiar en los tests."""
    ok, clean = island_commands.validate(action, args)
    if not ok:
        return False, {"ok": False, "message": clean}
    code, sent = call("POST", "/api/island/command", {"action": action, "args": clean}, 15)
    if code != 200:
        why = sent.get("message") or sent.get("error") or "el servidor dijo %s" % code
        return False, {"ok": False, "message": why}
    code, res = call("GET", "/api/island/result?id=%d&wait=%d" % (sent["id"], RESULT_WAIT), None, RESULT_WAIT + 10)
    if code != 200:
        return False, {"ok": False, "message": res.get("error") or "no pude leer el resultado"}
    if not res.get("done"):
        return False, {"ok": False, "message": "el PC no contestó a tiempo (la orden puede haberse hecho)"}
    r = res["result"]
    out = {"ok": bool(r.get("ok")), "message": r.get("message", "")}
    if "data" in r:
        out["data"] = r["data"]
    return out["ok"], out


def main(argv):
    if len(argv) < 2 or argv[1] in ("-h", "--help"):
        print(json.dumps({"ok": False, "message": "uso: jinx_island.py <acción> ['<json>']; acciones: %s"
                          % ", ".join(island_commands.ACTIONS)}))
        return 1
    try:
        args = json.loads(argv[2]) if len(argv) > 2 else {}
    except ValueError:
        print(json.dumps({"ok": False, "message": "los argumentos no son JSON"}))
        return 1
    ok, out = run(argv[1], args)
    print(json.dumps(out, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
