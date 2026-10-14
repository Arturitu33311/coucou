"""Gateway de chat de Jinx para los teléfonos (comando forzado por ssh).

Lee UNA línea JSON de stdin y escribe eventos NDJSON en stdout. Ver
docs/superpowers/specs/2026-10-13-jinx-assistant-text-design.md.
No hace getUpdates ni toca webhooks: sólo sendMessage.
"""
import contextlib
import fcntl
import hashlib
import http.client
import importlib.machinery
import importlib.util
import json
import os
import re
import shlex
import socket
import ssl
import subprocess
import sys
import time
import urllib.request
from dataclasses import dataclass
from typing import Callable, Optional

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from jinx_thread import EMOTES, Thread, valid_mood  # noqa: E402

HOME = os.path.expanduser("~")
BASE = os.path.join(HOME, "jinx-emergencia", "telegram")
# Quien puede escribir por el gateway: los dos teléfonos y Coucou del PC (local, sin ssh).
SRCS = ("s21", "s22", "pc")
# Cómo se ve cada origen al espejarlo en Telegram.
MIRROR_PREFIX = {"jinx": "🟠 ", "pc": "💻 "}
TG_MAX = 4000


@dataclass
class Deps:
    thread: Thread
    brain: object          # .reply(messages) -> (mode, text); lanza si falla
    mirror: object         # .send(text) -> bool
    core_text: str
    memory_text: str
    secrets: Callable[[], dict]
    base: str
    # Si está puesto, el espejo a Telegram NO se hace en la petición: se delega (proceso aparte) y la
    # respuesta sale de inmediato. El gateway sigue siendo responsable: la línea queda mirrored:false
    # en el hilo y se reintenta en cada op.
    defer_mirror: Optional[Callable[[], None]] = None
    # La Jinx real (Hermes): se usa para mostrar en el hilo lo hablado por Telegram. None = sin Hermes.
    hermes: Optional[object] = None
    # Su `emotion_state` (el STATE.md del servidor) para `op: mood`: () -> dict, o lanza. None = no hay de dónde leerlo.
    state: Optional[Callable[[], dict]] = None
    # La hora local (0-23) para el ánimo de noche. None = el reloj de este equipo.
    hour: Optional[Callable[[], int]] = None


# ---------- ánimo ----------
#
# Hermes no devuelve ánimo con sus respuestas (solo texto): el ánimo de fondo sale de su `emotion_state`
# (STATE.md del servidor, lo escribe el hook persona-engine-turn en cada mensaje de Alan) y el de una respuesta,
# si algún día el cerebro la termina con la etiqueta oculta `[[mood:<emote>]]`, de esa etiqueta.

_MOOD_TAG_RE = re.compile(r"[ \t]*\[\[\s*mood\s*:\s*([^\]]*?)\s*\]\]", re.IGNORECASE)


def split_mood(text):
    """(texto sin etiquetas, emote|None). Quita TODA etiqueta `[[mood:…]]`, válida o no; el emote es el de
    la última etiqueta válida. Sin etiqueta o con una desconocida no hay emote (los clientes usan el texto)."""
    if not isinstance(text, str) or "[[" not in text:
        return text, None
    mood = None
    for m in _MOOD_TAG_RE.finditer(text):
        mood = valid_mood(m.group(1).strip().lower()) or mood
    return _MOOD_TAG_RE.sub("", text).strip(), mood


# parse_emotion_state, baseline_emote y sus umbrales viven en jinx_baseline.py: el servidor de presencia usa los
# mismos (ver ese módulo).
from jinx_baseline import baseline_emote, parse_emotion_state  # noqa: E402,F401


def _mood_event(deps):
    """`op: mood`: el ánimo de fondo. Nunca falla: sin STATE.md → emote null, source "text"."""
    try:
        state = deps.state() if deps.state else None
    except Exception:  # noqa: BLE001 - el servidor puede no contestar; los clientes siguen con el texto
        state = None
    if not isinstance(state, dict):
        return {"ev": "mood", "emote": None, "source": "text", "emotion_state": None}
    try:
        hour = deps.hour() if deps.hour else time.localtime().tm_hour
    except Exception:  # noqa: BLE001
        hour = None
    return {"ev": "mood", "emote": baseline_emote(state, hour), "source": "state", "emotion_state": state}


_TOKEN_RE = re.compile(r"(?:bot)?\d{6,}(?::|%3A)[A-Za-z0-9_-]{20,}", re.IGNORECASE)


def redact(text):
    """Quita de cualquier texto lo que parezca un token de bot de Telegram (nunca debe llegar a un log)."""
    return _TOKEN_RE.sub("<token>", str(text))


def append_log(path, line):
    """Añade una línea ya redactada a un log privado (0600)."""
    fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600)
    try:
        os.fchmod(fd, 0o600)
        os.write(fd, (redact(line).rstrip("\n") + "\n").encode("utf-8", "replace"))
    finally:
        os.close(fd)


def _err(msg):
    return {"ev": "error", "msg": msg}


def _mirror_pending(deps):
    """Intenta espejar, en orden, lo que quedó sin espejar. Para al primer fallo."""
    done = []
    for d in deps.thread.unmirrored():
        prefix = MIRROR_PREFIX.get(d["src"], "📱 ")
        if not deps.mirror.send((prefix + d["text"])[:TG_MAX]):
            break
        done.append(d["id"])
    if done:
        deps.thread.mark_mirrored(set(done))
    return done


@contextlib.contextmanager
def _cid_lock(base, cid):
    """Serializa los envíos del mismo cid (un reintento espera a la primera generación y la reutiliza).

    Se usa un número fijo de archivos de candado (hash del cid) para que no se acumulen.
    """
    bucket = int(hashlib.sha1(cid.encode("utf-8")).hexdigest(), 16) % 64
    with open(os.path.join(base, "inflight-%02d.lock" % bucket), "a+") as lf:
        fcntl.flock(lf, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(lf, fcntl.LOCK_UN)


def _mirrored(deps, ids):
    by_id = {d["id"]: d for d in deps.thread.read()}
    return all(by_id.get(i, {}).get("mirrored") for i in ids)


def _settle_mirror(deps, ids):
    """Espeja ahora (síncrono) o delega; devuelve True si el gateway se hace responsable de las líneas."""
    if deps.defer_mirror is not None:
        deps.defer_mirror()
        return True
    _mirror_pending(deps)
    return _mirrored(deps, ids)


def _pull_hermes(deps):
    """Mezcla en el hilo lo hablado en la sesión de Telegram de Hermes (lo que no está ya). Nunca falla."""
    if deps.hermes is None:
        return 0
    try:
        items = deps.hermes.history(40)
    except Exception:  # noqa: BLE001 - Hermes caído: el hilo local sigue sirviendo
        return 0
    known = {("jinx" if d["src"] == "jinx" else "user", d["text"].strip()) for d in deps.thread.read()}
    incoming = []
    for m in items:
        role = "jinx" if m.get("role") == "assistant" else "user"
        # La etiqueta de ánimo nunca llega al hilo: lo local ya se guardó sin ella y la comparación es por texto.
        text, mood = split_mood(str(m.get("text", "")))
        if not text.strip() or (role, text.strip()) in known:
            continue
        src = "jinx" if role == "jinx" else "tg"
        line = {"id": "h%s:%s" % (m.get("id"), src), "ts": float(m.get("ts") or 0), "src": src,
                "text": text, "cid": None, "mirrored": True}
        if mood and src == "jinx":
            line["mood"] = mood
        incoming.append(line)
    return len(deps.thread.merge(incoming)) if incoming else 0


def _send(req, deps):
    text = req.get("text")
    cid = req.get("cid")
    src = req.get("src", "s21")
    if not isinstance(text, str) or not text.strip() or not isinstance(cid, str) or not cid:
        yield _err("send necesita text y cid")
        return
    if src not in SRCS:
        yield _err("src desconocido")
        return
    ids = ["%s:%s" % (cid, src), "%s:jinx" % cid]
    with _cid_lock(deps.base, cid):
        cached = deps.thread.answer_for(cid)
        if cached:
            yield {"ev": "ack"}
            done = {"ev": "done", "mode": "cached", "text": cached["text"], "mirrored": _settle_mirror(deps, ids)}
            if valid_mood(cached.get("mood")):
                done["mood"] = cached["mood"]
            yield done
            return
        deps.thread.append(src, text, cid=cid)
        yield {"ev": "ack"}
        try:
            mode, out = deps.brain.reply(deps.thread.context())
        except Exception as e:  # noqa: BLE001 - el modelo puede fallar de mil formas
            yield _err("no pude generar respuesta: %s" % e)
            return
        # La etiqueta oculta de ánimo se quita ANTES de guardar y de espejar a Telegram; el emote viaja aparte.
        out, mood = split_mood((out or "").strip())
        out = (out or "").strip() or "(sin respuesta)"
        line = deps.thread.append("jinx", out, cid=cid, mood=mood)
        done = {"ev": "done", "mode": mode, "text": line["text"], "mirrored": _settle_mirror(deps, ids)}
        if mood:
            done["mood"] = mood  # solo si la respuesta traía una etiqueta válida; nunca el ánimo de fondo
        yield done


def handle(req, deps):
    op = req.get("op")
    if op == "send":
        yield from _send(req, deps)
    elif op == "history":
        n = req.get("n", 40)
        n = n if isinstance(n, int) and 0 < n <= 500 else 40
        _pull_hermes(deps)
        yield {"ev": "history", "items": deps.thread.read()[-n:]}
    elif op == "merge":
        lines = req.get("lines")
        have = req.get("have")
        if not isinstance(lines, list) or not isinstance(have, list):
            yield _err("merge necesita lines y have")
            return
        # Una línea que llegue con la etiqueta (p. ej. de un cerebro en el teléfono) se limpia igual que aquí.
        clean = []
        for d in lines:
            if isinstance(d, dict) and isinstance(d.get("text"), str) and "[[" in d["text"]:
                text, mood = split_mood(d["text"])
                d = dict(d, text=text)
                if mood and d.get("src") == "jinx":
                    d["mood"] = mood
            clean.append(d)
        added = deps.thread.merge(clean)
        _pull_hermes(deps)
        sent_ids = {d["id"] for d in lines if isinstance(d, dict) and isinstance(d.get("id"), str)}
        if deps.defer_mirror is not None:
            deps.defer_mirror()
            # El gateway ya tiene esas líneas en su hilo y se hace responsable de espejarlas.
            mirrored_ids = [d["id"] for d in deps.thread.read() if d["id"] in sent_ids]
        else:
            _mirror_pending(deps)
            mirrored_ids = [d["id"] for d in deps.thread.read() if d["id"] in sent_ids and d["mirrored"]]
        yield {"ev": "merged", "added": len(added),
               "items": deps.thread.since({h for h in have if isinstance(h, str)}),
               "mirrored_ids": mirrored_ids}
    elif op == "mood":
        yield _mood_event(deps)
    elif op == "core":
        yield {"ev": "core", "core": deps.core_text, "memory": deps.memory_text}
    elif op == "provision":
        flag = os.path.join(deps.base, "ALLOW_PROVISION")
        if not os.path.exists(flag):
            yield _err("provision no autorizado")
            return
        s = deps.secrets()
        os.remove(flag)
        yield {"ev": "secrets", "openrouter_key": s.get("openrouter_key", ""),
               "telegram_token": s.get("telegram_token", ""), "chat": s.get("chat", "")}
    else:
        yield _err("op desconocida")


def run_line(raw, deps):
    raw = (raw or "").strip()
    if not raw:
        yield _err("petición vacía")
        return
    try:
        req = json.loads(raw)
    except ValueError:
        yield _err("petición no es JSON")
        return
    if not isinstance(req, dict) or "op" not in req:
        yield _err("petición sin op")
        return
    try:
        yield from handle(req, deps)
    except Exception as e:  # noqa: BLE001
        yield _err("error interno: %s" % e)


# ---------- implementaciones reales ----------

TG_HOST = "api.telegram.org"
# Desde la red del servidor el DNS de Telegram a veces no llega ("No route to host"): se prueban estas IPs
# primero (las mismas que usa hermes_telegram_retry.py) y el DNS al final.
TG_DEFAULT_IPS = ("149.154.167.99", "149.154.167.220", "149.154.166.110")
HERMES_ENV = os.path.join(HOME, ".hermes", ".env")


def telegram_ips(env=None, env_file=HERMES_ENV):
    """IPs fijas de Telegram del servidor: TELEGRAM_FALLBACK_IPS (entorno o ~/.hermes/.env), o las de siempre."""
    raw = (env if env is not None else os.environ).get("TELEGRAM_FALLBACK_IPS", "")
    if not raw:
        try:
            for line in _read(env_file).splitlines():
                if line.startswith("TELEGRAM_FALLBACK_IPS="):
                    raw = line.split("=", 1)[1].strip().strip("\"'")
                    break
        except OSError:
            pass
    ips = [p.strip() for p in raw.split(",") if p.strip()] or list(TG_DEFAULT_IPS)
    return list(dict.fromkeys(ips))


class _PinnedConn(http.client.HTTPSConnection):
    """HTTPS a una IP fija conservando SNI y validación del certificado de api.telegram.org."""

    def __init__(self, ip, timeout=8):
        super().__init__(TG_HOST, timeout=timeout)
        self._ip = ip

    def connect(self):
        sock = socket.create_connection((self._ip or TG_HOST, 443), timeout=self.timeout)
        self.sock = ssl.create_default_context().wrap_socket(sock, server_hostname=TG_HOST)


class TelegramMirror:
    def __init__(self, token, chat, opener=urllib.request.urlopen, ips=None, conn_factory=_PinnedConn):
        self.token, self.chat, self.opener = token, chat, opener
        self.ips, self.conn_factory = ips, conn_factory
        self.last_error = ""

    def _send_pinned(self, data):
        """Prueba cada IP fija y al final el DNS; la primera que contesta con una respuesta de la API gana."""
        last = "sin ruta"
        for ip in list(self.ips) + [None]:
            try:
                conn = self.conn_factory(ip)
                conn.request("POST", "/bot%s/sendMessage" % self.token, data, {"Content-Type": "application/json"})
                resp = conn.getresponse()
                body = json.loads(resp.read() or b"{}")
                conn.close()
            except (OSError, ssl.SSLError, ValueError, http.client.HTTPException) as e:
                last = type(e).__name__
                continue
            if body.get("ok"):
                self.last_error = ""
                return True
            last = "%s:%s" % (body.get("error_code"), str(body.get("description"))[:80])
            if body.get("error_code") in (400, 403):  # chat o bot mal: otra IP no ayuda
                break
        self.last_error = redact(last)
        return False

    def send(self, text):
        if not self.token or not self.chat or self.opener is None:
            self.last_error = "sin token o chat"
            return False
        if self.ips:
            return self._send_pinned(json.dumps({"chat_id": str(self.chat), "text": text[:TG_MAX]}).encode())
        data = json.dumps({"chat_id": str(self.chat), "text": text[:TG_MAX]}).encode()
        req = urllib.request.Request(
            "https://api.telegram.org/bot%s/sendMessage" % self.token, data=data,
            headers={"Content-Type": "application/json"})
        try:
            with self.opener(req, timeout=15) as r:
                ok = r.status == 200
                self.last_error = "" if ok else "HTTP %s" % r.status
                return ok
        except Exception as e:  # noqa: BLE001 - sin red, 4xx, timeout: se reintenta luego
            self.last_error = redact(type(e).__name__ + ": " + str(e).replace(self.token, "<token>"))
            return False


class LazyTelegramMirror(TelegramMirror):
    """Igual que TelegramMirror, pero token y chat se piden a la bóveda sólo al primer envío real."""

    def __init__(self, token_fn, chat_fn, opener=urllib.request.urlopen, ips=None, conn_factory=_PinnedConn):
        super().__init__("", "", opener, ips=ips, conn_factory=conn_factory)
        self._token_fn, self._chat_fn, self._resolved = token_fn, chat_fn, False

    def send(self, text):
        if not self._resolved:
            self.token, self.chat = self._token_fn() or "", self._chat_fn() or ""
            self._resolved = True
        return super().send(text)


def _load_jinx():
    """El módulo `jinx` del portátil (bóveda, memoria, modelos locales), o None si no está: en el servidor no
    existe y el gateway corre en modo servidor (ver `build_server_deps`)."""
    path = os.path.join(HOME, "bin", "jinx")
    if not os.path.exists(path):
        return None
    loader = importlib.machinery.SourceFileLoader("jinxmod", path)
    spec = importlib.util.spec_from_loader("jinxmod", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


def _vault_get(jx, key):
    try:
        out = subprocess.run(
            [jx.VAULT_PY, "-c", "from vaultknox.autonomous_secrets import "
             "get_secret as g; print(g(%r))" % key],
            capture_output=True, text=True, timeout=10)
        return out.stdout.strip()
    except Exception:  # noqa: BLE001
        return ""


def _read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


HERMES_CMD = os.environ.get(
    "JINX_HERMES_CMD",
    "/home/alan/.hermes/hermes-agent/venv/bin/python /home/alan/.hermes/scripts/jinx_hermes_chat.py")


def default_hermes_host(cmd=HERMES_CMD, exists=os.path.exists):
    """Dónde está Hermes si nadie lo dice: aquí mismo cuando su intérprete y su puente están en esta máquina
    (el gateway corre en el servidor), si no el servidor por ssh (el gateway de respaldo del portátil). Pura."""
    argv = shlex.split(cmd)
    return "local" if argv and all(exists(a) for a in argv if a.startswith("/")) else "server"


HERMES_SSH = os.environ.get("JINX_HERMES_SSH") or default_hermes_host()


def hermes_argv(cmd, host=None):
    """Cómo se corre un comando de Hermes: por ssh al servidor desde el portátil; tal cual cuando el gateway
    corre en el propio servidor (JINX_HERMES_SSH=local o vacío). Pura."""
    host = HERMES_SSH if host is None else host
    if host.strip() in ("", "local"):
        return shlex.split(cmd)
    return ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=6", host, cmd]


def ssh_runner(payload, timeout=85):
    """Una petición al puente de Hermes (por ssh, o local en el servidor); devuelve el evento JSON o lanza."""
    p = subprocess.run(
        hermes_argv(HERMES_CMD),
        input=json.dumps(payload) + "\n", capture_output=True, text=True, timeout=timeout)
    lines = [ln for ln in p.stdout.splitlines() if ln.strip()]
    if not lines:
        raise RuntimeError("Hermes no respondió (%s)" % (p.stderr.strip().splitlines() or ["sin salida"])[-1][:80])
    return json.loads(lines[-1])


STATE_CMD = os.environ.get("JINX_STATE_CMD", "cat /home/alan/.hermes/STATE.md")


def ssh_state_reader(timeout=12, runner=subprocess.run):
    """El `emotion_state` de Hermes: STATE.md del servidor, leído por ssh (solo lectura, con tope de tiempo).
    No se lee por el montaje del servidor: un montaje colgado bloquearía al gateway. Lanza si no hay estado."""
    p = runner(hermes_argv(STATE_CMD),
               capture_output=True, text=True, timeout=timeout)
    state = parse_emotion_state(p.stdout)
    if state is None:
        raise RuntimeError("STATE.md sin emotion_state")
    return state


class HermesBrain:
    """La Jinx real, con su memoria, sus herramientas y la conversación de Telegram."""

    def __init__(self, runner=ssh_runner):
        self.runner = runner

    def reply(self, messages):
        user = next((m["content"] for m in reversed(messages) if m.get("role") == "user"), None)
        if not user:
            raise RuntimeError("no hay mensaje al que contestar")
        ev = self.runner({"op": "send", "text": user})
        if ev.get("ev") != "done":
            raise RuntimeError(ev.get("msg") or "Hermes no contestó")
        return "hermes", ev["text"]

    def history(self, n=40):
        ev = self.runner({"op": "history", "n": n})
        return ev.get("items", []) if ev.get("ev") == "history" else []


class ChainBrain:
    """Prueba cerebros en orden; el primero que contesta gana. Si todos fallan, lanza el último error."""

    def __init__(self, brains):
        self.brains = brains

    def reply(self, messages):
        last = RuntimeError("sin cerebros")
        for b in self.brains:
            try:
                return b.reply(messages)
            except Exception as e:  # noqa: BLE001
                last = e
        raise last


class JinxBrain:
    """Mismo cerebro de respaldo que el relay: OpenRouter, si no, Ollama local."""

    def __init__(self, jx):
        self.jx = jx
        self.core = _read(jx.CORE_PATH if os.path.exists(jx.CORE_PATH) else jx.SOUL_PATH)
        self.mem, _ = jx.load_synced_memory()
        self.key = os.environ.get("JINX_OR_KEY") or jx.get_api_key()

    def system(self, mode):
        core = self.core
        if mode == "or":
            core = core.replace("en un modelo local pequeño", "sobre un modelo capaz vía OpenRouter")
        return self.jx.PREAMBLE[mode] + "\n\n---\n\n" + core + (self.mem if mode == "or" else "")

    def reply(self, messages):
        jx = self.jx
        # Sin sonda previa de alcanzabilidad: era una petición extra por mensaje; si OpenRouter falla,
        # el except de abajo ya cae al modelo local.
        mode = "or" if self.key else "local"
        msgs = [{"role": "system", "content": self.system(mode)}] + messages
        try:
            if mode == "or":
                return mode, "".join(jx.stream_openrouter(msgs, self.key))
            jx.pick_local_backend()
            return mode, "".join(jx.strip_think(jx.stream_ollama(msgs)))
        except Exception:  # noqa: BLE001 - OpenRouter cayó a medias -> local
            if mode != "or":
                raise
            jx.pick_local_backend()
            msgs[0]["content"] = self.system("local")
            return "local", "".join(jx.strip_think(jx.stream_ollama(msgs)))


def _spawn_flush_mirror():
    """Lanza `jinx-chat --flush-mirror` desacoplado: sobrevive al cierre de la sesión ssh del teléfono."""
    try:
        subprocess.Popen(
            [sys.executable, os.path.abspath(__file__), "--flush-mirror"],
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True, close_fds=True)
    except OSError:
        pass  # las líneas siguen mirrored:false y se reintentan en la próxima op


# ---------- el gateway en el propio servidor ----------
#
# Allí no hay `~/bin/jinx` ni la bóveda del portátil: los secretos salen de la bóveda del servidor
# (`hermes-secrets get`, la misma que usan jinx_send.py y jinx_hermes_chat.py; nunca se copian ni se escriben) y
# Hermes es local. Si Hermes falla pero el servidor vive, contesta OpenRouter con el alma de Jinx: un `send`
# que ya llegó al servidor no se reenvía al portátil, así que aquí no puede quedarse sin respuesta.

SECRETS_CMD = os.environ.get("JINX_SECRETS_CMD", "/home/alan/.hermes/hermes-agent/venv/bin/hermes-secrets")
# Nombre que pide el gateway -> nombres en la bóveda del servidor, por orden.
SERVER_SECRET_NAMES = {
    "TELEGRAM_BOT_TOKEN": ("TELEGRAM_BOT_TOKEN",),
    "TELEGRAM_ALLOWED_CHAT": ("TELEGRAM_ALLOWED_CHAT", "TELEGRAM_HOME_CHANNEL"),
    "OPENROUTER_API_KEY": ("OPENROUTER_API_KEY",),
}
OR_URL = os.environ.get("JINX_OR_BASE", "https://openrouter.ai/api/v1") + "/chat/completions"
OR_MODEL = os.environ.get("JINX_OR_MODEL", "deepseek/deepseek-v4-flash")
SOUL_FALLBACK = os.path.join(HOME, ".hermes", "SOUL.md")
SOUL_MAX = 6000


def server_secret(name, run=subprocess.run, cmd=None):
    """Un secreto de la bóveda del servidor, o "" (sin imprimirlo ni registrarlo en ningún sitio)."""
    cmd = cmd or SECRETS_CMD
    for n in SERVER_SECRET_NAMES.get(name, (name,)):
        try:
            p = run([cmd, "get", n], capture_output=True, text=True, timeout=10)
        except Exception:  # noqa: BLE001 - sin bóveda: el espejo se reintenta luego
            continue
        if getattr(p, "returncode", 1) == 0 and (p.stdout or "").strip():
            return p.stdout.strip().splitlines()[-1].strip()
    return ""


class ServerFallbackBrain:
    """Respaldo del servidor cuando Hermes falla: OpenRouter con el alma de Jinx. Lanza si no hay llave o falla."""

    def __init__(self, key_fn, core_fn, opener=urllib.request.urlopen):
        self.key_fn, self.core_fn, self.opener = key_fn, core_fn, opener

    def reply(self, messages):
        key = self.key_fn()
        if not key:
            raise RuntimeError("sin llave de OpenRouter en el servidor")
        system = ("Eres Jinx. Tu memoria completa (Hermes) no está disponible ahora mismo: contesta breve y "
                  "natural, sin inventar lo que no sepas.\n\n---\n\n" + self.core_fn())
        body = json.dumps({"model": OR_MODEL,
                           "messages": [{"role": "system", "content": system}] + list(messages)}).encode()
        req = urllib.request.Request(OR_URL, data=body, headers={
            "Content-Type": "application/json", "Authorization": "Bearer " + key})
        try:
            with self.opener(req, timeout=60) as r:
                data = json.loads(r.read().decode("utf-8", "replace"))
            text = data["choices"][0]["message"]["content"]
        except Exception as e:  # noqa: BLE001
            raise RuntimeError(redact(str(e)).replace(key, "<llave>")) from None
        if not (text or "").strip():
            raise RuntimeError("OpenRouter contestó vacío")
        return "or", text


def _read_first(*paths, limit=None):
    for p in paths:
        try:
            t = _read(p)
        except OSError:
            continue
        if t.strip():
            return t[:limit] if limit else t
    return ""


def build_server_deps(base=BASE, defer=False, secret=server_secret):
    cache = {}

    def lazy(name):
        if name not in cache:
            cache[name] = os.environ.get({"TELEGRAM_BOT_TOKEN": "JINX_TG_TOKEN",
                                          "TELEGRAM_ALLOWED_CHAT": "JINX_TG_CHAT",
                                          "OPENROUTER_API_KEY": "JINX_OR_KEY"}.get(name, ""), "") or secret(name)
        return cache[name]

    emerg = os.path.join(HOME, "jinx-emergencia")
    core = _read_first(os.path.join(emerg, "JINX-CORE.md"))
    memory = _read_first(os.path.join(emerg, "MEMORY.md"), os.path.join(HOME, ".hermes", "memories", "MEMORY.md"))
    soul = core or _read_first(SOUL_FALLBACK, limit=SOUL_MAX)
    no_hermes = bool(os.environ.get("JINX_NO_HERMES"))
    fallback = ServerFallbackBrain(lambda: lazy("OPENROUTER_API_KEY"), lambda: soul)
    hermes = None if no_hermes else HermesBrain()
    brain = ChainBrain([hermes, fallback]) if hermes else fallback
    thread = Thread(base)

    def defer_if_needed():
        if thread.unmirrored():
            _spawn_flush_mirror()

    return Deps(thread, brain,
                LazyTelegramMirror(lambda: lazy("TELEGRAM_BOT_TOKEN"), lambda: lazy("TELEGRAM_ALLOWED_CHAT"),
                                   ips=telegram_ips()),
                core, memory,
                lambda: {"openrouter_key": lazy("OPENROUTER_API_KEY"), "telegram_token": lazy("TELEGRAM_BOT_TOKEN"),
                         "chat": lazy("TELEGRAM_ALLOWED_CHAT")},
                base, defer_mirror=defer_if_needed if defer else None, hermes=hermes,
                state=None if no_hermes else ssh_state_reader)


def build_real_deps(base=BASE, defer=False):
    jx = _load_jinx()
    if jx is None:
        return build_server_deps(base, defer)
    cache = {}

    def secret(env_names, vault_key):
        if vault_key not in cache:
            cache[vault_key] = next((os.environ[e] for e in env_names if os.environ.get(e)), "") \
                or _vault_get(jx, vault_key)
        return cache[vault_key]

    def token():
        return secret(("JINX_TG_TOKEN", "TELEGRAM_BOT_TOKEN"), "TELEGRAM_BOT_TOKEN")

    def chat():
        return secret(("JINX_TG_CHAT", "TELEGRAM_ALLOWED_CHAT"), "TELEGRAM_ALLOWED_CHAT")

    jinx_brain = JinxBrain(jx)
    hermes = None if os.environ.get("JINX_NO_HERMES") else HermesBrain()
    brain = ChainBrain([hermes, jinx_brain]) if hermes else jinx_brain
    memory = jx.load_synced_memory()[0]
    thread = Thread(base)

    def defer_if_needed():
        if thread.unmirrored():
            _spawn_flush_mirror()

    return Deps(thread, brain, LazyTelegramMirror(token, chat),
                jinx_brain.core, memory,
                lambda: {"openrouter_key": jinx_brain.key or "", "telegram_token": token(), "chat": chat()},
                base, defer_mirror=defer_if_needed if defer else None, hermes=hermes,
                state=None if os.environ.get("JINX_NO_HERMES") else ssh_state_reader)


def flush_mirror_main():
    """`jinx-chat --flush-mirror`: repite a Telegram lo que quedó pendiente (corre desacoplado)."""
    log_path = os.path.join(BASE, "gateway.log")

    def log(msg):
        try:
            append_log(log_path, "%s flush-mirror: %s" % (time.strftime("%Y-%m-%dT%H:%M:%S"), msg))
        except OSError:
            pass

    try:
        deps = build_real_deps(defer=False)
        pending = len(deps.thread.unmirrored())
        done = _mirror_pending(deps)
        if len(done) < pending:
            log("espejadas %d de %d; último error: %s" % (len(done), pending, getattr(deps.mirror, "last_error", "?")))
    except Exception as e:  # noqa: BLE001 - se reintenta en la próxima op
        log("excepción: %s: %s" % (type(e).__name__, e))
        return 1
    return 0


# ---------- el portátil como respaldo del servidor ----------
#
# El gateway principal corre en el servidor; el del portátil solo atiende cuando el servidor falla (los clientes
# deciden: Failover en ChatApi.kt y jinx_gateway.rs). Lo que el portátil atendió entonces vive en SU hilo: al
# volver el servidor, `jinx-chat --sync-to-server` se lo pasa (op merge: deduplica por id y no vuelve a espejar a
# Telegram lo ya espejado) y se trae lo del servidor, así los dos hilos vuelven a ser el mismo.

SYNC_HOST = os.environ.get("JINX_SYNC_HOST", "server")
SYNC_CMD = os.environ.get("JINX_SYNC_CMD", "bin/jinx-chat")
SYNC_LINES = 500


def sync_to_server(thread, ask, limit=SYNC_LINES):
    """Pasa al servidor las últimas `limit` líneas de este hilo y trae las suyas. `ask(req) -> [eventos]`.
    Devuelve (las que el servidor añadió, las que se añadieron aquí). Lanza si el servidor no contesta como
    gateway (nada cambia aquí)."""
    mine = thread.read()[-limit:]
    events = ask({"op": "merge", "lines": mine, "have": [d["id"] for d in thread.read()]})
    merged = next((e for e in events if isinstance(e, dict) and e.get("ev") == "merged"), None)
    if merged is None:
        err = next((e.get("msg") for e in events if isinstance(e, dict) and e.get("ev") == "error"), None)
        raise RuntimeError(err or "el servidor no contestó como gateway")
    items = merged.get("items") if isinstance(merged.get("items"), list) else []
    # Lo que el servidor ya espejó cuenta como espejado aquí también (y al revés lo hizo el merge del servidor).
    mirrored = set(merged.get("mirrored_ids") or [])
    here = thread.merge(items)
    if mirrored:
        thread.mark_mirrored(mirrored)
    return merged.get("added", 0), len(here)


def ssh_gateway_ask(req, host=SYNC_HOST, cmd=SYNC_CMD, timeout=60):
    """Una petición al gateway del servidor por ssh: sus eventos."""
    p = subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=6", host, cmd],
                       input=json.dumps(req, ensure_ascii=False) + "\n", capture_output=True, text=True, timeout=timeout)
    out = []
    for ln in p.stdout.splitlines():
        try:
            out.append(json.loads(ln))
        except ValueError:
            continue
    return out


def sync_main():
    try:
        sent, got = sync_to_server(Thread(BASE), ssh_gateway_ask)
    except Exception as e:  # noqa: BLE001 - servidor caído: se reintenta en la próxima pasada
        sys.stdout.write("sync: el servidor no está (%s)\n" % e)
        return 1
    sys.stdout.write("sync: %d líneas nuevas en el servidor, %d aquí\n" % (sent, got))
    return 0


def main(stdin=sys.stdin, stdout=sys.stdout):
    if "--flush-mirror" in sys.argv[1:]:
        return flush_mirror_main()
    if "--sync-to-server" in sys.argv[1:]:
        return sync_main()
    raw = stdin.readline()
    try:
        req = json.loads(raw) if raw.strip() else {}
    except ValueError:
        req = {}
    # Sólo se construye el cerebro real si la op lo necesita (history no).
    needs_real = isinstance(req, dict) and req.get("op") in ("send", "core", "provision", "merge")
    t_start = time.time()
    try:
        # Las ops ligeras (history, mood) no necesitan cerebro ni bóveda; mood sí necesita poder leer el estado.
        deps = build_real_deps(defer=True) if needs_real else Deps(
            Thread(BASE), None, TelegramMirror("", ""), "", "", dict, BASE,
            state=None if os.environ.get("JINX_NO_HERMES") else ssh_state_reader)
    except Exception as e:  # noqa: BLE001
        stdout.write(json.dumps(_err("gateway sin cerebro: %s" % e)) + "\n")
        return 0
    t_deps = time.time()
    for ev in run_line(raw, deps):
        stdout.write(json.dumps(ev, ensure_ascii=False) + "\n")
        stdout.flush()
    t_end = time.time()
    # Tiempos por petición (sin contenido): sirven para ver dónde se va la espera de un mensaje.
    try:
        op = req.get("op", "?") if isinstance(req, dict) else "?"
        append_log(os.path.join(BASE, "gateway.log"),
                   "%s op=%s deps=%.2fs run=%.2fs total=%.2fs" % (
                       time.strftime("%Y-%m-%dT%H:%M:%S"), op, t_deps - t_start, t_end - t_deps, t_end - t_start))
    except OSError:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
