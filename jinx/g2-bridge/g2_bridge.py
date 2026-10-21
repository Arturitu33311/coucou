#!/usr/bin/env python3
"""g2_bridge — las gafas Even G2 hablan con Jinx. Servicio del servidor, solo en loopback (Tailscale Serve le pone HTTPS).

  GET  /health                         sin llave
  POST /api/g2/ask      {text, cid?}   -> {id, status: done|pending, text?, mode?}   (espera hasta ASK_WAIT)
  GET  /api/g2/answer?id=&wait=        -> lo mismo, para una pregunta que quedó pendiente
  POST /api/g2/voice    PCM crudo      -> {heard, id, status, ...}  (16 kHz mono s16le, g2_stt)
  GET  /api/g2/notices?since=&wait=    -> {notices: [{id,text,kind,source,ts}]}  (los avisos del servidor de presencia)
  POST /v1/chat/completions, GET /v1/models   compatible con OpenAI, para "Agent Configuration" de Even AI

Todo (menos /health) lleva `Authorization: Bearer <llave>` (o `X-G2-Key`); la llave sale de G2_KEY_PATH
(~/.config/jinx-g2/key, 0600; nunca en el repo). Las preguntas van por el gateway de Jinx (`jinx-chat`, op send, src
"g2"), así el hilo compartido las ve igual que las del teléfono. El gateway NO reparte esas respuestas: las recogen las
gafas. Si nadie recoge una en GRACE segundos (te quitaste las gafas), el puente pide `deliver` y va a la isla o a
Telegram como cualquier otra: nunca se pierde. Los avisos se LEEN sin tocarlos (no hay ack): las gafas son una
superficie más, no cambian quién es el dispositivo activo.
"""
import hmac
import json
import os
import secrets
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import g2_stt  # noqa: E402

HOME = os.path.expanduser("~")
PORT = int(os.environ.get("JINX_G2_PORT", "8898"))
KEY_PATH = os.environ.get("G2_KEY_PATH", os.path.join(HOME, ".config", "jinx-g2", "key"))
GATEWAY_CMD = os.environ.get("JINX_CHAT_CMD", os.path.join(HOME, "bin", "jinx-chat"))
PRESENCE_URL = os.environ.get("JINX_PRESENCE_BASE", "http://127.0.0.1:8897").rstrip("/")
PRESENCE_KEY_PATH = os.path.join(HOME, ".config", "mochi-presence", "key")

ASK_WAIT = 40          # lo que espera /ask antes de decir "pendiente" (las gafas reintentan con /answer)
ANSWER_WAIT_MAX = 30
OPENAI_WAIT = 25       # Even AI no espera eternamente: pasado esto, una frase corta y la respuesta se reparte luego
GRACE = 180            # una respuesta que nadie recogió en este tiempo se reparte por el camino normal
REPLY_MAX = 2000       # lo que se devuelve a las gafas (el hilo guarda el texto entero)
OPENAI_MAX = 700
TEXT_MAX = 2000
NOTICE_WAIT_MAX = 25
NOTICE_CAP = 20
HIDDEN_NOTICE_SOURCES = ("jinx-chat",)  # las respuestas a otras conversaciones no son avisos para las gafas
STILL_THINKING = "Sigo pensándolo; te lo mando a la isla o a Telegram cuando esté."


# ---------- el gateway ----------

def gateway_call(req, cmd=GATEWAY_CMD, run=subprocess.run, timeout=540):
    """Una petición al gateway (`jinx-chat`): la lista de eventos NDJSON. Lanza OSError si no arranca."""
    r = run([cmd], input=json.dumps(req), capture_output=True, text=True, timeout=timeout)
    out = []
    for line in (r.stdout or "").splitlines():
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if isinstance(d, dict):
            out.append(d)
    return out


def final_event(events):
    """(done|error|None, evento) de la última respuesta del gateway."""
    for ev in reversed(events):
        if ev.get("ev") in ("done", "error"):
            return ev
    return None


# ---------- las preguntas en curso ----------

class Jobs:
    """Una pregunta a Jinx en un hilo aparte, por cid. Idempotente por cid; guarda si alguien recogió la respuesta."""

    def __init__(self, gateway, now=time.time, spawn=None):
        self.gateway = gateway
        self.now = now
        self.lock = threading.Lock()
        self.cv = threading.Condition(self.lock)
        self.jobs = {}
        self.spawn = spawn or self._thread

    @staticmethod
    def _thread(fn):
        threading.Thread(target=fn, daemon=True).start()

    def submit(self, text, cid=None):
        cid = cid if isinstance(cid, str) and 0 < len(cid) <= 64 and cid.replace("-", "").replace("_", "").isalnum() \
            else "g2" + secrets.token_hex(6)
        with self.lock:
            if cid in self.jobs:
                return cid
            self.jobs[cid] = {"status": "pending", "ts": self.now(), "collected": False, "swept": False}
        self.spawn(lambda: self._run(cid, text))
        return cid

    def _run(self, cid, text):
        try:
            ev = final_event(self.gateway({"op": "send", "text": text, "cid": cid, "src": "g2"}))
        except Exception as e:  # noqa: BLE001 - gateway sin arrancar, tiempo agotado...
            ev = {"ev": "error", "error": "no pude hablar con Jinx (%s)" % type(e).__name__}
        with self.cv:
            j = self.jobs[cid]
            j["ts"] = self.now()
            if ev and ev.get("ev") == "done":
                j.update(status="done", text=str(ev.get("text", "")), mode=ev.get("mode"), mood=ev.get("mood"))
            else:
                j.update(status="error", error=str((ev or {}).get("error") or "Jinx no contestó"))
            self.cv.notify_all()

    def wait(self, cid, seconds):
        """El estado de la pregunta; espera hasta `seconds` a que acabe. None si no existe."""
        end = time.monotonic() + max(0.0, seconds)
        with self.cv:
            while True:
                j = self.jobs.get(cid)
                if j is None:
                    return None
                if j["status"] != "pending":
                    return dict(j)
                left = end - time.monotonic()
                if left <= 0:
                    return dict(j)
                self.cv.wait(min(left, 1.0))

    def collect(self, cid):
        with self.lock:
            if cid in self.jobs:
                self.jobs[cid]["collected"] = True

    def sweep(self):
        """Reparte las respuestas que nadie recogió a tiempo (una sola vez cada una). Devuelve los cid."""
        stale = []
        with self.lock:
            for cid, j in self.jobs.items():
                if j["status"] == "done" and not j["collected"] and not j["swept"] and self.now() - j["ts"] >= GRACE:
                    j["swept"] = True
                    stale.append(cid)
            for cid in [c for c, j in self.jobs.items() if self.now() - j["ts"] > 3600 and j["status"] != "pending"]:
                del self.jobs[cid]
        for cid in stale:
            try:
                self.gateway({"op": "deliver", "cid": cid})
            except Exception:  # noqa: BLE001 - se reintenta en la siguiente pasada
                with self.lock:
                    if cid in self.jobs:
                        self.jobs[cid]["swept"] = False
        return stale


def reply_view(cid, j, limit=REPLY_MAX):
    if j["status"] == "done":
        text = j["text"]
        out = {"id": cid, "status": "done", "text": text if len(text) <= limit else text[: limit - 1] + "…",
               "mode": j.get("mode")}
        if j.get("mood"):
            out["mood"] = j["mood"]
        return out
    if j["status"] == "error":
        return {"id": cid, "status": "error", "error": j.get("error", "")}
    return {"id": cid, "status": "pending"}


# ---------- avisos ----------

def presence_get(path, opener=urllib.request.urlopen, key_path=PRESENCE_KEY_PATH):
    headers = {}
    try:
        with open(key_path, encoding="utf-8") as f:
            headers["X-Mochi-Key"] = f.read().strip()
    except OSError:
        pass  # en el propio servidor basta con el loopback
    with opener(urllib.request.Request(PRESENCE_URL + path, headers=headers), timeout=6) as r:
        return json.loads(r.read() or b"{}")


def notices_since(since, fetch=presence_get, hidden=HIDDEN_NOTICE_SOURCES):
    """Los avisos nuevos del servidor de presencia, sin marcarlos leídos (GET del inbox, nunca ack)."""
    data = fetch("/api/mochi/inbox?since_id=%d" % since)
    out = []
    for m in data.get("messages", []):
        if not isinstance(m, dict) or not isinstance(m.get("id"), int) or m.get("source") in hidden:
            continue
        out.append({"id": m["id"], "text": str(m.get("text", ""))[:400], "kind": str(m.get("kind") or "info"),
                    "source": str(m.get("source") or ""), "ts": m.get("ts")})
    return out[-NOTICE_CAP:]


# ---------- HTTP ----------

def load_key(path=KEY_PATH):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read().strip()
    except OSError:
        return ""


def make_key(path=KEY_PATH):
    """Crea la llave si no existe (0600) y la devuelve."""
    key = load_key(path)
    if key:
        return key
    os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)
    key = secrets.token_urlsafe(32)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write(key + "\n")
    return key


def authorized(headers, key):
    if not key:
        return False
    got = headers.get("X-G2-Key") or ""
    auth = headers.get("Authorization") or ""
    if auth.lower().startswith("bearer "):
        got = auth[7:].strip()
    return hmac.compare_digest(got.encode(), key.encode())


def last_user_text(messages):
    if not isinstance(messages, list):
        return ""
    for m in reversed(messages):
        if isinstance(m, dict) and m.get("role") == "user":
            c = m.get("content")
            if isinstance(c, list):  # [{type: text, text: ...}]
                c = " ".join(p.get("text", "") for p in c if isinstance(p, dict))
            return " ".join(str(c or "").split())
    return ""


def clean_text(value):
    return " ".join(value.split())[:TEXT_MAX] if isinstance(value, str) else ""


class App:
    def __init__(self, jobs, key, stt=g2_stt.transcribe, notices=notices_since):
        self.jobs = jobs
        self.key = key
        self.stt = stt
        self.notices = notices


def make_handler(app):
    class H(BaseHTTPRequestHandler):
        server_version = "jinx-g2"

        def log_message(self, *a):  # silencio: nada de textos en los registros
            pass

        def _send(self, code, body, ctype="application/json"):
            raw = body if isinstance(body, bytes) else json.dumps(body, ensure_ascii=False).encode()
            self.send_response(code)
            self.send_header("Content-Type", ctype + "; charset=utf-8" if ctype.startswith("text") else ctype)
            self.send_header("Content-Length", str(len(raw)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()
            self.wfile.write(raw)

        def do_OPTIONS(self):  # la app de las gafas vive en un WebView: pregunta antes (CORS)
            self.send_response(204)
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Access-Control-Allow-Headers", "Authorization, X-G2-Key, Content-Type")
            self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
            self.send_header("Content-Length", "0")
            self.end_headers()

        def _body(self, limit):
            try:
                n = int(self.headers.get("Content-Length") or 0)
            except ValueError:
                n = 0
            if n < 0 or n > limit:
                return None
            return self.rfile.read(n)

        def _json(self):
            raw = self._body(64 * 1024)
            try:
                d = json.loads(raw or b"{}") if raw is not None else None
            except ValueError:
                d = None
            return d if isinstance(d, dict) else None

        def _ask_reply(self, text, cid, wait, limit=REPLY_MAX):
            cid = app.jobs.submit(text, cid)
            j = app.jobs.wait(cid, wait)
            if j and j["status"] == "done":
                app.jobs.collect(cid)
            return reply_view(cid, j, limit) if j else {"id": cid, "status": "error", "error": "desconocida"}

        def do_GET(self):
            u = urlparse(self.path)
            q = parse_qs(u.query)
            if u.path == "/health":
                return self._send(200, {"ok": True})
            if not authorized(self.headers, app.key):
                return self._send(401, {"error": "llave"})
            if u.path == "/api/g2/answer":
                cid = (q.get("id") or [""])[0]
                wait = _num(q.get("wait"), 0, ANSWER_WAIT_MAX)
                j = app.jobs.wait(cid, wait)
                if j is None:
                    return self._send(404, {"error": "no hay pregunta con ese id"})
                if j["status"] == "done":
                    app.jobs.collect(cid)
                return self._send(200, reply_view(cid, j))
            if u.path == "/api/g2/notices":
                since = int(_num(q.get("since"), 0, 10 ** 12))
                end = time.monotonic() + _num(q.get("wait"), 0, NOTICE_WAIT_MAX)
                while True:
                    try:
                        found = app.notices(since)
                    except Exception:  # noqa: BLE001 - presencia caída: sin avisos, no es un error de las gafas
                        found = []
                    if found or time.monotonic() >= end:
                        return self._send(200, {"notices": found})
                    time.sleep(1.5)
            if u.path == "/v1/models":
                return self._send(200, {"object": "list", "data": [{"id": "jinx", "object": "model",
                                                                    "owned_by": "jinx"}]})
            self._send(404, {"error": "no existe"})

        def do_POST(self):
            u = urlparse(self.path)
            if not authorized(self.headers, app.key):
                return self._send(401, {"error": "llave"})
            if u.path == "/api/g2/ask":
                d = self._json()
                text = clean_text((d or {}).get("text"))
                if not text:
                    return self._send(400, {"error": "falta text"})
                return self._send(200, self._ask_reply(text, d.get("cid"), ASK_WAIT))
            if u.path == "/api/g2/voice":
                pcm = self._body(g2_stt.MAX_BYTES + 1024)
                if pcm is None:
                    return self._send(413, {"error": "audio demasiado largo"})
                try:
                    heard = app.stt(pcm)
                except g2_stt.SttBadAudio as e:
                    return self._send(400, {"error": str(e)})
                except g2_stt.SttUnavailable as e:
                    return self._send(503, {"error": str(e)})
                if not heard:
                    return self._send(200, {"heard": "", "status": "empty"})
                out = self._ask_reply(clean_text(heard), None, ASK_WAIT)
                out["heard"] = heard
                return self._send(200, out)
            if u.path == "/v1/chat/completions":
                d = self._json()
                text = clean_text(last_user_text((d or {}).get("messages")))
                if not text:
                    return self._send(400, {"error": {"message": "falta un mensaje de usuario"}})
                out = self._ask_reply(text, None, OPENAI_WAIT, OPENAI_MAX)
                content = out.get("text") if out["status"] == "done" else (
                    STILL_THINKING if out["status"] == "pending" else out.get("error", "Jinx no pudo contestar"))
                return self._send(*_openai(content, bool((d or {}).get("stream"))))
            self._send(404, {"error": "no existe"})

    return H


def _num(values, low, high):
    try:
        return max(low, min(high, float((values or ["0"])[0])))
    except ValueError:
        return low


def _openai(content, stream):
    ident = "chatcmpl-" + secrets.token_hex(6)
    now = int(time.time())
    if stream:
        chunk = {"id": ident, "object": "chat.completion.chunk", "created": now, "model": "jinx",
                 "choices": [{"index": 0, "delta": {"role": "assistant", "content": content}, "finish_reason": None}]}
        end = {"id": ident, "object": "chat.completion.chunk", "created": now, "model": "jinx",
               "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}
        body = ("data: %s\n\ndata: %s\n\ndata: [DONE]\n\n" % (json.dumps(chunk, ensure_ascii=False),
                                                              json.dumps(end))).encode()
        return 200, body, "text/event-stream"
    return 200, {"id": ident, "object": "chat.completion", "created": now, "model": "jinx",
                 "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
                              "finish_reason": "stop"}],
                 "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}}


def serve(port=PORT):
    key = make_key()
    jobs = Jobs(gateway_call)

    def sweeper():
        while True:
            time.sleep(30)
            jobs.sweep()
    threading.Thread(target=sweeper, daemon=True).start()
    srv = ThreadingHTTPServer(("127.0.0.1", port), make_handler(App(jobs, key)))
    print("jinx g2-bridge en 127.0.0.1:%d (llave: %s)" % (port, KEY_PATH), flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    serve()
