#!/usr/bin/env python3
"""mochi_presence — presence PC/teléfonos<->panel VGA + inbox de mensajes de Jinx (solo texto).

El panel VGA es el RESPALDO de la casa: si ningún otro dispositivo (pc, s21, s22) late con la pantalla
encendida y tocada hace poco, `active_device` es "vga" y es él quien muestra a Mochi.

Sin voz: nada de TTS/STT/mic aquí. Los mensajes entran por POST y los muestra
el dispositivo activo (PC via coucou, o el panel VGA). Telegram queda como
fallback en los funnels (ver mochi_route.py), nunca aquí.

Endpoints (127.0.0.1:8897 + IP tailnet, clave X-Mochi-Key salvo localhost):
  POST /api/presence/heartbeat {"device":"pc|s21|s22","idle_seconds":N,"screen_on":bool}
  GET  /api/presence/state
  POST /api/mochi/message {"text","mood"|null,"kind","source"}  (texto <=500)
  GET  /api/mochi/inbox?since_id=N  -> {messages, active_device}
  POST /api/mochi/ack {"id","by"}
  GET  /health
"""
from __future__ import annotations

import json
import os
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

BASE = Path(__file__).parent
STATE_PATH = BASE / "mochi_presence_state.json"
KEY_PATH = Path.home() / ".config" / "mochi-presence" / "key"

BIND_TAILNET = os.environ.get("MOCHI_BIND_TAILNET", "100.92.254.125")
PORT = int(os.environ.get("MOCHI_PORT", "8897"))

HEARTBEAT_TIMEOUT = 45   # s sin latir -> el dispositivo se declara fuera
IDLE_LIMIT = 300         # idle mayor -> no cuenta como "mirado"
DEVICES = ("pc", "s21", "s22")  # el panel (vga) es el failsafe: siempre presente, no late
# La página del panel (kiosco) es el único navegador que habla con este servidor desde un origen web.
PANEL_ORIGINS = ("http://127.0.0.1:8896", "http://localhost:8896")
INBOX_CAP = 50
TEXT_CAP = 500
MOODS = {"Idle", "Happy", "Sleepy", "Alert", "Listening", "Celebrating",
         "Dancing", "Tired",
         # Las caras de Jinx (los emotes de Mochi en PC y teléfono: MochiMood.kt con `emote`).
         "Love", "Proud", "Excited", "Confused", "Shy", "Sad", "Curious", "Bored",
         "Scared", "Grumpy", "Smug", "Laughing", "Crying", "Relieved", "Determined",
         "Flustered"}
KINDS = {"info", "warn", "alert"}

state = {
    "presence": {},      # device -> {idle_seconds, screen_on, ts}
    "inbox": [],         # [{id, text, mood, kind, source, ts, acked_by}]
    "next_id": 1,
}


def load():
    try:
        data = json.loads(STATE_PATH.read_text(encoding="utf-8"))
        if isinstance(data, dict):
            state["inbox"] = [m for m in data.get("inbox", []) if isinstance(m, dict)][:INBOX_CAP]
            state["next_id"] = int(data.get("next_id", 1) or 1)
    except Exception:
        pass


def save():
    try:
        STATE_PATH.write_text(json.dumps(
            {"inbox": state["inbox"][-INBOX_CAP:], "next_id": state["next_id"]},
            ensure_ascii=False), encoding="utf-8")
    except Exception:
        pass


def read_key() -> str:
    try:
        return KEY_PATH.read_text(encoding="utf-8").strip()
    except Exception:
        return ""


def active_device(now: float) -> str:
    best, best_idle = "vga", None
    for dev in DEVICES:
        p = state["presence"].get(dev)
        if not p or now - p["ts"] >= HEARTBEAT_TIMEOUT:
            continue
        if not p.get("screen_on") or p.get("idle_seconds", 0) >= IDLE_LIMIT:
            continue
        idle = p["idle_seconds"]
        if best_idle is None or idle < best_idle or (idle == best_idle and dev == "pc"):
            best, best_idle = dev, idle
    return best


def host_allowed(host_header, port):
    """Loopback y la IP de la tailnet: cualquier otro `Host` es DNS rebinding."""
    if not host_header:
        return False
    return host_header.strip().lower() in (
        "127.0.0.1:%d" % port, "localhost:%d" % port, "[::1]:%d" % port,
        "%s:%d" % (BIND_TAILNET.lower(), port))


class Handler(BaseHTTPRequestHandler):
    server_version = "MochiPresence/1.0"

    def _cors_origin(self):
        origin = self.headers.get("Origin", "")
        return origin if origin in PANEL_ORIGINS else None

    def _host_ok(self):
        if host_allowed(self.headers.get("Host"), PORT):
            return True
        self._send(421, {"error": "host no permitido"})
        return False

    def _send(self, code, body, ctype="application/json; charset=utf-8"):
        data = body if isinstance(body, bytes) else json.dumps(body, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        allow = self._cors_origin()
        if allow:
            self.send_header("Access-Control-Allow-Origin", allow)
            self.send_header("Vary", "Origin")
            self.send_header("Access-Control-Allow-Headers", "Content-Type, X-Mochi-Key")
            self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.end_headers()
        self.wfile.write(data)

    def _authed(self) -> bool:
        if self.client_address[0] in ("127.0.0.1", "::1"):
            return True
        want = read_key()
        if not want:
            return False
        import hmac
        return hmac.compare_digest(self.headers.get("X-Mochi-Key", ""), want)

    def _body(self, limit=4096):
        try:
            n = int(self.headers.get("Content-Length", 0))
        except ValueError:
            return None
        if n <= 0 or n > limit:
            return None
        try:
            return json.loads(self.rfile.read(n))
        except Exception:
            return None

    def do_GET(self):
        if not self._host_ok():
            return
        if self.path == "/health":
            return self._send(200, {"ok": True})
        if not self._authed():
            return self._send(403, {"error": "forbidden"})
        if self.path == "/api/presence/state":
            now = time.time()

            def view(dev):
                p = state["presence"].get(dev)
                return {**p, "age_s": round(now - p["ts"], 1)} if p else None

            return self._send(200, {
                "active_device": active_device(now),
                **{dev: view(dev) for dev in DEVICES},
                "devices": list(DEVICES),
                "failsafe": "vga",
                "ts": int(now),
            })
        if self.path.startswith("/api/mochi/inbox"):
            since = 0
            if "since_id=" in self.path:
                try:
                    since = int(self.path.split("since_id=")[1].split("&")[0])
                except ValueError:
                    pass
            now = time.time()
            msgs = [m for m in state["inbox"]
                    if not m.get("acked_by") and m["id"] > since][-INBOX_CAP:]
            return self._send(200, {
                "messages": msgs,
                "active_device": active_device(now),
                "ts": int(now),
            })
        return self._send(404, {"error": "not found"})

    def do_OPTIONS(self):
        if not self._host_ok():
            return
        self.send_response(204)
        allow = self._cors_origin()
        if allow:
            self.send_header("Access-Control-Allow-Origin", allow)
            self.send_header("Vary", "Origin")
            self.send_header("Access-Control-Allow-Headers", "Content-Type, X-Mochi-Key")
            self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self):
        if not self._host_ok():
            return
        # CSRF: el loopback se acepta sin llave, asi que una pagina web que haga un POST "simple" a 127.0.0.1
        # pasaria. Los navegadores siempre mandan Origin en un POST cross-site; los clientes propios no.
        origin = self.headers.get("Origin")
        if origin is not None and origin not in PANEL_ORIGINS:
            return self._send(403, {"error": "origen no permitido"})
        if not self._authed():
            return self._send(403, {"error": "forbidden"})
        now = time.time()
        if self.path == "/api/presence/heartbeat":
            b = self._body()
            if not isinstance(b, dict) or b.get("device") not in DEVICES:
                return self._send(400, {"error": "device debe ser " + "|".join(DEVICES)})
            try:
                idle = max(0, int(b.get("idle_seconds", 0)))
            except (TypeError, ValueError):
                return self._send(400, {"error": "idle_seconds inválido"})
            state["presence"][b["device"]] = {
                "idle_seconds": idle,
                "screen_on": bool(b.get("screen_on", True)),
                "ts": now,
            }
            return self._send(200, {"ok": True, "active_device": active_device(now)})
        if self.path == "/api/mochi/message":
            b = self._body()
            if not isinstance(b, dict) or not str(b.get("text", "")).strip():
                return self._send(400, {"error": "text requerido"})
            mood = b.get("mood")
            kind = b.get("kind", "info")
            msg = {
                "id": state["next_id"],
                "text": str(b["text"]).strip()[:TEXT_CAP],
                "mood": mood if mood in MOODS else None,
                "kind": kind if kind in KINDS else "info",
                "source": str(b.get("source", ""))[:60],
                "ts": int(now),
            }
            state["next_id"] += 1
            state["inbox"].append(msg)
            state["inbox"] = state["inbox"][-INBOX_CAP:]
            save()
            return self._send(200, {"ok": True, "id": msg["id"],
                                   "active_device": active_device(now)})
        if self.path == "/api/mochi/ack":
            b = self._body()
            if not isinstance(b, dict):
                return self._send(400, {"error": "json requerido"})
            try:
                mid = int(b.get("id", 0))
            except (TypeError, ValueError):
                return self._send(400, {"error": "id inválido"})
            for m in state["inbox"]:
                if m["id"] == mid:
                    m["acked_by"] = str(b.get("by", ""))[:16]
                    save()
                    return self._send(200, {"ok": True})
            return self._send(404, {"error": "id desconocido"})
        return self._send(404, {"error": "not found"})

    def log_message(self, fmt, *args):
        pass


def serve_on(addr):
    srv = ThreadingHTTPServer((addr, PORT), Handler)
    print(f"[mochi_presence] en http://{addr}:{PORT}/", flush=True)
    srv.serve_forever()


def main():
    load()
    import threading
    addrs = ["127.0.0.1"]
    if BIND_TAILNET and BIND_TAILNET not in addrs:
        addrs.append(BIND_TAILNET)
    threads = [threading.Thread(target=serve_on, args=(a,), daemon=True) for a in addrs]
    for t in threads:
        t.start()
    for t in threads:
        t.join()


if __name__ == "__main__":
    main()
