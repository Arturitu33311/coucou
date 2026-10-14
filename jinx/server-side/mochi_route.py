#!/usr/bin/env python3
"""mochi_route — entrega un texto de Jinx al mochi activo (PC o panel VGA).

Solo texto, sin voz. No decide nada de Telegram: devuelve ok/fallo y el
funnel (notify_alan.sh / jinx_send.py) cae a Telegram si esto falla.

Uso:
    mochi_route.py "texto" [--mood Happy --kind info --source pendientes]
    echo "texto" | mochi_route.py [...]
Imprime el active_device (pc|vga) y sale 0, o "FAIL: motivo" y sale 1.
"""
from __future__ import annotations

import json
import sys
import urllib.request

URL = "http://127.0.0.1:8897/api/mochi/message"
TIMEOUT = 8


def deliver(text: str, mood=None, kind="info", source="") -> tuple[bool, str]:
    if not text or not text.strip():
        return False, "vacío"
    payload = json.dumps({
        "text": text.strip()[:500],
        "mood": mood,
        "kind": kind if kind in ("info", "warn", "alert") else "info",
        "source": (source or "")[:60],
    }).encode()
    req = urllib.request.Request(URL, data=payload,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
            d = json.loads(r.read())
    except Exception as ex:
        return False, str(ex)[:120]
    if d.get("ok"):
        # watched=false: nadie mira ningún dispositivo (Mochi esta en el panel VGA). El aviso queda en el panel,
        # pero se informa como fallo para que el embudo (notify_alan.sh, jinx_send.py) tambien avise por Telegram.
        if d.get("watched") is False:
            return False, "nadie mirando (%s)" % d.get("active_device", "?")
        return True, d.get("active_device", "?")
    return False, "rechazado"


def main(argv) -> int:
    args, text, mood, kind, source = argv[1:], "", None, "info", ""
    while args and args[0].startswith("--"):
        k, args = args[0], args[1:]
        if k in ("--mood", "--kind", "--source") and args:
            v, args = args[0], args[1:]
            if k == "--mood":
                mood = v
            elif k == "--kind":
                kind = v
            else:
                source = v
    if args:
        text = " ".join(args)
    else:
        text = sys.stdin.read()
    ok, info = deliver(text, mood, kind, source)
    print(info if ok else f"FAIL: {info}")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
