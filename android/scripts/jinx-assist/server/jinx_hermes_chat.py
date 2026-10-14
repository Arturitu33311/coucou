#!/usr/bin/env python3
"""Puente de texto hacia la Jinx real (Hermes) para el asistente del teléfono y la laptop.

Corre EN EL SERVER (`~/.hermes/scripts/jinx_hermes_chat.py`, con el python del venv de Hermes), lo invoca
`jinx-chat` de la laptop por ssh. Lee UNA línea JSON de stdin y escribe UNA línea JSON:

  {"op":"send","text":"…"}  -> {"ev":"done","text":"…","session_id":"…"} | {"ev":"error","msg":"…"}
  {"op":"history","n":40}   -> {"ev":"history","items":[{"id","role","text","ts"}…]}

Habla con la API de Hermes (`/api/sessions/{id}/chat`), dentro de la sesión de Telegram más reciente, así que
la Jinx real contesta con SU contexto (memoria, herramientas, lo hablado por Telegram). La llave de la API
se resuelve aquí (bóveda) y no sale de este proceso: nunca se imprime ni se envía a otro lado.
"""
import json
import os
import sys
import urllib.error
import urllib.request

CANDIDATE_BASES = ("http://127.0.0.1:8642", "http://100.92.254.125:8642")
SEND_TIMEOUT = 75
READ_TIMEOUT = 20


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


def handle(req, api):
    op = req.get("op") if isinstance(req, dict) else None
    try:
        if op == "send":
            text = req.get("text")
            if not isinstance(text, str) or not text.strip():
                return {"ev": "error", "msg": "send necesita text"}
            picked = pick_telegram_session(_sessions(api))
            if not picked:
                return {"ev": "error", "msg": "no hay una sesión de Telegram activa en Hermes"}
            sid = picked[0]
            res = api.post("/api/sessions/%s/chat" % sid, {"message": text})
            answer = _text_of((res.get("message") or {}).get("content"))
            if not answer.strip():
                return {"ev": "error", "msg": "Hermes no devolvió texto"}
            return {"ev": "done", "text": answer.strip(), "session_id": res.get("session_id") or sid}
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
