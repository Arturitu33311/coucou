#!/usr/bin/env python3
"""Servidor de PRUEBA del panel VGA (no es el de producción).

Sirve dist/panel.html en :8896 y simula /api/status (desde un JSON de muestra) y el servidor de presencia en
:8897 (/api/mochi/inbox, /api/mochi/ack). Sirve para capturas y pruebas visuales:

  mock_server.py --status status.json --active vga --msg "Texto" --kind alert
  --active   vga | pc | s21 | s22 | none (sin respuesta)
  --msg/--kind   un mensaje sin leer (repetible: --msg A --msg B)
"""
import argparse
import json
import os
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))


def make_handler(page_path, status, active, msgs, port_role):
    acked = set()

    class H(BaseHTTPRequestHandler):
        def _send(self, code, body, ctype="application/json; charset=utf-8"):
            data = body if isinstance(body, bytes) else json.dumps(body, ensure_ascii=False).encode()
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Access-Control-Allow-Headers", "Content-Type")
            self.end_headers()
            self.wfile.write(data)

        def do_OPTIONS(self):
            self._send(204, b"")

        def do_GET(self):
            if port_role == "panel":
                if self.path in ("/", "/index.html"):
                    with open(page_path, "r", encoding="utf-8") as f:
                        html = f.read()
                    if os.environ.get("MOCK_NO_FONTS"):
                        # el entorno de captura no tiene red: sin las fuentes de Google (en el kiosco sí cargan)
                        import re
                        html = re.sub(r"<link[^>]*(fonts\.googleapis|fonts\.gstatic)[^>]*>", "", html)
                    return self._send(200, html.encode("utf-8"), "text/html; charset=utf-8")
                if self.path.startswith("/api/status"):
                    return self._send(200, status)
                return self._send(404, {"error": "no"})
            if self.path.startswith("/api/mochi/inbox"):
                if active == "none":
                    return self._send(503, {"error": "sin presencia"})
                live = [m for m in msgs if m["id"] not in acked]
                return self._send(200, {"messages": live, "active_device": active, "ts": int(time.time())})
            return self._send(404, {"error": "no"})

        def do_POST(self):
            if port_role == "presence" and self.path == "/api/mochi/ack":
                n = int(self.headers.get("Content-Length", 0))
                try:
                    acked.add(int(json.loads(self.rfile.read(n)).get("id")))
                except Exception:
                    pass
                return self._send(200, {"ok": True})
            return self._send(404, {"error": "no"})

        def log_message(self, *a):
            pass

    return H


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--status", required=True)
    ap.add_argument("--page", default=os.path.join(HERE, "dist", "panel.html"))
    ap.add_argument("--active", default="vga")
    ap.add_argument("--msg", action="append", default=[])
    ap.add_argument("--kind", action="append", default=[])
    ap.add_argument("--port", type=int, default=8896)
    ap.add_argument("--presence-port", type=int, default=8897)
    a = ap.parse_args()
    with open(a.status, encoding="utf-8") as f:
        status = json.load(f)
    msgs = []
    for i, text in enumerate(a.msg):
        kind = a.kind[i] if i < len(a.kind) else "info"
        msgs.append({"id": i + 1, "text": text, "kind": kind, "source": "prueba", "ts": int(time.time()) - 120})
    srv_p = ThreadingHTTPServer(("127.0.0.1", a.presence_port), make_handler(a.page, status, a.active, msgs, "presence"))
    srv = ThreadingHTTPServer(("127.0.0.1", a.port), make_handler(a.page, status, a.active, msgs, "panel"))
    threading.Thread(target=srv_p.serve_forever, daemon=True).start()
    print("panel en http://127.0.0.1:%d/  presencia en :%d (active=%s, %d mensajes)" % (a.port, a.presence_port, a.active, len(msgs)), flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
