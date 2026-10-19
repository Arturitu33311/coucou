#!/usr/bin/env python3
"""mochi_presence — presence PC/teléfonos<->panel VGA + inbox de mensajes de Jinx (solo texto).

El panel VGA es el RESPALDO de la casa: si ningún otro dispositivo (pc, s21, s22) late con la pantalla
encendida y tocada hace poco, `active_device` es "vga" y es él quien muestra a Mochi.

Sin voz: nada de TTS/STT/mic aquí. Los mensajes entran por POST y los muestra
el dispositivo activo (PC via coucou, o el panel VGA). Telegram queda como
fallback en los funnels (ver mochi_route.py), nunca aquí.

Endpoints (127.0.0.1:8897 + IP tailnet, clave X-Mochi-Key salvo localhost):
  POST /api/presence/heartbeat {"device":"pc|s21|s22","idle_seconds":N,"screen_on":bool}
       -> {ok, active_device, jinx_mood}   ("laptop" se acepta como alias de "pc")
  GET  /api/presence/state
  POST /api/mochi/message {"text","mood"|null,"kind","source"}  (texto <=500)
       -> {ok, id, active_device, watched}   watched=false: nadie mirando, avisar también por Telegram
  GET  /api/mochi/inbox?since_id=N  -> {messages, active_device}
  POST /api/mochi/ack {"id","by"}
  POST /api/island/command {"action","args"}   (Jinx → la isla del PC; solo lo de island_commands.py)
       -> {ok, id}  |  409 pc_offline  |  429 demasiadas  |  400 con el motivo
  GET  /api/island/poll?device=pc&wait=N       (el PC pregunta por órdenes; espera hasta N s, máx. 25)
  POST /api/island/result {"id","ok","message","data"}   (el PC dice cómo le fue)
  GET  /api/island/result?id=N&wait=N          (quien pidió espera el resultado)
  GET  /health
"""
from __future__ import annotations

import json
import os
import sys
import threading
import time
from urllib.parse import parse_qs, urlparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

# El ánimo de fondo de Jinx: el mismo módulo que usa el gateway. En el servidor va copiado al lado de este
# archivo (deploy.sh); en el repo está en jinx/jinx-assist.
sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.append(str(Path(__file__).resolve().parents[2] / "jinx-assist"))
try:
    from jinx_baseline import load_recent_events, mood_now, parse_emotion_state
except ImportError:  # sin el módulo el servidor sigue: simplemente no hay ánimo de fondo
    load_recent_events = mood_now = parse_emotion_state = None
try:
    import island_commands
except ImportError:  # sin el módulo no se aceptan órdenes para la isla; todo lo demás sigue
    island_commands = None

BASE = Path(__file__).parent
STATE_PATH = BASE / "mochi_presence_state.json"
KEY_PATH = Path.home() / ".config" / "mochi-presence" / "key"

BIND_TAILNET = os.environ.get("MOCHI_BIND_TAILNET", "100.92.254.125")
PORT = int(os.environ.get("MOCHI_PORT", "8897"))

HEARTBEAT_TIMEOUT = 45   # s sin latir -> el dispositivo se declara fuera
IDLE_LIMIT = 300         # idle mayor -> no cuenta como "mirado"
DEVICES = ("pc", "s21", "s22")  # el panel (vga) es el failsafe: siempre presente, no late
# Coucou del portátil se llama a sí mismo "laptop"; aquí es "pc".
ALIASES = {"laptop": "pc"}
# El STATE.md de Hermes (local en este servidor) y cada cuánto se vuelve a leer.
JINX_STATE_MD = Path(os.environ.get("MOCHI_JINX_STATE", str(Path.home() / ".hermes" / "STATE.md")))
# El log de turnos de Hermes: de ahí salen los eventos recientes (halagos, roces…) que le dan cara a Mochi.
JINX_EVENTS_LOG = Path(os.environ.get("MOCHI_JINX_EVENTS",
                                      str(Path.home() / ".hermes" / "logs" / "emotion_turn_log.jsonl")))
JINX_MOOD_TTL = 60
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

# Órdenes de Jinx para la isla del PC. En memoria: si el servidor se reinicia se pierden, y está bien (caducan solas).
ISLAND_DEVICE = "pc"      # lo único que puede ejecutarlas
ISLAND_TTL = 90           # s: una orden que el PC no recogió en este tiempo caduca
ISLAND_RATE = 30          # órdenes por minuto, de todas
ISLAND_MAX_WAIT = 25      # s: lo más que espera una petición larga
ISLAND_KEEP = 100         # resultados guardados
island = {"next_id": 1, "queue": [], "results": {}, "stamps": [], "pc_polled": 0.0}
island_cv = threading.Condition()

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


def pc_alive(now: float) -> bool:
    """El PC late (con la pantalla como esté) o está esperando órdenes: hay a quién pedirle cosas."""
    p = state["presence"].get(ISLAND_DEVICE)
    beating = bool(p) and now - p["ts"] < HEARTBEAT_TIMEOUT
    return beating or now - island["pc_polled"] < ISLAND_MAX_WAIT + 15


def island_submit(action, args, now):
    """Encola una orden. (código HTTP, cuerpo). Todo se valida aquí; el PC lo valida otra vez."""
    if island_commands is None:
        return 503, {"error": "este servidor no tiene island_commands.py"}
    ok, clean = island_commands.validate(action, args)
    if not ok:
        return 400, {"error": clean}
    with island_cv:
        if not pc_alive(now):
            return 409, {"error": "pc_offline", "message": "el PC no está conectado ahora mismo"}
        island["stamps"] = [t for t in island["stamps"] if now - t < 60]
        if len(island["stamps"]) >= ISLAND_RATE:
            return 429, {"error": "demasiadas órdenes por minuto"}
        island["stamps"].append(now)
        cmd = {"id": island["next_id"], "action": action, "args": clean, "ts": now, "sent": False}
        island["next_id"] += 1
        island["queue"].append(cmd)
        island_cv.notify_all()
    return 200, {"ok": True, "id": cmd["id"]}


def _island_expire(now):
    """Las órdenes que el PC no recogió a tiempo caducan, y quien espera su resultado lo sabe. Con el candado."""
    for cmd in [c for c in island["queue"] if not c["sent"] and now - c["ts"] > ISLAND_TTL]:
        island["queue"].remove(cmd)
        island["results"][cmd["id"]] = {"ok": False, "message": "caducó: el PC no la recogió", "ts": now}
    for old in sorted(island["results"])[:-ISLAND_KEEP]:
        island["results"].pop(old, None)
    island["queue"] = [c for c in island["queue"] if now - c["ts"] < ISLAND_TTL * 4]


def island_poll(wait, clock=time.time):
    """Las órdenes nuevas para el PC; espera hasta `wait` s a que llegue alguna. Las marca entregadas."""
    wait = max(0.0, min(float(wait), ISLAND_MAX_WAIT))
    end = clock() + wait
    with island_cv:
        while True:
            now = clock()
            island["pc_polled"] = now
            _island_expire(now)
            fresh = [c for c in island["queue"] if not c["sent"]]
            if fresh or now >= end:
                for c in fresh:
                    c["sent"] = True
                return [{"id": c["id"], "action": c["action"], "args": c["args"]} for c in fresh]
            island_cv.wait(timeout=min(1.0, end - now))


def island_result_post(cid, ok, message, data, now):
    with island_cv:
        known = any(c["id"] == cid for c in island["queue"])
        if not known and cid not in island["results"]:
            return False
        island["results"][cid] = {"ok": bool(ok), "message": str(message or "")[:500], "ts": now}
        if isinstance(data, (dict, list)) and len(json.dumps(data)) <= 12000:
            island["results"][cid]["data"] = data
        island["queue"] = [c for c in island["queue"] if c["id"] != cid]
        island_cv.notify_all()
    return True


def island_result_wait(cid, wait, clock=time.time):
    """El resultado de una orden; espera hasta `wait` s. None si aún no hay."""
    wait = max(0.0, min(float(wait), ISLAND_MAX_WAIT))
    end = clock() + wait
    with island_cv:
        while True:
            now = clock()
            _island_expire(now)
            if cid in island["results"]:
                return island["results"][cid]
            if now >= end:
                return None
            island_cv.wait(timeout=min(1.0, end - now))


def hour_of(now: float) -> int:
    """La hora local de `now` (de 22 a 6 Mochi duerme). Aparte para que los tests no dependan del huso horario."""
    return time.localtime(now).tm_hour


_jinx_cache = {"at": 0.0, "mood": None, "latch": {}}


def jinx_mood(now: float):
    """El ánimo de fondo de Jinx (un emote de Mochi o None), de su STATE.md, como mucho una lectura por minuto.
    Nunca falla: sin archivo, sin módulo o con un archivo raro es None (tranquila)."""
    if now - _jinx_cache["at"] < JINX_MOOD_TTL:
        return _jinx_cache["mood"]
    mood, latch = None, _jinx_cache["latch"]
    if mood_now is not None:
        try:
            st = parse_emotion_state(JINX_STATE_MD.read_text(encoding="utf-8"))
            mood, latch = mood_now(st, load_recent_events(JINX_EVENTS_LOG, now), now, latch,
                                   hour_of(now))
        except Exception:
            mood = None
    _jinx_cache.update(at=now, mood=mood, latch=latch)
    return mood


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
                "jinx_mood": jinx_mood(now),
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
        if self.path.startswith("/api/island/"):
            u = urlparse(self.path)
            q = parse_qs(u.query)
            try:
                wait = float((q.get("wait") or ["0"])[0])
            except ValueError:
                wait = 0.0
            if u.path == "/api/island/poll":
                if (q.get("device") or [ISLAND_DEVICE])[0] != ISLAND_DEVICE:
                    return self._send(400, {"error": "device debe ser pc"})
                return self._send(200, {"ok": True, "commands": island_poll(wait)})
            if u.path == "/api/island/result":
                try:
                    cid = int((q.get("id") or ["0"])[0])
                except ValueError:
                    return self._send(400, {"error": "id inválido"})
                r = island_result_wait(cid, wait)
                return self._send(200, {"ok": True, "done": r is not None, "result": r})
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
            if isinstance(b, dict) and b.get("device") in ALIASES:
                b["device"] = ALIASES[b["device"]]
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
            return self._send(200, {"ok": True, "active_device": active_device(now), "jinx_mood": jinx_mood(now)})
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
            active = active_device(now)
            # `watched`: hay alguien mirando un dispositivo (pc o un teléfono). Con "vga" (el respaldo) puede no
            # haber nadie delante del panel: quien manda el aviso debe mandarlo también por Telegram.
            return self._send(200, {"ok": True, "id": msg["id"],
                                   "active_device": active, "watched": active != "vga"})
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
        if self.path == "/api/island/command":
            b = self._body()
            if not isinstance(b, dict):
                return self._send(400, {"error": "json requerido"})
            code, body = island_submit(b.get("action"), b.get("args"), now)
            return self._send(code, body)
        if self.path == "/api/island/result":
            b = self._body(limit=16384)
            if not isinstance(b, dict) or not isinstance(b.get("id"), int):
                return self._send(400, {"error": "id requerido"})
            if not island_result_post(b["id"], b.get("ok"), b.get("message"), b.get("data"), now):
                return self._send(404, {"error": "id desconocido"})
            return self._send(200, {"ok": True})
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
