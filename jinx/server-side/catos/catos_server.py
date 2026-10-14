#!/home/alan/.hermes/hermes-agent/venv/bin/python3
"""Hermes Cat OS V2 — dashboard móvil de Jinx para Alan.

Sirve un dashboard oscuro (estilo Jarvis) accesible solo por Tailscale.
Sin dependencias fuera del venv de Hermes (yaml, croniter ya presentes).

Endpoints:
  GET  /            → HTML del dashboard
  GET  /api/status  → JSON agregado (emociones, patrones, memorias, servicios,
                      gasto OpenRouter, crons, modo ocupado)
  POST /api/busy    → {"mode": "busy"|"normal"} — toggle via admin_cli de andorina
"""

import hashlib
import json
import os
import re
import sqlite3
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import yaml

try:
    from croniter import croniter
except ImportError:
    croniter = None

BIND_IP = os.environ.get("CATOS_BIND", "100.92.254.125")  # tailscale0 — nunca 0.0.0.0
PORT = int(os.environ.get("CATOS_PORT", "8899"))


def host_allowed(host_header, port):
    """Solo los nombres con los que se llega a este equipo: cualquier otro `Host` es DNS rebinding."""
    if not host_header:
        return False
    host, _, p = host_header.strip().lower().rpartition(":")
    if not host or p != str(port):
        return False
    return host in (BIND_IP, "127.0.0.1", "localhost", "server") or host.endswith(".ts.net")


def origin_allowed(origin, host_header):
    """Sin Origin = cliente que no es navegador (curl, scripts). Con Origin, debe ser esta misma página."""
    if origin is None:
        return True
    return bool(host_header) and origin.strip().lower() == "http://" + host_header.strip().lower()

VTUBER_ROOT = "http://127.0.0.1:8910"     # Open-LLM-VTuber (avatar, emoción)
# OJO: el gateway NO escucha en localhost. Desde el endurecimiento del
# 2026-07-22 (api_server 0.0.0.0/localhost → Tailscale-only) está bindeado
# SOLO en la IP de Tailscale del server. GATEWAY_ROOT apuntaba a
# "http://127.0.0.1:8642" y por eso la sección "Sesión unificada" del
# dashboard venía fallando en silencio con
# "urlopen error [Errno 111] Connection refused" (corregido 2026-07-23,
# mismo patrón que noche_guardian.py).
GATEWAY_ROOT = "http://100.92.254.125:8642"  # Hermes Gateway (sesiones) — IP Tailscale, no localhost
GATEWAY_SESSION_KEY = "agent:main:telegram:dm:6528281557"  # sesión unificada de Jinx

HERMES = Path.home() / ".hermes"
STATE_MD = HERMES / "STATE.md"
PATTERNS = HERMES / "state" / "patterns.json"
QUEUE = HERMES / "state" / "proactive_queue.json"
BUSY = HERMES / "state" / "busy_mode.json"
MNEMO_DB = HERMES / "mnemosyne" / "data" / "mnemosyne.db"
CRON_JOBS = HERMES / "cron" / "jobs.json"
SECRETS_BIN = str(HERMES / "hermes-agent" / "venv" / "bin" / "hermes-secrets")
ADMIN_CLI = HERMES / "skills" / "andorina" / "scripts" / "utils" / "admin_cli.py"
VENV_PY = str(HERMES / "hermes-agent" / "venv" / "bin" / "python3")

HTML_PATH = Path(__file__).parent / "dashboard.html"

# ── secrets (solo en memoria; nunca se exponen en /api) ──────────────────────

def _get_secret(name):
    try:
        r = subprocess.run([SECRETS_BIN, "get", name], capture_output=True,
                           text=True, timeout=15)
        v = r.stdout.strip()
        return v or None
    except Exception:
        return None

OR_KEYS = {}  # etiqueta → api key
GATEWAY_KEY = None  # API_SERVER_KEY del hermes-gateway (sesiones)

def load_keys():
    global GATEWAY_KEY
    for label, name in (("jinx", "OPENROUTER_API_KEY"),):
        k = _get_secret(name)
        if k:
            OR_KEYS[label] = k
    GATEWAY_KEY = _get_secret("API_SERVER_KEY")

# ── helpers de datos ─────────────────────────────────────────────────────────

def read_emotion():
    try:
        txt = STATE_MD.read_text(encoding="utf-8")
        m = re.match(r"^---\n(.*?)\n---", txt, re.S)
        if not m:
            return {}
        fm = yaml.safe_load(m.group(1)) or {}
        e = fm.get("emotion_state", {})
        base = e.get("baselines", {})
        return {
            "affection": e.get("affection"),
            "trust": e.get("trust"),
            "possessiveness": e.get("possessiveness"),
            "patience": e.get("patience"),
            # dimensiones nuevas (2026-07-03)
            "energy": e.get("energy"),
            "pride": e.get("pride"),
            "jealousy": e.get("jealousy"),
            "worry": e.get("worry"),
            "satisfaction": e.get("satisfaction"),
            "boredom": e.get("boredom"),
            "ops_streak": e.get("ops_streak"),
            "emotion_score": e.get("emotion_score"),
            "current_emotion": e.get("current_emotion"),
            "last_trigger": e.get("last_raw_trigger_type") or e.get("last_trigger_type"),
            "last_trigger_semantic": e.get("last_trigger_type"),
            "last_update": e.get("last_update"),
            "baselines": base,
        }
    except Exception as ex:
        return {"error": str(ex)}


def read_patterns():
    try:
        d = json.loads(PATTERNS.read_text(encoding="utf-8"))
        return {"updated_at": d.get("updated_at"),
                "active": d.get("active", []),
                "cleared": [p.get("id") for p in d.get("cleared", []) if isinstance(p, dict)]}
    except Exception as ex:
        return {"error": str(ex)}


def read_queue():
    try:
        d = json.loads(QUEUE.read_text(encoding="utf-8"))
        items = d.get("items", [])
        pending = [it for it in items if isinstance(it, dict) and it.get("status") == "pending"]
        pending.sort(key=lambda it: it.get("created_at", ""))
        return {
            "updated_at": d.get("updated_at"),
            "pending_count": len(pending),
            "pending": pending,
        }
    except FileNotFoundError:
        return {"updated_at": None, "pending_count": 0, "pending": []}
    except Exception as ex:
        return {"error": str(ex)}


def read_busy():
    try:
        d = json.loads(BUSY.read_text(encoding="utf-8"))
        return {"mode": d.get("mode", "normal"),
                "reason": d.get("reason"),
                "entered_busy_at": d.get("entered_busy_at")}
    except Exception as ex:
        return {"mode": "unknown", "error": str(ex)}


def read_memories(n=10):
    try:
        con = sqlite3.connect(f"file:{MNEMO_DB}?mode=ro", uri=True, timeout=3)
        rows = con.execute(
            "SELECT content, source, importance, created_at FROM memories "
            "ORDER BY datetime(created_at) DESC LIMIT ?", (n,)).fetchall()
        con.close()
        return [{"content": r[0][:220], "source": r[1], "importance": r[2],
                 "created_at": r[3]} for r in rows]
    except Exception as ex:
        return [{"content": f"error: {ex}", "source": "catos", "importance": 0,
                 "created_at": None}]


def _user_unit_active(unit):
    try:
        r = subprocess.run(["systemctl", "--user", "is-active", unit],
                           capture_output=True, text=True, timeout=5)
        return r.stdout.strip()
    except Exception as ex:
        return f"error: {ex}"


def read_services():
    out = {"containers": [], "hermes_gateway": "unknown"}
    try:
        r = subprocess.run(["docker", "ps", "-a", "--format",
                            "{{.Names}}\t{{.Status}}"],
                           capture_output=True, text=True, timeout=10)
        interesting = ("big-bear-home-assistant", "piper", "whisper", "pihole")
        for line in r.stdout.splitlines():
            name, _, status = line.partition("\t")
            if any(k in name for k in interesting):
                out["containers"].append({
                    "name": name,
                    "status": status,
                    "up": status.startswith("Up"),
                })
    except Exception as ex:
        out["containers_error"] = str(ex)
    try:
        r = subprocess.run(["systemctl", "is-active", "hermes-gateway"],
                           capture_output=True, text=True, timeout=5)
        out["hermes_gateway"] = r.stdout.strip()
    except Exception:
        pass
    # servicios de usuario (avatar Live2D + cara/push-to-talk + este dashboard)
    out["user_units"] = {
        "open-llm-vtuber.service": _user_unit_active("open-llm-vtuber.service"),
        "hermes-jinx-face-ssl.service": _user_unit_active("hermes-jinx-face-ssl.service"),
        "hermes-catos.service": _user_unit_active("hermes-catos.service"),
    }
    return out


def _http_get_json(url, headers=None, timeout=5):
    import urllib.request
    req = urllib.request.Request(url, headers=headers or {})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode())


def read_dominant_expression():
    """Expresión Live2D dominante actual (:8910/jinx/emotion)."""
    try:
        d = _http_get_json(f"{VTUBER_ROOT}/jinx/emotion", timeout=4)
        return {"dominant_now": d.get("dominant_now"), "clients": d.get("clients")}
    except Exception as ex:
        return {"error": str(ex)[:150]}


def read_wakeword():
    """No hay endpoint HTTP propio: el filtro vive en-proceso en open-llm-vtuber."""
    state = _user_unit_active("open-llm-vtuber.service")
    return {"listening": state == "active", "via": "open-llm-vtuber.service"}


def read_session():
    """Sesión unificada activa de Jinx (hermes-gateway :8642)."""
    if not GATEWAY_KEY:
        return {"error": "sin API_SERVER_KEY"}
    headers = {"Authorization": f"Bearer {GATEWAY_KEY}"}
    try:
        r = _http_get_json(
            f"{GATEWAY_ROOT}/api/sessions/resolve?key={GATEWAY_SESSION_KEY}",
            headers=headers, timeout=5)
        session_id = r.get("session_id")
        if not session_id:
            return {"error": "sin session_id"}
        detail = _http_get_json(f"{GATEWAY_ROOT}/api/sessions/{session_id}",
                                headers=headers, timeout=5)
        return {
            "session_id": session_id,
            "started_at": detail.get("started_at"),
            "last_active": detail.get("last_active"),
            "message_count": detail.get("message_count"),
        }
    except Exception as ex:
        return {"error": str(ex)[:150]}


_or_cache = {"t": 0, "data": None}

def read_openrouter():
    now = time.time()
    if _or_cache["data"] is not None and now - _or_cache["t"] < 300:
        return _or_cache["data"]
    import urllib.request
    result = {}
    for label, key in OR_KEYS.items():
        try:
            req = urllib.request.Request("https://openrouter.ai/api/v1/auth/key",
                                         headers={"Authorization": f"Bearer {key}"})
            with urllib.request.urlopen(req, timeout=10) as resp:
                d = json.load(resp).get("data", {})
            result[label] = {
                "usage_daily": round(d.get("usage_daily", 0), 4),
                "usage_weekly": round(d.get("usage_weekly", 0), 4),
                "usage_monthly": round(d.get("usage_monthly", 0), 4),
                "limit": d.get("limit"),
                "reset": d.get("limit_reset"),
                "limit_remaining": (round(d["limit_remaining"], 2)
                                    if d.get("limit_remaining") is not None else None),
            }
        except Exception as ex:
            result[label] = {"error": str(ex)[:120]}
    # créditos de la cuenta (key de jinx)
    if "jinx" in OR_KEYS:
        try:
            req = urllib.request.Request("https://openrouter.ai/api/v1/credits",
                                         headers={"Authorization": f"Bearer {OR_KEYS['jinx']}"})
            with urllib.request.urlopen(req, timeout=10) as resp:
                d = json.load(resp).get("data", {})
            result["credits"] = {
                "total": d.get("total_credits"),
                "used": round(d.get("total_usage", 0), 2),
                "remaining": (round(d["total_credits"] - d["total_usage"], 2)
                              if d.get("total_credits") is not None else None),
            }
        except Exception as ex:
            result["credits"] = {"error": str(ex)[:120]}
    _or_cache.update(t=now, data=result)
    return result


def read_crons():
    jobs = []
    now = time.time()

    def next_run(expr):
        if not croniter:
            return None
        try:
            return int(croniter(expr, now).get_next())
        except Exception:
            return None

    try:
        r = subprocess.run(["crontab", "-l"], capture_output=True, text=True, timeout=5)
        for line in r.stdout.splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split(None, 5)
            if len(parts) < 6:
                continue
            expr = " ".join(parts[:5])
            cmd = parts[5]
            name = cmd
            m = re.search(r"([\w.-]+\.(?:sh|py))", cmd)
            if m:
                name = m.group(1)
            elif "openclaw" in cmd:
                name = "whatsapp re-login"
            elif "hermes-gateway" in cmd:
                name = "watchdog hermes-gateway"
            jobs.append({"name": name[:60], "expr": expr, "source": "crontab",
                         "next": next_run(expr)})
    except Exception:
        pass

    try:
        data = json.loads(CRON_JOBS.read_text(encoding="utf-8"))
        lst = data.get("jobs", data) if isinstance(data, dict) else data
        for j in lst:
            sched = j.get("schedule") or {}
            expr = sched.get("expr")
            enabled = j.get("enabled", True)
            if not expr or not enabled:
                continue
            jobs.append({"name": (j.get("name") or j.get("id", "?"))[:60],
                         "expr": expr, "source": "hermes",
                         "next": next_run(expr)})
    except Exception:
        pass

    # este panel no muestra tareas de otros proyectos (Zynara tiene su propio tablero)
    jobs = [j for j in jobs if "zynara" not in j["name"].lower()]
    jobs.sort(key=lambda x: x["next"] or 9e18)
    return jobs


def busy_toggle(mode):
    sub = "on" if mode == "busy" else "off"
    args = [VENV_PY, str(ADMIN_CLI), "busy", sub]
    if sub == "on":
        args.append("manual (Cat OS dashboard)")
    r = subprocess.run(args, capture_output=True, text=True, timeout=15,
                       cwd=str(ADMIN_CLI.parent.parent))
    try:
        return json.loads(r.stdout.strip().splitlines()[-1])
    except Exception:
        return {"status": "ERROR", "raw": (r.stdout + r.stderr)[-300:]}

# ── HTTP ─────────────────────────────────────────────────────────────────────

class Handler(BaseHTTPRequestHandler):
    server_version = "CatOS/2.0"

    def _send(self, code, body, ctype="application/json; charset=utf-8"):
        data = body if isinstance(body, bytes) else json.dumps(
            body, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def _host_ok(self):
        if host_allowed(self.headers.get("Host"), PORT):
            return True
        self._send(421, {"error": "host no permitido"})
        return False

    def do_GET(self):
        if not self._host_ok():
            return
        if self.path in ("/", "/index.html"):
            try:
                html = HTML_PATH.read_bytes()
            except Exception:
                return self._send(500, {"error": "dashboard.html no encontrado"})
            return self._send(200, html, "text/html; charset=utf-8")
        if self.path == "/api/status":
            payload = {
                "ts": int(time.time()),
                "emotion": read_emotion(),
                "expression": read_dominant_expression(),
                "wakeword": read_wakeword(),
                "session": read_session(),
                "patterns": read_patterns(),
                "queue": read_queue(),
                "busy": read_busy(),
                "memories": read_memories(10),
                "services": read_services(),
                "openrouter": read_openrouter(),
                "crons": read_crons(),
            }
            return self._send(200, payload)
        return self._send(404, {"error": "not found"})

    def do_POST(self):
        if not self._host_ok():
            return
        # CSRF: un POST "simple" (text/plain) desde otra página llegaría aquí; el navegador siempre manda Origin.
        if not origin_allowed(self.headers.get("Origin"), self.headers.get("Host")):
            return self._send(403, {"error": "origen no permitido"})
        if self.path == "/api/busy":
            try:
                n = int(self.headers.get("Content-Length", 0))
                body = json.loads(self.rfile.read(n) or b"{}")
                mode = body.get("mode")
                if mode not in ("busy", "normal"):
                    return self._send(400, {"error": "mode debe ser busy|normal"})
                return self._send(200, busy_toggle(mode))
            except Exception as ex:
                return self._send(500, {"error": str(ex)})
        return self._send(404, {"error": "not found"})

    def log_message(self, fmt, *args):
        pass  # silencioso — systemd journal ya captura errores


def main():
    load_keys()
    # espera a que tailscale0 tenga la IP (arranque tras reboot)
    for _ in range(30):
        try:
            srv = ThreadingHTTPServer((BIND_IP, PORT), Handler)
            break
        except OSError:
            time.sleep(5)
    else:
        raise SystemExit(f"No se pudo enlazar {BIND_IP}:{PORT}")
    print(f"[catos] Hermes Cat OS V2 en http://{BIND_IP}:{PORT}/", flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
