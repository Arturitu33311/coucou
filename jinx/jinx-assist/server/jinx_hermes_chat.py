#!/usr/bin/env python3
"""Puente de texto hacia la Jinx real (Hermes) para el asistente del teléfono y la laptop.

Corre EN EL SERVER (`~/.hermes/scripts/jinx_hermes_chat.py`, con el python del venv de Hermes), lo invoca
`jinx-chat` de la laptop por ssh. Lee UNA línea JSON de stdin y escribe UNA línea JSON:

  {"op":"send","text":"…"}  -> {"ev":"done","text":"…","session_id":"…"} | {"ev":"error","msg":"…"}
  {"op":"history","n":40}   -> {"ev":"history","items":[{"id","role","text","ts"}…]}
  {"op":"approvals"}        -> {"ev":"approvals","items":[{"run_id","request_id","command","description","choices","ts"}…]}
  {"op":"approve","run_id","request_id","choice"} -> {"ev":"approved","choice":"…"} | {"ev":"error","msg":"…"}

Aprobaciones: `send` va por `/api/sessions/{id}/chat/stream`, que es el único camino de sesión donde Hermes
PREGUNTA antes de un comando peligroso (el `/chat` normal no tiene a quién preguntar y aplica
`approvals.unattended_mode`, que por defecto niega). Mientras el turno espera, la pregunta queda en
APPROVALS_FILE; el teléfono (o el PC) la lee con `approvals`, Alan la confirma con su huella y `approve` la
contesta con `POST /v1/runs/{run_id}/approval`. Solo se puede contestar lo que está en ese archivo. Si este
Hermes no tiene el endpoint de stream, `send` vuelve al `/chat` de siempre.

Habla con la API de Hermes (`/api/sessions/{id}/chat`), dentro de la sesión de Telegram más reciente, así que
la Jinx real contesta con SU contexto (memoria, herramientas, lo hablado por Telegram). La llave de la API
se resuelve aquí (bóveda) y no sale de este proceso: nunca se imprime ni se envía a otro lado.
"""
import fcntl
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request

CANDIDATE_BASES = ("http://127.0.0.1:8642", "http://100.92.254.125:8642")
SEND_TIMEOUT = 75
READ_TIMEOUT = 20
# Hermes espera una aprobación hasta `approvals.timeout` (300 s por defecto); pasado esto la pregunta ya no vale.
APPROVAL_LIFE = 330
# Un turno sin preguntas pendientes no pasa de esto (como el /chat de antes); con una, espera a Alan.
TURN_LIMIT = 80
APPROVALS_FILE = os.environ.get(
    "JINX_APPROVALS_FILE", os.path.expanduser("~/.hermes/state/coucou/approvals.json"))
CHOICES = ("once", "session", "always", "deny")
RUN_ID_RE = re.compile(r"^[A-Za-z0-9_-]{1,100}$")
REQUEST_ID_RE = re.compile(r"^[A-Za-z0-9_.:-]{0,256}$")


class ApiError(Exception):
    pass


def redact(text, key):
    s = str(text)
    return s.replace(key, "<llave>") if key else s


def resolve_key():
    try:
        from vaultknox.autonomous_secrets import get_secret  # type: ignore
        k = get_secret("API_SERVER_KEY") or ""
        if k:
            return k
    except Exception:  # noqa: BLE001 - se prueba el entorno enseguida
        pass
    return os.environ.get("API_SERVER_KEY", "")


class HttpApi:
    def __init__(self, key, timeout_read=READ_TIMEOUT, timeout_send=SEND_TIMEOUT):
        self.key = key
        self.timeout_read = timeout_read
        self.timeout_send = timeout_send
        self.base = None

    def _pick_base(self):
        if self.base:
            return self.base
        last = "sin respuesta"
        for b in CANDIDATE_BASES:
            try:
                with urllib.request.urlopen(b + "/health", timeout=4) as r:
                    if r.status == 200:
                        self.base = b
                        return b
            except Exception as e:  # noqa: BLE001
                last = str(e)
        raise ApiError("la API de Hermes no responde (%s)" % redact(last, self.key))

    def _request(self, method, path, body=None, timeout=READ_TIMEOUT):
        url = self._pick_base() + path
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(url, data=data, method=method, headers={
            "Authorization": "Bearer " + self.key, "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.loads(r.read() or b"{}")
        except urllib.error.HTTPError as e:
            raise ApiError("HTTP %s" % e.code)
        except Exception as e:  # noqa: BLE001
            raise ApiError(redact("%s: %s" % (type(e).__name__, e), self.key))

    def get(self, path):
        return self._request("GET", path, timeout=self.timeout_read)

    def post(self, path, body):
        return self._request("POST", path, body, timeout=self.timeout_send)

    def stream(self, path, body):
        """Los eventos SSE de un POST, como (nombre, datos) uno por uno. Lanza ApiError si no se puede abrir."""
        url = self._pick_base() + path
        req = urllib.request.Request(url, data=json.dumps(body).encode(), method="POST", headers={
            "Authorization": "Bearer " + self.key, "Content-Type": "application/json",
            "Accept": "text/event-stream"})
        try:
            r = urllib.request.urlopen(req, timeout=self.timeout_send)
        except urllib.error.HTTPError as e:
            raise ApiError("HTTP %s" % e.code)
        except Exception as e:  # noqa: BLE001
            raise ApiError(redact("%s: %s" % (type(e).__name__, e), self.key))
        with r:
            try:
                yield from sse_events(r)
            except ApiError:
                raise
            except Exception as e:  # noqa: BLE001 - la conexión puede romperse a medias
                raise ApiError(redact("se cortó el stream: %s: %s" % (type(e).__name__, e), self.key))


def sse_events(lines):
    """(nombre, datos) de cada evento de un stream SSE (líneas en bytes). Un comentario (el keepalive de Hermes,
    cada 10 s) sale como ("keepalive", {}) para que quien lee pueda mirar el reloj; la basura se salta. Pura."""
    name, data = "", []
    for raw in lines:
        line = raw.decode("utf-8", "replace").rstrip("\r\n") if isinstance(raw, bytes) else raw.rstrip("\r\n")
        if not line:
            if data:
                try:
                    payload = json.loads("\n".join(data))
                except ValueError:
                    payload = None
                if isinstance(payload, dict):
                    yield name or payload.get("event") or "", payload
            name, data = "", []
        elif line.startswith(":"):
            yield "keepalive", {}
        elif line.startswith("event:"):
            name = line[6:].strip()
        elif line.startswith("data:"):
            data.append(line[5:].lstrip(" ") if line[5:6] == " " else line[5:])


class ApprovalStore:
    """Las preguntas de Hermes que esperan respuesta, en un archivo JSON con candado (varios procesos: el `send`
    que espera y el `approve` que contesta). Las viejas (más de APPROVAL_LIFE) se tiran al leer."""

    def __init__(self, path=APPROVALS_FILE, clock=time.time):
        self.path = path
        self.clock = clock

    def _edit(self, change):
        os.makedirs(os.path.dirname(self.path) or ".", exist_ok=True)
        fd = os.open(self.path, os.O_RDWR | os.O_CREAT, 0o600)
        with os.fdopen(fd, "r+") as f:
            fcntl.flock(f, fcntl.LOCK_EX)
            try:
                items = json.loads(f.read() or "[]")
            except ValueError:
                items = []
            now = self.clock()
            items = [d for d in items if isinstance(d, dict) and now - float(d.get("ts") or 0) < APPROVAL_LIFE]
            out = change(items)
            f.seek(0)
            f.truncate()
            f.write(json.dumps(items, ensure_ascii=False))
            return out

    def add(self, item):
        def change(items):
            items[:] = [d for d in items if not (d["run_id"] == item["run_id"] and d["request_id"] == item["request_id"])]
            items.append(item)
        self._edit(change)

    def list(self):
        return self._edit(lambda items: [dict(d) for d in items])

    def find(self, run_id, request_id):
        return self._edit(lambda items: next(
            (dict(d) for d in items if d["run_id"] == run_id and d["request_id"] == request_id), None))

    def remove(self, run_id, request_id=None):
        def change(items):
            items[:] = [d for d in items if not (d["run_id"] == run_id and (request_id is None or d["request_id"] == request_id))]
        self._edit(change)


def clip(text, n):
    t = " ".join(str(text or "").split())
    return t if len(t) <= n else t[: n - 1] + "…"


def clip_keep_lines(text, n):
    """Como clip, pero un comando conserva sus saltos de línea: es lo que Alan tiene que leer antes de aprobar."""
    t = str(text or "").strip()
    return t if len(t) <= n else t[: n - 1] + "…"


def approval_item(data, clock=time.time):
    """Lo que se guarda (y se le enseña a Alan) de un `approval.request`, o None si no trae run_id. Pura."""
    run_id = str(data.get("run_id") or "")
    request_id = str(data.get("request_id") or "")
    if not RUN_ID_RE.match(run_id) or not REQUEST_ID_RE.match(request_id):
        return None
    raw = data.get("choices")
    choices = [c for c in CHOICES if isinstance(raw, list) and c in raw] or ["once", "deny"]
    if "deny" not in choices:
        choices.append("deny")
    return {"run_id": run_id, "request_id": request_id, "command": clip_keep_lines(data.get("command"), 2000),
            "description": clip(data.get("description"), 200), "choices": choices, "ts": clock()}


PRESENCE_URL = os.environ.get("JINX_PRESENCE_URL", "http://127.0.0.1:8897/api/mochi/message")


def announce_approval(item, url=PRESENCE_URL, opener=urllib.request.urlopen):
    """Dice en el dispositivo que Alan está usando (el servidor de presencia, aquí mismo) que Jinx espera su
    permiso, para que vaya a la pestaña de Jinx. Nunca falla: sin presencia, la pregunta sigue en el archivo."""
    text = "Jinx necesita tu permiso: %s" % (item.get("description") or "un comando")
    body = json.dumps({"text": clip(text, 200), "kind": "alert", "source": "jinx", "mood": None}).encode()
    try:
        req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"})
        with opener(req, timeout=3):
            pass
    except Exception:  # noqa: BLE001
        pass


BRIDGE_LOG = os.environ.get("JINX_BRIDGE_LOG", os.path.expanduser("~/.hermes/state/coucou/bridge.log"))


def log_line(line, path=None):
    """Una línea de diagnóstico SIN contenido (nombres de eventos, tiempos, motivos). Nunca falla."""
    try:
        path = path or BRIDGE_LOG
        os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
        with open(path, "a", encoding="utf-8") as f:
            f.write("%s %s\n" % (time.strftime("%Y-%m-%dT%H:%M:%S"), line))
    except Exception:  # noqa: BLE001
        pass


class StreamNotOpened(ApiError):
    """El stream no llegó a abrirse (ningún evento): el mensaje no entró a Hermes, se puede usar /chat."""


def _latest_reply(api, sid, since):
    """La última respuesta de Jinx en la sesión escrita después de `since` (segundos), o None."""
    try:
        count = next((int(x.get("message_count") or 0) for x in _sessions(api) if x.get("id") == sid), 0)
        d = api.get("/api/sessions/%s/messages?limit=6&offset=%d" % (sid, max(0, count - 6)))
    except (ApiError, ValueError, TypeError):
        return None
    raw = d.get("messages") or d.get("data") or d.get("items") or []
    best = None
    for m in raw:
        if m.get("role") != "assistant" or m.get("tool_calls"):
            continue
        try:
            ts = float(m.get("timestamp") or 0)
        except (TypeError, ValueError):
            ts = 0.0
        text = _text_of(m.get("content")).strip()
        if text and ts >= since and (best is None or ts > best[0]):
            best = (ts, text)
    return best[1] if best else None


def _send_stream(api, sid, text, store, clock=time.monotonic, wall=time.time, pause=time.sleep):
    """Un turno por el stream de la sesión: guarda las preguntas mientras espera. (texto, session_id).
    Tope: TURN_LIMIT sin preguntas; cada pregunta lo alarga a APPROVAL_LIFE + TURN_LIMIT desde que llegó.
    El texto sale de `assistant.completed`, si no de `run.completed`, si no de los deltas; si el stream
    termina sin texto (p. ej. otro proceso tomó el turno: `run.queued`), se lee de la sesión."""
    answer, failed, runs, deltas, seen = None, None, set(), [], {}
    started_wall = wall() - 5
    deadline = clock() + TURN_LIMIT
    queued = False
    try:
        try:
            events = api.stream("/api/sessions/%s/chat/stream" % sid, {"message": text})
            for name, data in events:
                seen[name] = seen.get(name, 0) + 1
                if clock() > deadline:
                    raise ApiError("Hermes tardó demasiado")
                if data.get("run_id"):
                    runs.add(str(data["run_id"]))
                if name == "approval.request":
                    item = approval_item(data)
                    if item:
                        store.add(item)
                        announce_approval(item)
                        deadline = max(deadline, clock() + APPROVAL_LIFE + TURN_LIMIT)
                elif name == "approval.responded":
                    store.remove(str(data.get("run_id") or ""), str(data.get("request_id") or "") or None)
                elif name == "assistant.completed":
                    answer = _text_of(data.get("content")) or answer
                    sid = data.get("session_id") or sid
                elif name == "run.completed":
                    answer = answer or _text_of(data.get("output") or data.get("content"))
                elif name in ("assistant.delta", "message.delta"):
                    deltas.append(str(data.get("delta") or ""))
                elif name == "run.queued":
                    queued = True
                elif name in ("run.failed", "error"):
                    failed = str(data.get("error") or data.get("message") or "Hermes no pudo terminar")
                elif name in ("run.cancelled", "run.interrupted"):
                    failed = failed or "el turno se canceló"
                elif name == "done":
                    break
        except ApiError as e:
            if not seen and not isinstance(e, StreamNotOpened):
                raise StreamNotOpened(str(e))
            raise
    finally:
        for run in runs:
            store.remove(run)
        log_line("send stream events=%s queued=%s answer=%s failed=%s" % (
            ",".join("%s:%d" % kv for kv in sorted(seen.items())) or "-", queued,
            bool(answer or "".join(deltas).strip()), bool(failed)))
    answer = (answer or "").strip() or "".join(deltas).strip()
    if not answer and not failed:
        # El turno lo terminó (o lo sigue) otro proceso: la respuesta queda en la sesión.
        while not answer and clock() < deadline:
            answer = _latest_reply(api, sid, started_wall) or ""
            if not answer:
                pause(3)
        log_line("send read-back found=%s queued=%s" % (bool(answer), queued))
    if not answer and failed:
        raise ApiError(failed)
    return answer, sid


def pick_telegram_session(sessions):
    """(id, message_count) de la sesión de Telegram viva más reciente, o None."""
    live = [s for s in sessions
            if s.get("source") == "telegram" and not s.get("archived") and not s.get("hidden") and s.get("id")]
    if not live:
        return None
    best = max(live, key=lambda s: float(s.get("last_active") or 0))
    return best["id"], int(best.get("message_count") or 0)


def _sessions(api):
    d = api.get("/api/sessions?limit=50")
    return d.get("sessions") or d.get("data") or d.get("items") or []


def _text_of(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(p.get("text", "") for p in content if isinstance(p, dict) and p.get("type") in ("text", None))
    return ""


def handle(req, api, store=None):
    op = req.get("op") if isinstance(req, dict) else None
    store = store or ApprovalStore()
    try:
        if op == "send":
            text = req.get("text")
            if not isinstance(text, str) or not text.strip():
                return {"ev": "error", "msg": "send necesita text"}
            picked = pick_telegram_session(_sessions(api))
            if not picked:
                return {"ev": "error", "msg": "no hay una sesión de Telegram activa en Hermes"}
            sid = picked[0]
            try:
                answer, sid = _send_stream(api, sid, text, store)
            except StreamNotOpened as e:
                # El stream no abrió (un Hermes sin ese endpoint, o lo rechazó): el mensaje no entró, así que
                # va por el /chat de siempre (sin aprobaciones, como antes).
                log_line("send stream-not-opened (%s): /chat" % e)
                res = api.post("/api/sessions/%s/chat" % sid, {"message": text})
                answer = _text_of((res.get("message") or {}).get("content"))
                sid = res.get("session_id") or sid
            if not answer.strip():
                return {"ev": "error", "msg": "Hermes no devolvió texto"}
            return {"ev": "done", "text": answer.strip(), "session_id": sid}
        if op == "approvals":
            items = [{k: d[k] for k in ("run_id", "request_id", "command", "description", "choices", "ts")}
                     for d in store.list()]
            return {"ev": "approvals", "items": items}
        if op == "approve":
            run_id, request_id, choice = req.get("run_id"), req.get("request_id") or "", req.get("choice")
            if not isinstance(run_id, str) or not RUN_ID_RE.match(run_id) or not isinstance(request_id, str) \
                    or not REQUEST_ID_RE.match(request_id):
                return {"ev": "error", "msg": "approve necesita run_id y request_id"}
            pending = store.find(run_id, request_id)
            if pending is None:
                return {"ev": "error", "msg": "esa aprobación ya no está pendiente"}
            if choice not in pending["choices"]:
                return {"ev": "error", "msg": "opción no válida para esta aprobación"}
            body = {"choice": choice}
            if request_id:
                body["request_id"] = request_id
            try:
                api.post("/v1/runs/%s/approval" % run_id, body)
            except ApiError as e:
                if str(e) == "HTTP 409":
                    store.remove(run_id, request_id)
                    return {"ev": "error", "msg": "esa aprobación ya no está pendiente"}
                raise
            store.remove(run_id, request_id)
            return {"ev": "approved", "choice": choice}
        if op == "history":
            n = req.get("n", 40)
            n = n if isinstance(n, int) and 0 < n <= 200 else 40
            picked = pick_telegram_session(_sessions(api))
            if not picked:
                return {"ev": "history", "items": []}
            sid, count = picked
            offset = max(0, count - n)
            d = api.get("/api/sessions/%s/messages?limit=%d&offset=%d" % (sid, n, offset))
            raw = d.get("messages") or d.get("data") or d.get("items") or []
            items = []
            for m in raw:
                role = m.get("role")
                if role not in ("user", "assistant") or m.get("tool_calls") or m.get("tool_call_id"):
                    continue
                text = _text_of(m.get("content")).strip()
                if not text:
                    continue
                # "[[DISPARADOR AUTOMÁTICO DEL SISTEMA …]]": the system wakes Jinx up, Alan wrote nothing.
                if role == "user" and text.startswith("[["):
                    continue
                try:
                    ts = float(m.get("timestamp") or 0)
                except (TypeError, ValueError):
                    ts = 0.0
                items.append({"id": str(m.get("id")), "role": role, "text": text, "ts": ts})
            return {"ev": "history", "items": items}
        return {"ev": "error", "msg": "op desconocida"}
    except ApiError as e:
        return {"ev": "error", "msg": str(e)}


def main():
    raw = sys.stdin.readline()
    try:
        req = json.loads(raw) if raw.strip() else {}
    except ValueError:
        print(json.dumps({"ev": "error", "msg": "petición no es JSON"}))
        return 0
    key = resolve_key()
    if not key:
        print(json.dumps({"ev": "error", "msg": "no pude resolver la llave de la API de Hermes"}))
        return 0
    out = handle(req, HttpApi(key))
    print(json.dumps(out, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
