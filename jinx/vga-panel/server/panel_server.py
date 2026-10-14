#!/home/alan/.hermes/hermes-agent/venv/bin/python3
"""Hermes VGA Panel — panel de control personal para Alan, sin animación.

Reemplaza el visualizador de orbe en la pantalla VGA física (kiosco Firefox
del servidor). Clon del ESQUELETO de Cat OS (catos_server.py: agregador
único /api/status, ThreadingHTTPServer, polling fetch+setInterval) — servicio
separado, NO modifica Cat OS. Cat OS sigue siendo el panel técnico/operativo
para el teléfono de Alan; este es el panel personal para la pantalla física.

Solo localhost: lo único que lo consume es el kiosco Firefox del mismo host.

Endpoints:
  GET  /            → HTML del panel
  GET  /api/status  → JSON agregado (clima, tareas, emoción de Jinx, saldo
                      OpenRouter)
"""

import json
import os
import re
import shutil
import subprocess
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

BIND_IP = os.environ.get("VGA_PANEL_BIND", "127.0.0.1")  # solo el kiosco local
PORT = int(os.environ.get("VGA_PANEL_PORT", "8896"))

HERMES = Path.home() / ".hermes"
SCRIPTS_DIR = HERMES / "scripts"
sys.path.insert(0, str(SCRIPTS_DIR))

import briefing_matutino as bm    # reuso: recolectar_clima(), recolectar_tareas()
import openrouter_saldo as ors  # reuso: saldo_efectivo() — NO reimplementar el saldo

STATE_MD = HERMES / "STATE.md"                       # emoción actual (frontmatter)
EMOTION_HISTORY = HERMES / "state" / "emotion_history.json"  # serie 6 h, la escribe pattern_detector — aquí SOLO se lee

HTML_PATH = Path(__file__).parent / "panel.html"

# ── clima / tareas / salud / presencia — todo reusado, nada reconstruido ────

def read_clima():
    try:
        return bm.recolectar_clima()
    except Exception as ex:
        return {"ok": False, "error": str(ex)[:150]}


def read_tareas():
    try:
        return bm.recolectar_tareas()
    except Exception as ex:
        return {"ok": False, "error": str(ex)[:150]}


_EMOCION_KEYS = ("affection", "patience", "trust")
_EMOCION_BANDA = 2.0   # ±2 pts respecto a hace ~24 h = "estable"


def read_emocion():
    """Afecto/paciencia/confianza actuales (STATE.md) + tendencia vs ~24 h atrás.

    El histórico son lecturas cada 6 h (máx 28 ≈ 7 d); 4 lecturas atrás ≈ 24 h.
    """
    try:
        import yaml
        m = re.match(r"^---\n(.*?)\n---", STATE_MD.read_text(encoding="utf-8"), re.S)
        es = (yaml.safe_load(m.group(1)) or {}).get("emotion_state", {}) if m else {}
        actual = {k: es.get(k) for k in _EMOCION_KEYS}
    except Exception as ex:
        return {"error": str(ex)[:150]}
    try:
        hist = json.loads(EMOTION_HISTORY.read_text(encoding="utf-8"))
        ref = hist[-5] if len(hist) >= 5 else (hist[0] if hist else None)
    except Exception:
        ref = None
    tendencia = {}
    for k in _EMOCION_KEYS:
        v, r = actual.get(k), (ref or {}).get(k)
        if v is None or r is None:
            tendencia[k] = "flat"
        else:
            d = v - r
            tendencia[k] = "up" if d > _EMOCION_BANDA else "down" if d < -_EMOCION_BANDA else "flat"
    return {**actual, "tendencia": tendencia}


def read_uptime():
    try:
        secs = int(float(Path("/proc/uptime").read_text().split()[0]))
        d, r = divmod(secs, 86400)
        h, r = divmod(r, 3600)
        return {"texto": (f"{d}D " if d else "") + f"{h}H {r // 60}M", "secs": secs}
    except Exception as ex:
        return {"texto": None, "error": str(ex)[:150]}


# ── sistema: los dos apagones silenciosos ya vividos ────────────────────────
# 2026-08-15/17: wallet de OpenRouter en $0 → 1.15M errores 402, Jinx muda dos
# días, y los watchdogs decían "OK" porque miraban el cupo mensual, no el
# dinero. 2026-08-09: el log de Cognee se comió 8.45 GB/día. Ninguno de los dos
# se veía en ninguna pantalla. El saldo cuesta 2 llamadas HTTP, así que se
# cachea: el panel refresca cada 30 s y no hace falta preguntar tan seguido.
# ponytail: caché en memoria de proceso, sin TTL configurable; si algún día
# hay más de un consumidor, mover a un archivo compartido.
_SALDO_TTL = 300
_saldo_cache = {"ts": 0.0, "val": None}


def read_saldo():
    """Los valores de OpenRouter que muestra el panel: cuenta y límite de la llave de Jinx."""
    ahora = time.monotonic()
    if _saldo_cache["val"] is not None and ahora - _saldo_cache["ts"] < _SALDO_TTL:
        return _saldo_cache["val"]
    try:
        def _sec(n):
            return subprocess.run([ors.SECRETS, "get", n],
                                  capture_output=True, text=True, timeout=30).stdout.strip()
        jinx = _sec("OPENROUTER_API_KEY")
        raw = ors.tres_valores(jinx, None) if jinx else {}
        # solo lo que este panel muestra: la cuenta y el límite de la llave de Jinx
        out = {k: v for k, v in raw.items() if k in ("cuenta", "jinx")}
        if out.get("cuenta") is None and not out.get("jinx", {}).get("restante"):
            out["error"] = "sin datos de OpenRouter"
    except Exception as ex:
        out = {"error": str(ex)[:150]}
    _saldo_cache.update(ts=ahora, val=out)
    return out


def read_disco():
    try:
        u = shutil.disk_usage("/")
        return {"libre_gb": round(u.free / 2**30, 1), "pct_usado": round(100 * u.used / u.total)}
    except Exception as ex:
        return {"libre_gb": None, "error": str(ex)[:150]}


def read_gateway():
    try:
        r = subprocess.run(["systemctl", "is-active", "hermes-gateway"],
                           capture_output=True, text=True, timeout=10)
        return {"activo": r.stdout.strip() == "active", "estado": r.stdout.strip()}
    except Exception as ex:
        return {"activo": None, "error": str(ex)[:150]}


def host_allowed(host_header, port):
    """Solo el kiosco local: un `Host` que no sea este equipo es DNS rebinding (una página de otro origen que
    resuelve su dominio a 127.0.0.1) y no se sirve."""
    if not host_header:
        return False
    return host_header.strip().lower() in ("127.0.0.1:%d" % port, "localhost:%d" % port, "[::1]:%d" % port)


def status_payload():
    return {
        "ts": int(time.time()),
        # mtime de panel.html: el front recarga solo si cambia, así un
        # deploy de HTML no exige un F5 a mano en la pantalla física.
        "build": int(HTML_PATH.stat().st_mtime) if HTML_PATH.exists() else 0,
        "clima": read_clima(),
        "tareas": read_tareas(),
        "emocion": read_emocion(),
        "saldo": read_saldo(),
        "disco": read_disco(),
        "gateway": read_gateway(),
        "uptime": read_uptime(),
    }


# ── HTTP ─────────────────────────────────────────────────────────────────────

class Handler(BaseHTTPRequestHandler):
    server_version = "HermesVGAPanel/1.0"

    def _send(self, code, body, ctype="application/json; charset=utf-8"):
        data = body if isinstance(body, bytes) else json.dumps(body, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if not host_allowed(self.headers.get("Host"), PORT):
            return self._send(421, {"error": "host no permitido"})
        if self.path in ("/", "/index.html"):
            try:
                html = HTML_PATH.read_bytes()
            except Exception:
                return self._send(500, {"error": "panel.html no encontrado"})
            return self._send(200, html, "text/html; charset=utf-8")
        if self.path == "/api/status":
            return self._send(200, status_payload())
        return self._send(404, {"error": "not found"})

    def log_message(self, fmt, *args):
        pass  # silencioso — systemd journal ya captura errores


def main():
    for _ in range(10):
        try:
            srv = ThreadingHTTPServer((BIND_IP, PORT), Handler)
            break
        except OSError:
            time.sleep(3)
    else:
        raise SystemExit(f"No se pudo enlazar {BIND_IP}:{PORT}")
    print(f"[vga_panel] Hermes VGA Panel en http://{BIND_IP}:{PORT}/", flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
