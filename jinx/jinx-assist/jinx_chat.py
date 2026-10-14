"""Gateway de chat de Jinx para los teléfonos (comando forzado por ssh).

Lee UNA línea JSON de stdin y escribe eventos NDJSON en stdout. Ver
docs/superpowers/specs/2026-10-13-jinx-assistant-text-design.md.
No hace getUpdates ni toca webhooks: sólo sendMessage.
"""
import contextlib
import fcntl
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import re
import subprocess
import sys
import time
import urllib.request
from dataclasses import dataclass
from typing import Callable, Optional

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from jinx_thread import Thread  # noqa: E402

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
        text = str(m.get("text", ""))
        if not text.strip() or (role, text.strip()) in known:
            continue
        src = "jinx" if role == "jinx" else "tg"
        incoming.append({"id": "h%s:%s" % (m.get("id"), src), "ts": float(m.get("ts") or 0), "src": src,
                         "text": text, "cid": None, "mirrored": True})
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
            yield {"ev": "done", "mode": "cached", "text": cached["text"],
                   "mirrored": _settle_mirror(deps, ids)}
            return
        deps.thread.append(src, text, cid=cid)
        yield {"ev": "ack"}
        try:
            mode, out = deps.brain.reply(deps.thread.context())
        except Exception as e:  # noqa: BLE001 - el modelo puede fallar de mil formas
            yield _err("no pude generar respuesta: %s" % e)
            return
        out = (out or "").strip() or "(sin respuesta)"
        line = deps.thread.append("jinx", out, cid=cid)
        yield {"ev": "done", "mode": mode, "text": line["text"], "mirrored": _settle_mirror(deps, ids)}


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
        added = deps.thread.merge(lines)
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

class TelegramMirror:
    def __init__(self, token, chat, opener=urllib.request.urlopen):
        self.token, self.chat, self.opener = token, chat, opener
        self.last_error = ""

    def send(self, text):
        if not self.token or not self.chat or self.opener is None:
            self.last_error = "sin token o chat"
            return False
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

    def __init__(self, token_fn, chat_fn, opener=urllib.request.urlopen):
        super().__init__("", "", opener)
        self._token_fn, self._chat_fn, self._resolved = token_fn, chat_fn, False

    def send(self, text):
        if not self._resolved:
            self.token, self.chat = self._token_fn() or "", self._chat_fn() or ""
            self._resolved = True
        return super().send(text)


def _load_jinx():
    path = os.path.join(HOME, "bin", "jinx")
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


HERMES_SSH = os.environ.get("JINX_HERMES_SSH", "server")
HERMES_CMD = os.environ.get(
    "JINX_HERMES_CMD",
    "/home/alan/.hermes/hermes-agent/venv/bin/python /home/alan/.hermes/scripts/jinx_hermes_chat.py")


def ssh_runner(payload, timeout=85):
    """Una petición al puente de Hermes por ssh; devuelve el evento JSON o lanza si no hay respuesta."""
    p = subprocess.run(
        ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=6", HERMES_SSH, HERMES_CMD],
        input=json.dumps(payload) + "\n", capture_output=True, text=True, timeout=timeout)
    lines = [ln for ln in p.stdout.splitlines() if ln.strip()]
    if not lines:
        raise RuntimeError("Hermes no respondió (%s)" % (p.stderr.strip().splitlines() or ["sin salida"])[-1][:80])
    return json.loads(lines[-1])


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


def build_real_deps(base=BASE, defer=False):
    jx = _load_jinx()
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
                base, defer_mirror=defer_if_needed if defer else None, hermes=hermes)


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


def main(stdin=sys.stdin, stdout=sys.stdout):
    if "--flush-mirror" in sys.argv[1:]:
        return flush_mirror_main()
    raw = stdin.readline()
    try:
        req = json.loads(raw) if raw.strip() else {}
    except ValueError:
        req = {}
    # Sólo se construye el cerebro real si la op lo necesita (history no).
    needs_real = isinstance(req, dict) and req.get("op") in ("send", "core", "provision", "merge")
    t_start = time.time()
    try:
        deps = build_real_deps(defer=True) if needs_real else Deps(
            Thread(BASE), None, TelegramMirror("", ""), "", "", dict, BASE)
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
