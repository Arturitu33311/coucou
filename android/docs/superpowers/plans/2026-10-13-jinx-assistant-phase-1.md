# Jinx digital assistant — phases 1a + 1b Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Jinx can be chosen as the Samsung's digital assistant; a panel gives two-way text in the same thread as Telegram, answered by the laptop when reachable and by the phone itself (OpenRouter) when not, with a spike that measures an on-device model for phase 1c.

**Architecture:** A shared, flock-protected `thread.jsonl` on the laptop is the source of truth that `jinx-telegram` and a new `jinx-chat` gateway both use; the phone reaches `jinx-chat` through the existing restricted ssh key via a `coucou-dispatch` forced command. The app has its own `ThreadStore`, persona cache, cloud brain and Telegram mirror so it works with no laptop; the thread reconciles by `merge` when the laptop is back.

**Tech Stack:** Python 3.12 stdlib (`unittest`, no pytest installed), Kotlin + Compose, sshj (already in), `org.json`, Android `VoiceInteractionService`, Keystore AES-GCM.

**Spec:** `docs/superpowers/specs/2026-10-13-jinx-assistant-text-design.md`

## Global Constraints

- minSdk 26, compileSdk/targetSdk 36; lint is `abortOnError` + `warningsAsErrors` (a new exported service needs a permission or an explicit `tools:ignore`).
- Kotlin sources carry the header `Coucou for Android — part of this SmartIsland fork (GNU GPL v3).` and live under `app/src/main/java/com/agupta07505/smartisland/coucou/assist/`; tests under `app/src/test/java/com/agupta07505/smartisland/coucou/`.
- Build: `JAVA_HOME=~/Android/jdk21 ./gradlew --offline :app:testDebugUnitTest` / `:app:assembleDebug` (system Java is JRE-only).
- Never `adb uninstall` (it wipes the Keystore ssh key); install with `install -r` on **both** phones: S21 `100.78.102.31:<port>`, S22 `100.100.137.97:<port>` (Wireless-debugging port changes; ask the user if `adb connect` fails). **Never inject taps on the phones.**
- Never push to `origin` (upstream); push to `fork`. Commit identity flags: `git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com"` (no git identity is configured).
- The phone's ssh key may only ever run `coucou-dispatch`, which runs exactly two fixed programs; no shell, no client-supplied arguments reach a command line.
- Telegram: never `deleteWebhook` / `setWebhook` / `logOut`; only `sendMessage` from the gateway and the app. The relay stays the only `getUpdates` consumer.
- Secrets (OpenRouter key, Telegram token, chat id) never go in the APK, in logs, or in `thread.jsonl`; on the phone they live Keystore-wrapped.
- Thread line format (identical in Python and Kotlin): `{"id","ts","src","text","cid","mirrored"}`; `src` ∈ `tg|s21|s22|jinx`; `id` is `"<cid>:<src>"` when there is a `cid`, else a uuid.
- Mirror prefixes: `📱 ` for Alan's text typed at a phone, `🟠 ` for Jinx's answers. Lines that came from Telegram (`src: tg`) are stored `mirrored: true`.
- The model's context is always the last 20 lines of the thread (`HIST_MAX = 20`); answers are capped at 400 generated tokens on the phone.

## Review Focus

- A request line that is not JSON, is empty, or has `op` missing/unknown → one `error` event, exit 0, nothing written to the thread.
- The same `cid` sent twice (connection dropped after the model ran) → the stored answer is returned and the model is **not** called again; the user line is not duplicated.
- Two writers (relay and gateway, or two gateway calls) appending at once → no lost or interleaved lines (flock + atomic rewrite).
- Telegram unreachable or 4xx → the answer is still delivered to the phone; the lines stay `mirrored:false` and are mirrored later (by `merge` or the next send).
- Phone clock differs from laptop clock → merge keeps both lines, orders by `ts` stably, never drops a line because of its timestamp.
- Text with newlines, emoji, quotes or 10 000 characters → round-trips through JSON in both languages; Telegram text is cut at 4000 chars, the thread keeps it whole.
- `provision` when the flag file is absent → refused; when present → returns the secrets once and removes the flag.
- Phone with no laptop, no internet, no downloaded model → the user line stays visible as pending and no answer is invented.

---

## File Structure

Laptop side (new directory in the repo, installed to `~/bin` by a script):

- `scripts/jinx-assist/jinx_thread.py` — shared thread: lock, append, merge, context, mirrored flags.
- `scripts/jinx-assist/jinx_chat.py` — the gateway: request handling, brain, Telegram mirror, `main`.
- `scripts/jinx-assist/jinx-chat` — 4-line launcher for `jinx_chat.main`.
- `scripts/jinx-assist/coucou-dispatch` — forced command: routes `jinx-chat` or `coucou-hook --remote`.
- `scripts/jinx-assist/install.sh` — copies the four files to `~/bin` with backups.
- `scripts/jinx-assist/test_jinx_thread.py`, `test_jinx_chat.py`, `test_dispatch.py`.

Android (`coucou/assist/`):

- `ChatModels.kt` — `ChatLine`, `ChatEvent`, `ChatException`, `ChatProtocol` (pure).
- `ThreadStore.kt` — local jsonl thread.
- `ChatApi.kt` — `ChatApi` interface + `LaptopChatApi` (ssh).
- `ChatRepository.kt` — `Via`, `SendResult`, `LocalBrain`, `Mirror`, `ChatRepository`.
- `Persona.kt` — `Persona`, `PersonaStore`, built-in persona.
- `OpenRouterBrain.kt` — cloud `LocalBrain`.
- `Secrets.kt` — `Secrets` interface, `KeystoreSecrets`, `TelegramMirror`.
- `AssistService.kt` — `JinxVoiceInteractionService`, `JinxSessionService`, `JinxSession`, `JinxRecognitionService`.
- `JinxAssistActivity.kt` — the panel (Compose) and `AssistModel`.
- `res/xml/jinx_voice_interaction.xml`, manifest entries, `docs/ASSISTANT.md`.

---

## Task 1: `jinx_thread.py` — the shared thread

**Files:**
- Create: `scripts/jinx-assist/jinx_thread.py`
- Test: `scripts/jinx-assist/test_jinx_thread.py`

**Interfaces:**
- Produces (used by Task 2, 4): `class Thread(base_dir)` with `read() -> list[dict]`, `append(src, text, cid=None, ts=None, mirrored=False) -> dict`, `answer_for(cid) -> dict|None`, `context(n=HIST_MAX) -> list[{"role","content"}]`, `merge(incoming: list[dict]) -> list[dict]` (returns the lines actually added), `since(have_ids: set[str], limit=200) -> list[dict]`, `unmirrored() -> list[dict]`, `mark_mirrored(ids: set[str]) -> None`; constant `HIST_MAX = 20`.

- [ ] **Step 1: Write the failing test**

```python
# scripts/jinx-assist/test_jinx_thread.py
import os
import sys
import tempfile
import threading
import unittest

sys.path.insert(0, os.path.dirname(__file__))
from jinx_thread import Thread, HIST_MAX


class ThreadTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.t = Thread(self.dir.name)

    def tearDown(self):
        self.dir.cleanup()

    def test_append_and_read_roundtrip_with_emoji_and_newlines(self):
        d = self.t.append("s21", "hola\n\"mundo\" 😀", cid="c1", ts=10.0)
        self.assertEqual(d["id"], "c1:s21")
        self.assertEqual(self.t.read()[0]["text"], "hola\n\"mundo\" 😀")

    def test_same_cid_and_src_is_not_duplicated(self):
        self.t.append("s21", "uno", cid="c1", ts=1.0)
        self.t.append("s21", "uno otra vez", cid="c1", ts=2.0)
        self.assertEqual(len(self.t.read()), 1)
        self.assertEqual(self.t.read()[0]["text"], "uno")

    def test_answer_for_finds_the_jinx_line_of_that_cid(self):
        self.t.append("s21", "pregunta", cid="c1", ts=1.0)
        self.assertIsNone(self.t.answer_for("c1"))
        self.t.append("jinx", "respuesta", cid="c1", ts=2.0)
        self.assertEqual(self.t.answer_for("c1")["text"], "respuesta")

    def test_context_maps_roles_and_trims_to_hist_max(self):
        for i in range(30):
            self.t.append("tg", "u%d" % i, ts=float(i))
        ctx = self.t.context()
        self.assertEqual(len(ctx), HIST_MAX)
        self.assertEqual(ctx[-1], {"role": "user", "content": "u29"})
        self.t.append("jinx", "a", cid="x", ts=100.0)
        self.assertEqual(self.t.context()[-1], {"role": "assistant", "content": "a"})

    def test_malformed_lines_are_skipped(self):
        self.t.append("tg", "ok", ts=1.0)
        with open(self.t.path, "a", encoding="utf-8") as f:
            f.write("{no json\n\n[1,2]\n")
        self.assertEqual([d["text"] for d in self.t.read()], ["ok"])

    def test_merge_adds_missing_dedupes_and_orders_by_ts(self):
        self.t.append("tg", "b", ts=20.0)
        incoming = [
            {"id": "c1:s22", "ts": 10.0, "src": "s22", "text": "a", "cid": "c1", "mirrored": False},
            {"id": "c1:s22", "ts": 10.0, "src": "s22", "text": "a", "cid": "c1", "mirrored": False},
            {"id": "", "ts": 1.0, "src": "s22", "text": "sin id"},
        ]
        added = self.t.merge(incoming)
        self.assertEqual([d["id"] for d in added], ["c1:s22"])
        self.assertEqual([d["text"] for d in self.t.read()], ["a", "b"])

    def test_since_returns_only_unknown_lines(self):
        a = self.t.append("tg", "a", ts=1.0)
        self.t.append("tg", "b", ts=2.0)
        self.assertEqual([d["text"] for d in self.t.since({a["id"]})], ["b"])

    def test_mark_mirrored_and_unmirrored(self):
        a = self.t.append("s21", "a", cid="c", ts=1.0)
        self.assertEqual(len(self.t.unmirrored()), 1)
        self.t.mark_mirrored({a["id"]})
        self.assertEqual(self.t.unmirrored(), [])

    def test_concurrent_appends_lose_nothing(self):
        def work(n):
            for i in range(25):
                self.t.append("tg", "%d-%d" % (n, i))
        threads = [threading.Thread(target=work, args=(n,)) for n in range(4)]
        [x.start() for x in threads]
        [x.join() for x in threads]
        self.assertEqual(len(self.t.read()), 100)


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /home/alan/dev/smartisland-coucou/.claude/worktrees/fix-tertiary && python3 -m unittest discover -s scripts/jinx-assist -p 'test_jinx_thread.py' -v`
Expected: FAIL / ERROR `ModuleNotFoundError: No module named 'jinx_thread'`

- [ ] **Step 3: Write minimal implementation**

```python
# scripts/jinx-assist/jinx_thread.py
"""Hilo compartido de la conversación de Jinx (Telegram + teléfonos + gateway).

Una línea JSON por mensaje en <base>/thread.jsonl:
  {"id","ts","src","text","cid","mirrored"}
src: tg | s21 | s22 | jinx.  id = "<cid>:<src>" si hay cid, si no un uuid.
Todas las operaciones toman un flock exclusivo y reescriben de forma atómica.
"""
import contextlib
import fcntl
import json
import os
import time
import uuid

HIST_MAX = 20


def _line_id(src, cid):
    return "%s:%s" % (cid, src) if cid else uuid.uuid4().hex


def _valid(d):
    return (isinstance(d, dict) and isinstance(d.get("id"), str) and d["id"]
            and isinstance(d.get("text"), str) and isinstance(d.get("ts"), (int, float))
            and isinstance(d.get("src"), str))


class Thread:
    def __init__(self, base):
        self.base = base
        os.makedirs(base, exist_ok=True)
        self.path = os.path.join(base, "thread.jsonl")
        self._lock_path = os.path.join(base, "thread.lock")

    @contextlib.contextmanager
    def _locked(self):
        with open(self._lock_path, "a+") as lf:
            fcntl.flock(lf, fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(lf, fcntl.LOCK_UN)

    def _read(self):
        out = []
        try:
            with open(self.path, encoding="utf-8") as f:
                for raw in f:
                    raw = raw.strip()
                    if not raw:
                        continue
                    try:
                        d = json.loads(raw)
                    except ValueError:
                        continue
                    if _valid(d):
                        d.setdefault("cid", None)
                        d.setdefault("mirrored", False)
                        out.append(d)
        except FileNotFoundError:
            pass
        return out

    def _write(self, lines):
        tmp = self.path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            for d in lines:
                f.write(json.dumps(d, ensure_ascii=False) + "\n")
        os.replace(tmp, self.path)

    def read(self):
        with self._locked():
            return self._read()

    def append(self, src, text, cid=None, ts=None, mirrored=False):
        with self._locked():
            lines = self._read()
            lid = _line_id(src, cid)
            for d in lines:
                if d["id"] == lid:
                    return d
            d = {"id": lid, "ts": time.time() if ts is None else ts, "src": src,
                 "text": text, "cid": cid, "mirrored": mirrored}
            lines.append(d)
            self._write(lines)
            return d

    def answer_for(self, cid):
        want = "%s:jinx" % cid
        for d in self.read():
            if d["id"] == want:
                return d
        return None

    def context(self, n=HIST_MAX):
        return [{"role": "assistant" if d["src"] == "jinx" else "user", "content": d["text"]}
                for d in self.read()[-n:]]

    def merge(self, incoming):
        with self._locked():
            lines = self._read()
            known = {d["id"] for d in lines}
            added = []
            for d in incoming:
                if not _valid(d) or d["id"] in known:
                    continue
                d = {"id": d["id"], "ts": d["ts"], "src": d["src"], "text": d["text"],
                     "cid": d.get("cid"), "mirrored": bool(d.get("mirrored", False))}
                known.add(d["id"])
                lines.append(d)
                added.append(d)
            if added:
                lines.sort(key=lambda d: d["ts"])  # estable: empates conservan el orden
                self._write(lines)
            return added

    def since(self, have_ids, limit=200):
        return [d for d in self.read() if d["id"] not in have_ids][-limit:]

    def unmirrored(self):
        return [d for d in self.read() if not d["mirrored"]]

    def mark_mirrored(self, ids):
        with self._locked():
            lines = self._read()
            changed = False
            for d in lines:
                if d["id"] in ids and not d["mirrored"]:
                    d["mirrored"] = True
                    changed = True
            if changed:
                self._write(lines)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python3 -m unittest discover -s scripts/jinx-assist -p 'test_jinx_thread.py' -v`
Expected: PASS, 9 tests OK

- [ ] **Step 5: Commit**

```bash
git add scripts/jinx-assist/jinx_thread.py scripts/jinx-assist/test_jinx_thread.py
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "jinx-assist: shared flock'd conversation thread"
```

---

## Task 2: `jinx_chat.py` — the gateway

**Files:**
- Create: `scripts/jinx-assist/jinx_chat.py`, `scripts/jinx-assist/jinx-chat`
- Test: `scripts/jinx-assist/test_jinx_chat.py`

**Interfaces:**
- Consumes: `Thread` from Task 1.
- Produces (protocol the app implements in Task 6): one JSON request line on stdin, NDJSON events on stdout.
  - `{"op":"history","n":40}` → `{"ev":"history","items":[line…]}`
  - `{"op":"send","text":"…","cid":"…","src":"s21"}` → `{"ev":"ack"}` then `{"ev":"done","mode":"or|local|cached","text":"…"}` or `{"ev":"error","msg":"…"}`
  - `{"op":"merge","lines":[line…],"have":["id",…]}` → `{"ev":"merged","added":N,"items":[lines the caller lacks],"mirrored_ids":[ids]}`
  - `{"op":"core"}` → `{"ev":"core","core":"…","memory":"…"}`
  - `{"op":"provision"}` → `{"ev":"secrets","openrouter_key":"…","telegram_token":"…","chat":"…"}` only if `<base>/ALLOW_PROVISION` exists (deleted on use), else `{"ev":"error","msg":"provision no autorizado"}`
- Python API: `handle(req: dict, deps: Deps) -> Iterator[dict]`; `Deps(thread, brain, mirror, core_text, memory_text, secrets, base)` where `brain.reply(messages) -> (mode, text)` raises on failure, `mirror.send(text) -> bool`, `secrets()` -> dict.

- [ ] **Step 1: Write the failing test**

```python
# scripts/jinx-assist/test_jinx_chat.py
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
from jinx_thread import Thread
import jinx_chat
from jinx_chat import Deps, handle, run_line


class FakeBrain:
    def __init__(self, text="respuesta", fail=False):
        self.calls = []
        self.text = text
        self.fail = fail

    def reply(self, messages):
        self.calls.append(messages)
        if self.fail:
            raise RuntimeError("modelo caído")
        return "or", self.text


class FakeMirror:
    def __init__(self, ok=True):
        self.ok = ok
        self.sent = []

    def send(self, text):
        self.sent.append(text)
        return self.ok


class GatewayTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.thread = Thread(self.dir.name)
        self.brain = FakeBrain()
        self.mirror = FakeMirror()
        self.deps = Deps(self.thread, self.brain, self.mirror, "CORE", "MEM",
                         lambda: {"openrouter_key": "k", "telegram_token": "t", "chat": "42"},
                         self.dir.name)

    def tearDown(self):
        self.dir.cleanup()

    def events(self, req):
        return list(handle(req, self.deps))

    def test_send_answers_stores_both_lines_and_mirrors(self):
        ev = self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        self.assertEqual(ev[0], {"ev": "ack"})
        self.assertEqual(ev[-1], {"ev": "done", "mode": "or", "text": "respuesta"})
        ids = [d["id"] for d in self.thread.read()]
        self.assertEqual(ids, ["c1:s21", "c1:jinx"])
        self.assertEqual(self.mirror.sent, ["📱 hola", "🟠 respuesta"])
        self.assertEqual(self.thread.unmirrored(), [])

    def test_same_cid_twice_does_not_call_the_model_again(self):
        self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        ev = self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        self.assertEqual(len(self.brain.calls), 1)
        self.assertEqual(ev[-1], {"ev": "done", "mode": "cached", "text": "respuesta"})
        self.assertEqual(len(self.thread.read()), 2)

    def test_context_given_to_the_model_includes_telegram_lines(self):
        self.thread.append("tg", "dime algo desde telegram", ts=1.0, mirrored=True)
        self.events({"op": "send", "text": "y ahora", "cid": "c2", "src": "s21"})
        sent = self.brain.calls[0]
        self.assertEqual([m["content"] for m in sent], ["dime algo desde telegram", "y ahora"])

    def test_model_failure_gives_error_and_no_jinx_line(self):
        self.brain.fail = True
        ev = self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        self.assertEqual(ev[-1]["ev"], "error")
        self.assertIn("modelo caído", ev[-1]["msg"])
        self.assertEqual([d["id"] for d in self.thread.read()], ["c1:s21"])

    def test_mirror_failure_still_answers_and_leaves_lines_unmirrored(self):
        self.mirror.ok = False
        ev = self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        self.assertEqual(ev[-1]["ev"], "done")
        self.assertEqual(len(self.thread.unmirrored()), 2)

    def test_next_send_mirrors_what_was_left_behind(self):
        self.mirror.ok = False
        self.events({"op": "send", "text": "uno", "cid": "c1", "src": "s21"})
        self.mirror.ok = True
        self.mirror.sent.clear()
        self.events({"op": "send", "text": "dos", "cid": "c2", "src": "s21"})
        self.assertEqual(self.thread.unmirrored(), [])
        self.assertEqual(self.mirror.sent[:2], ["📱 uno", "🟠 respuesta"])

    def test_telegram_text_is_cut_but_thread_keeps_it_whole(self):
        long = "x" * 10000
        self.events({"op": "send", "text": long, "cid": "c1", "src": "s21"})
        self.assertEqual(len(self.mirror.sent[0]), 4000)
        self.assertEqual(self.thread.read()[0]["text"], long)

    def test_send_without_text_or_cid_is_an_error(self):
        self.assertEqual(self.events({"op": "send", "cid": "c"})[-1]["ev"], "error")
        self.assertEqual(self.events({"op": "send", "text": "x"})[-1]["ev"], "error")
        self.assertEqual(self.thread.read(), [])

    def test_unknown_src_is_rejected(self):
        ev = self.events({"op": "send", "text": "x", "cid": "c", "src": "evil"})
        self.assertEqual(ev[-1]["ev"], "error")

    def test_history_returns_last_n(self):
        for i in range(5):
            self.thread.append("tg", "m%d" % i, ts=float(i))
        ev = self.events({"op": "history", "n": 2})
        self.assertEqual([d["text"] for d in ev[0]["items"]], ["m3", "m4"])

    def test_merge_adds_lines_returns_missing_and_mirrors_unmirrored(self):
        self.thread.append("tg", "de telegram", ts=1.0, mirrored=True)
        incoming = [{"id": "c9:s22", "ts": 5.0, "src": "s22", "text": "sin red",
                     "cid": "c9", "mirrored": False}]
        ev = self.events({"op": "merge", "lines": incoming, "have": []})[-1]
        self.assertEqual(ev["ev"], "merged")
        self.assertEqual(ev["added"], 1)
        self.assertEqual([d["text"] for d in ev["items"]], ["de telegram", "sin red"])
        self.assertEqual(ev["mirrored_ids"], ["c9:s22"])
        self.assertEqual(self.mirror.sent, ["📱 sin red"])

    def test_core_returns_persona_and_memory(self):
        self.assertEqual(self.events({"op": "core"})[-1],
                         {"ev": "core", "core": "CORE", "memory": "MEM"})

    def test_provision_is_one_shot_and_gated_by_flag_file(self):
        self.assertEqual(self.events({"op": "provision"})[-1]["ev"], "error")
        flag = os.path.join(self.dir.name, "ALLOW_PROVISION")
        open(flag, "w").close()
        ev = self.events({"op": "provision"})[-1]
        self.assertEqual(ev, {"ev": "secrets", "openrouter_key": "k",
                              "telegram_token": "t", "chat": "42"})
        self.assertFalse(os.path.exists(flag))
        self.assertEqual(self.events({"op": "provision"})[-1]["ev"], "error")

    def test_run_line_survives_garbage(self):
        for raw in ["", "   ", "{no", "[1]", '{"op":"nope"}', '{"nop":1}']:
            out = list(run_line(raw, self.deps))
            self.assertEqual(len(out), 1, raw)
            self.assertEqual(out[0]["ev"], "error", raw)
        self.assertEqual(self.thread.read(), [])


class TelegramMirrorTest(unittest.TestCase):
    def test_posts_sendmessage_and_reports_ok(self):
        seen = {}

        class Resp:
            status = 200

            def read(self):
                return b'{"ok":true}'

            def __enter__(self):
                return self

            def __exit__(self, *a):
                return False

        def opener(req, timeout=0):
            seen["url"] = req.full_url
            seen["body"] = req.data
            return Resp()

        m = jinx_chat.TelegramMirror("TOK", "42", opener=opener)
        self.assertTrue(m.send("hola"))
        self.assertTrue(seen["url"].endswith("/botTOK/sendMessage"))
        self.assertIn(b'"chat_id": "42"', seen["body"])

    def test_network_error_is_false_not_an_exception(self):
        def opener(req, timeout=0):
            raise OSError("sin red")
        self.assertFalse(jinx_chat.TelegramMirror("T", "1", opener=opener).send("x"))

    def test_no_token_means_no_mirror(self):
        self.assertFalse(jinx_chat.TelegramMirror("", "", opener=None).send("x"))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python3 -m unittest discover -s scripts/jinx-assist -p 'test_jinx_chat.py' -v`
Expected: ERROR `ModuleNotFoundError: No module named 'jinx_chat'`

- [ ] **Step 3: Write minimal implementation**

```python
# scripts/jinx-assist/jinx_chat.py
"""Gateway de chat de Jinx para los teléfonos (comando forzado por ssh).

Lee UNA línea JSON de stdin y escribe eventos NDJSON en stdout. Ver
docs/superpowers/specs/2026-10-13-jinx-assistant-text-design.md.
No hace getUpdates ni toca webhooks: sólo sendMessage.
"""
import importlib.machinery
import importlib.util
import json
import os
import subprocess
import sys
import urllib.request
from dataclasses import dataclass
from typing import Callable

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from jinx_thread import Thread  # noqa: E402

HOME = os.path.expanduser("~")
BASE = os.path.join(HOME, "jinx-emergencia", "telegram")
SRCS = ("s21", "s22")
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


def _err(msg):
    return {"ev": "error", "msg": msg}


def _mirror_pending(deps):
    """Intenta espejar, en orden, lo que quedó sin espejar. Para al primer fallo."""
    done = []
    for d in deps.thread.unmirrored():
        prefix = "🟠 " if d["src"] == "jinx" else "📱 "
        if not deps.mirror.send((prefix + d["text"])[:TG_MAX]):
            break
        done.append(d["id"])
    if done:
        deps.thread.mark_mirrored(set(done))
    return done


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
    cached = deps.thread.answer_for(cid)
    if cached:
        yield {"ev": "ack"}
        yield {"ev": "done", "mode": "cached", "text": cached["text"]}
        return
    deps.thread.append(src, text, cid=cid)
    yield {"ev": "ack"}
    try:
        mode, out = deps.brain.reply(deps.thread.context())
    except Exception as e:  # noqa: BLE001 - el modelo puede fallar de mil formas
        yield _err("no pude generar respuesta: %s" % e)
        return
    out = (out or "").strip() or "(sin respuesta)"
    deps.thread.append("jinx", out, cid=cid)
    _mirror_pending(deps)
    yield {"ev": "done", "mode": mode, "text": out}


def handle(req, deps):
    op = req.get("op")
    if op == "send":
        yield from _send(req, deps)
    elif op == "history":
        n = req.get("n", 40)
        n = n if isinstance(n, int) and 0 < n <= 500 else 40
        yield {"ev": "history", "items": deps.thread.read()[-n:]}
    elif op == "merge":
        lines = req.get("lines")
        have = req.get("have")
        if not isinstance(lines, list) or not isinstance(have, list):
            yield _err("merge necesita lines y have")
            return
        added = deps.thread.merge(lines)
        mirrored = _mirror_pending(deps)
        yield {"ev": "merged", "added": len(added),
               "items": deps.thread.since({h for h in have if isinstance(h, str)}),
               "mirrored_ids": [i for i in mirrored if i in {d["id"] for d in lines if isinstance(d, dict)}]}
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

    def send(self, text):
        if not self.token or not self.chat or self.opener is None:
            return False
        data = json.dumps({"chat_id": str(self.chat), "text": text[:TG_MAX]}).encode()
        req = urllib.request.Request(
            "https://api.telegram.org/bot%s/sendMessage" % self.token, data=data,
            headers={"Content-Type": "application/json"})
        try:
            with self.opener(req, timeout=15) as r:
                return r.status == 200
        except Exception:  # noqa: BLE001 - sin red, 4xx, timeout: se reintenta luego
            return False


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
        mode = "or" if (self.key and jx.openrouter_reachable(self.key)) else "local"
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


def _read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def build_real_deps(base=BASE):
    jx = _load_jinx()
    token = os.environ.get("JINX_TG_TOKEN") or os.environ.get("TELEGRAM_BOT_TOKEN") \
        or _vault_get(jx, "TELEGRAM_BOT_TOKEN")
    chat = os.environ.get("JINX_TG_CHAT") or os.environ.get("TELEGRAM_ALLOWED_CHAT") \
        or _vault_get(jx, "TELEGRAM_ALLOWED_CHAT")
    brain = JinxBrain(jx)
    memory = jx.load_synced_memory()[0]
    return Deps(Thread(base), brain, TelegramMirror(token, chat),
                brain.core, memory,
                lambda: {"openrouter_key": brain.key or "", "telegram_token": token, "chat": chat},
                base)


def main(stdin=sys.stdin, stdout=sys.stdout):
    raw = stdin.readline()
    try:
        req = json.loads(raw) if raw.strip() else {}
    except ValueError:
        req = {}
    # Sólo se construye el cerebro real si la op lo necesita (history/merge no).
    needs_real = isinstance(req, dict) and req.get("op") in ("send", "core", "provision", "merge")
    try:
        deps = build_real_deps() if needs_real else Deps(
            Thread(BASE), None, TelegramMirror("", ""), "", "", dict, BASE)
    except Exception as e:  # noqa: BLE001
        stdout.write(json.dumps(_err("gateway sin cerebro: %s" % e)) + "\n")
        return 0
    for ev in run_line(raw, deps):
        stdout.write(json.dumps(ev, ensure_ascii=False) + "\n")
        stdout.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

```python
# scripts/jinx-assist/jinx-chat   (chmod +x)
#!/usr/bin/env python3
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.realpath(__file__)))
import jinx_chat  # noqa: E402

sys.exit(jinx_chat.main())
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python3 -m unittest discover -s scripts/jinx-assist -p 'test_jinx_chat.py' -v`
Expected: PASS, 17 tests OK. Then `chmod +x scripts/jinx-assist/jinx-chat`.

- [ ] **Step 5: Commit**

```bash
git add scripts/jinx-assist/jinx_chat.py scripts/jinx-assist/jinx-chat scripts/jinx-assist/test_jinx_chat.py
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "jinx-assist: jinx-chat gateway (send/history/merge/core/provision)"
```

---

## Task 3: `coucou-dispatch` — the forced command

**Files:**
- Create: `scripts/jinx-assist/coucou-dispatch`
- Test: `scripts/jinx-assist/test_dispatch.py`

**Interfaces:**
- Produces: `route(original: str, home: str, uid: int, exists=os.path.exists) -> tuple[list[str], dict]|None`. `"jinx-chat"` → `[home/bin/jinx-chat]`; anything else (today's phones send `"coucou"`) → `coucou-hook --remote` exactly as the old forced command, so no existing behaviour changes.

- [ ] **Step 1: Write the failing test**

```python
# scripts/jinx-assist/test_dispatch.py
import importlib.machinery
import importlib.util
import os
import unittest

path = os.path.join(os.path.dirname(__file__), "coucou-dispatch")
loader = importlib.machinery.SourceFileLoader("coucou_dispatch", path)
spec = importlib.util.spec_from_loader("coucou_dispatch", loader)
mod = importlib.util.module_from_spec(spec)
loader.exec_module(mod)

HOME = "/home/u"
APP = HOME + "/.local/share/coucou-app/coucou-hook"
OLD = HOME + "/.local/share/coucou/bin/coucou-hook"


class RouteTest(unittest.TestCase):
    def test_jinx_chat_routes_to_the_gateway(self):
        argv, env = mod.route("jinx-chat", HOME, 1000, exists=lambda p: True)
        self.assertEqual(argv, [HOME + "/bin/jinx-chat"])

    def test_coucou_stream_keeps_the_old_command_and_env(self):
        argv, env = mod.route("coucou", HOME, 1000, exists=lambda p: p == APP)
        self.assertEqual(argv, [APP, "--remote"])
        self.assertEqual(env, {"XDG_RUNTIME_DIR": "/run/user/1000"})

    def test_falls_back_to_the_other_hook_path(self):
        argv, env = mod.route("coucou", HOME, 1000, exists=lambda p: p == OLD)
        self.assertEqual(argv, [OLD, "--remote"])

    def test_anything_else_is_the_old_behaviour_never_a_shell(self):
        for cmd in ["", "bash", "ls; rm -rf /", "jinx-chat --x", "coucou-pendientes"]:
            argv, _ = mod.route(cmd, HOME, 1000, exists=lambda p: p == APP)
            self.assertEqual(argv, [APP, "--remote"], cmd)

    def test_no_hook_installed_is_none(self):
        self.assertIsNone(mod.route("coucou", HOME, 1000, exists=lambda p: False))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python3 -m unittest discover -s scripts/jinx-assist -p 'test_dispatch.py' -v`
Expected: ERROR (file `coucou-dispatch` not found)

- [ ] **Step 3: Write minimal implementation**

```python
#!/usr/bin/env python3
# scripts/jinx-assist/coucou-dispatch   (chmod +x)
"""Comando forzado de las llaves de los teléfonos en la laptop.

Antes: command="…/coucou-hook --remote". Ahora este despachador lo reemplaza y
sólo conoce DOS programas fijos. Lo que el cliente pide (SSH_ORIGINAL_COMMAND)
elige entre ellos; nunca se interpreta como línea de comandos.
"""
import os
import sys


def route(original, home, uid, exists=os.path.exists):
    if (original or "").strip() == "jinx-chat":
        return [home + "/bin/jinx-chat"], {}
    for p, env in (
        (home + "/.local/share/coucou-app/coucou-hook", {"XDG_RUNTIME_DIR": "/run/user/%d" % uid}),
        (home + "/.local/share/coucou/bin/coucou-hook", {}),
    ):
        if exists(p):
            return [p, "--remote"], env
    return None


def main():
    home = os.path.expanduser("~")
    r = route(os.environ.get("SSH_ORIGINAL_COMMAND", ""), home, os.getuid())
    if r is None:
        sys.stderr.write("coucou-dispatch: nada que ejecutar\n")
        return 2
    argv, env = r
    os.execve(argv[0], argv, {**os.environ, **env})


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: Run test to verify it passes**

Run: `chmod +x scripts/jinx-assist/coucou-dispatch && python3 -m unittest discover -s scripts/jinx-assist -p 'test_dispatch.py' -v`
Expected: PASS, 5 tests OK

- [ ] **Step 5: Commit**

```bash
git add scripts/jinx-assist/coucou-dispatch scripts/jinx-assist/test_dispatch.py
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "jinx-assist: coucou-dispatch forced command (jinx-chat or coucou-hook --remote)"
```

---

## Task 4: Install on the laptop and make the relay write the thread

**Files:**
- Create: `scripts/jinx-assist/install.sh`
- Modify (outside the repo, backed up first): `~/bin/jinx-telegram`, `~/.ssh/authorized_keys`

**Interfaces:**
- Consumes: Tasks 1–3 artifacts.
- Produces: laptop has `~/bin/{jinx_thread.py,jinx_chat.py,jinx-chat,coucou-dispatch}`; Telegram traffic appears in `~/jinx-emergencia/telegram/thread.jsonl`; phone keys run `coucou-dispatch`.

- [ ] **Step 1: Write the install script**

```bash
#!/usr/bin/env bash
# scripts/jinx-assist/install.sh — copies the laptop side to ~/bin, keeping a backup of what was there.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
stamp="$(date +%Y%m%d-%H%M%S)"
mkdir -p "$HOME/bin"
for f in jinx_thread.py jinx_chat.py jinx-chat coucou-dispatch; do
  if [ -e "$HOME/bin/$f" ]; then cp -p "$HOME/bin/$f" "$HOME/bin/$f.bak-$stamp"; fi
  install -m 0755 "$here/$f" "$HOME/bin/$f"
done
python3 "$HOME/bin/jinx-chat" </dev/null | head -1
echo "ok: instalado en ~/bin (backups .bak-$stamp)"
```

- [ ] **Step 2: Run it and check the gateway answers without a brain**

Run: `chmod +x scripts/jinx-assist/install.sh && scripts/jinx-assist/install.sh`
Expected: prints `{"ev": "error", "msg": "petición vacía"}` then `ok: instalado en ~/bin …`

- [ ] **Step 3: End-to-end `history` against the real thread path**

Run: `echo '{"op":"history","n":3}' | ~/bin/jinx-chat`
Expected: `{"ev": "history", "items": []}` (empty before the relay writes anything)

- [ ] **Step 4: Patch the relay (backup first)**

Run: `cp -p ~/bin/jinx-telegram ~/bin/jinx-telegram.bak-$(date +%Y%m%d)-thread`

Edit `~/bin/jinx-telegram`:

1. Below `import urllib.request` add:
```python
sys.path.insert(0, os.path.join(HOME, "bin"))
from jinx_thread import Thread as ChatThread
```
2. In `run()`, after `brain = Brain(jx)` add `chat_thread = ChatThread(os.path.join(BASE, "telegram"))`.
3. Replace the three lines that handle one message:
```python
                log(jx, "user", "telegram", text)
                mode, out = brain.reply(text)
                log(jx, "assistant", mode, out)
```
with
```python
                log(jx, "user", "telegram", text)
                # El contexto del modelo sale del hilo compartido (lo que se habló en los teléfonos también).
                brain.hist = trim_history(chat_thread.context())
                chat_thread.append("tg", text, mirrored=True)
                mode, out = brain.reply(text)
                chat_thread.append("jinx", out, mirrored=True)
                log(jx, "assistant", mode, out)
```
(`brain.reply` appends the user turn to `hist` itself, which is why the context is taken *before* the `tg` line is appended.)

- [ ] **Step 5: Verify the relay's own checks still pass**

Run: `python3 ~/bin/jinx-telegram --self-check`
Expected: `self-check OK: decide + allowlist + historial + sin webhook calls + import jinx`

- [ ] **Step 6: Ask Alan before the two live changes, then apply them**

These two steps change a running service and the SSH login of both phones. **Stop and ask Alan** ("restart `jinx-telegram.service` and switch the two phone keys to `coucou-dispatch`?"). On a yes:

```bash
systemctl --user restart jinx-telegram.service && systemctl --user is-active jinx-telegram.service
cp -p ~/.ssh/authorized_keys ~/.ssh/authorized_keys.bak-$(date +%Y%m%d)-dispatch
sed -i -E 's#command="[^"]*coucou-hook --remote"#command="/home/alan/bin/coucou-dispatch"#' ~/.ssh/authorized_keys
grep -c 'command="/home/alan/bin/coucou-dispatch"' ~/.ssh/authorized_keys
```
Expected: `active`, then `2`. Confirm Claude Code's phone stream still connects (island shows the laptop as Connected) before moving on.

- [ ] **Step 7: Commit**

```bash
git add scripts/jinx-assist/install.sh
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "jinx-assist: install script; relay writes the shared thread"
```

---

## Task 5: Android protocol and thread store

**Files:**
- Create: `app/src/main/java/com/agupta07505/smartisland/coucou/assist/ChatModels.kt`, `.../assist/ThreadStore.kt`
- Test: `app/src/test/java/com/agupta07505/smartisland/coucou/ChatProtocolTest.kt`, `.../ThreadStoreTest.kt`

**Interfaces:**
- Produces: `ChatLine`, `ChatEvent` (`Ack`, `Done`, `Failure`, `History`, `Merged`, `Core`, `Secrets`), `ChatException`, `ChatProtocol.{history,send,merge,core,provision,parse,expectDone,lineToJson,lineFromJson}`, `ThreadStore.{all,append,merge,markMirrored,unmirrored,tail,answerFor,pending,of}`.

- [ ] **Step 1: Write the failing tests**

```kotlin
// ChatProtocolTest.kt
package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatEvent
import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.ChatProtocol
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class ChatProtocolTest {
    @Test fun sendRequestCarriesTextCidAndSrc() {
        val o = JSONObject(ChatProtocol.send("hola\n\"x\" 😀", "c1", "s22"))
        assertEquals("send", o.getString("op"))
        assertEquals("hola\n\"x\" 😀", o.getString("text"))
        assertEquals("c1", o.getString("cid"))
        assertEquals("s22", o.getString("src"))
    }

    @Test fun parseReadsEventsAndSkipsGarbageLines() {
        val raw = """
            {"ev":"ack"}
            not json
            {"ev":"done","mode":"or","text":"qué onda"}
        """.trimIndent()
        val ev = ChatProtocol.parse(raw)
        assertEquals(listOf(ChatEvent.Ack, ChatEvent.Done("or", "qué onda")), ev)
    }

    @Test fun expectDoneReturnsTheAnswer() {
        val done = ChatProtocol.expectDone("{\"ev\":\"ack\"}\n{\"ev\":\"done\",\"mode\":\"cached\",\"text\":\"x\"}")
        assertEquals("x", done.text)
    }

    @Test fun expectDoneThrowsTheGatewaysError() {
        try {
            ChatProtocol.expectDone("{\"ev\":\"error\",\"msg\":\"modelo caído\"}")
            fail("should throw")
        } catch (e: ChatException) {
            assertTrue(e.message!!.contains("modelo caído"))
        }
    }

    @Test fun expectDoneThrowsOnEmptyOutput() {
        try {
            ChatProtocol.expectDone("")
            fail("should throw")
        } catch (e: ChatException) {
            assertTrue(e.message!!.isNotBlank())
        }
    }

    @Test fun lineRoundTrips() {
        val l = ChatLine("c1:s21", 12.5, "s21", "hola", "c1", false)
        assertEquals(l, ChatProtocol.lineFromJson(ChatProtocol.lineToJson(l)))
    }

    @Test fun mergedAndSecretsParse() {
        val m = ChatProtocol.parse(
            """{"ev":"merged","added":1,"items":[{"id":"a:tg","ts":1,"src":"tg","text":"t","cid":null,"mirrored":true}],"mirrored_ids":["z:s21"]}""",
        ).single() as ChatEvent.Merged
        assertEquals(1, m.added)
        assertEquals("t", m.items.single().text)
        assertEquals(listOf("z:s21"), m.mirroredIds)
        val s = ChatProtocol.parse("""{"ev":"secrets","openrouter_key":"k","telegram_token":"t","chat":"42"}""").single()
        assertEquals(ChatEvent.Secrets("k", "t", "42"), s)
    }
}
```

```kotlin
// ThreadStoreTest.kt
package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.ThreadStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class ThreadStoreTest {
    @get:Rule val tmp = TemporaryFolder()
    private fun store() = ThreadStore(File(tmp.root, "t.jsonl"))
    private fun line(id: String, ts: Double, src: String = "s21", cid: String? = null, mirrored: Boolean = false) =
        ChatLine(id, ts, src, "txt-$id", cid, mirrored)

    @Test fun appendIsIdempotentById() {
        val s = store()
        s.append(line("c1:s21", 1.0, cid = "c1"))
        s.append(line("c1:s21", 2.0, cid = "c1"))
        assertEquals(1, s.all().size)
    }

    @Test fun answerForAndPending() {
        val s = store()
        s.append(line("c1:s21", 1.0, cid = "c1"))
        s.append(line("c2:s21", 2.0, cid = "c2"))
        s.append(line("c1:jinx", 3.0, src = "jinx", cid = "c1"))
        assertNotNull(s.answerFor("c1"))
        assertNull(s.answerFor("c2"))
        assertEquals(listOf("c2:s21"), s.pending().map { it.id })
    }

    @Test fun mergeAddsOnlyUnknownAndOrdersByTs() {
        val s = store()
        s.append(line("b", 20.0))
        val added = s.merge(listOf(line("a", 10.0), line("b", 20.0)))
        assertEquals(1, added)
        assertEquals(listOf("a", "b"), s.all().map { it.id })
    }

    @Test fun unmirroredAndMark() {
        val s = store()
        s.append(line("a", 1.0))
        s.append(line("b", 2.0, mirrored = true))
        assertEquals(listOf("a"), s.unmirrored().map { it.id })
        s.markMirrored(setOf("a"))
        assertEquals(emptyList<String>(), s.unmirrored().map { it.id })
    }

    @Test fun tailReturnsTheLastN() {
        val s = store()
        for (i in 1..5) s.append(line("m$i", i.toDouble()))
        assertEquals(listOf("m4", "m5"), s.tail(2).map { it.id })
    }

    @Test fun survivesACorruptFileAndKeepsAtMost2000Lines() {
        val f = File(tmp.root, "t.jsonl")
        f.writeText("{bad\n")
        val s = ThreadStore(f)
        assertEquals(0, s.all().size)
        for (i in 1..2100) s.append(line("m$i", i.toDouble()))
        assertEquals(2000, s.all().size)
        assertEquals("m2100", s.all().last().id)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `JAVA_HOME=~/Android/jdk21 ./gradlew --offline :app:testDebugUnitTest --tests '*ChatProtocolTest' --tests '*ThreadStoreTest'`
Expected: FAIL (compilation error: unresolved `assist` package)

- [ ] **Step 3: Write the implementation**

```kotlin
// ChatModels.kt
/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import org.json.JSONArray
import org.json.JSONObject

/** One message of the shared conversation. `src` is tg | s21 | s22 | jinx. */
data class ChatLine(
    val id: String,
    val ts: Double,
    val src: String,
    val text: String,
    val cid: String?,
    val mirrored: Boolean,
) {
    val fromJinx: Boolean get() = src == "jinx"
}

sealed interface ChatEvent {
    data object Ack : ChatEvent
    data class Done(val mode: String, val text: String) : ChatEvent
    data class Failure(val msg: String) : ChatEvent
    data class History(val items: List<ChatLine>) : ChatEvent
    data class Merged(val added: Int, val items: List<ChatLine>, val mirroredIds: List<String>) : ChatEvent
    data class Core(val core: String, val memory: String) : ChatEvent
    data class Secrets(val openrouterKey: String, val telegramToken: String, val chat: String) : ChatEvent
}

class ChatException(message: String) : Exception(message)

/** The requests and events `jinx-chat` on the laptop speaks (see the spec). Pure: tested on the JVM. */
object ChatProtocol {
    fun history(n: Int): String = JSONObject().put("op", "history").put("n", n).toString()

    fun send(text: String, cid: String, src: String): String =
        JSONObject().put("op", "send").put("text", text).put("cid", cid).put("src", src).toString()

    fun merge(lines: List<ChatLine>, have: List<String>): String =
        JSONObject().put("op", "merge")
            .put("lines", JSONArray().also { a -> lines.forEach { a.put(lineToJson(it)) } })
            .put("have", JSONArray(have))
            .toString()

    fun core(): String = JSONObject().put("op", "core").toString()
    fun provision(): String = JSONObject().put("op", "provision").toString()

    fun lineToJson(l: ChatLine): JSONObject = JSONObject()
        .put("id", l.id).put("ts", l.ts).put("src", l.src).put("text", l.text)
        .put("cid", l.cid ?: JSONObject.NULL).put("mirrored", l.mirrored)

    fun lineFromJson(o: JSONObject): ChatLine? {
        val id = o.optString("id").takeIf { it.isNotEmpty() } ?: return null
        val src = o.optString("src").takeIf { it.isNotEmpty() } ?: return null
        if (!o.has("text") || !o.has("ts")) return null
        return ChatLine(
            id = id,
            ts = o.optDouble("ts"),
            src = src,
            text = o.optString("text"),
            cid = if (o.isNull("cid")) null else o.optString("cid").takeIf { it.isNotEmpty() },
            mirrored = o.optBoolean("mirrored", false),
        )
    }

    private fun lines(arr: JSONArray?): List<ChatLine> =
        (0 until (arr?.length() ?: 0)).mapNotNull { i -> arr!!.optJSONObject(i)?.let(::lineFromJson) }

    fun parse(raw: String): List<ChatEvent> = raw.lineSequence().mapNotNull { l ->
        val o = runCatching { JSONObject(l.trim()) }.getOrNull() ?: return@mapNotNull null
        when (o.optString("ev")) {
            "ack" -> ChatEvent.Ack
            "done" -> ChatEvent.Done(o.optString("mode"), o.optString("text"))
            "error" -> ChatEvent.Failure(o.optString("msg", "error"))
            "history" -> ChatEvent.History(lines(o.optJSONArray("items")))
            "merged" -> ChatEvent.Merged(
                o.optInt("added"),
                lines(o.optJSONArray("items")),
                o.optJSONArray("mirrored_ids")?.let { a -> (0 until a.length()).map { a.getString(it) } }.orEmpty(),
            )
            "core" -> ChatEvent.Core(o.optString("core"), o.optString("memory"))
            "secrets" -> ChatEvent.Secrets(o.optString("openrouter_key"), o.optString("telegram_token"), o.optString("chat"))
            else -> null
        }
    }.toList()

    /** The answer of a `send`, or the gateway's error as an exception. */
    fun expectDone(raw: String): ChatEvent.Done {
        val ev = parse(raw)
        ev.filterIsInstance<ChatEvent.Failure>().firstOrNull()?.let { throw ChatException(it.msg) }
        return ev.filterIsInstance<ChatEvent.Done>().firstOrNull()
            ?: throw ChatException("Jinx no contestó")
    }

    inline fun <reified T : ChatEvent> expect(raw: String): T {
        val ev = parse(raw)
        ev.filterIsInstance<ChatEvent.Failure>().firstOrNull()?.let { throw ChatException(it.msg) }
        return ev.filterIsInstance<T>().firstOrNull() ?: throw ChatException("Jinx no contestó")
    }
}
```

```kotlin
// ThreadStore.kt
/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Context
import org.json.JSONObject
import java.io.File

/**
 * This phone's copy of the conversation: one JSON line per message, the same format as the laptop's
 * `thread.jsonl`, written through a temporary file so a crash never leaves half of it.
 */
class ThreadStore(private val file: File) {
    @Synchronized
    fun all(): List<ChatLine> {
        if (!file.exists()) return emptyList()
        return file.readLines().mapNotNull { l ->
            runCatching { JSONObject(l) }.getOrNull()?.let(ChatProtocol::lineFromJson)
        }
    }

    @Synchronized
    private fun write(lines: List<ChatLine>) {
        val kept = if (lines.size > MAX_LINES) lines.takeLast(MAX_LINES) else lines
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(kept.joinToString("\n", postfix = "\n") { ChatProtocol.lineToJson(it).toString() })
        check(tmp.renameTo(file) || (file.delete() && tmp.renameTo(file))) { "Could not save the conversation" }
    }

    @Synchronized
    fun append(line: ChatLine): ChatLine {
        val lines = all()
        lines.firstOrNull { it.id == line.id }?.let { return it }
        write(lines + line)
        return line
    }

    /** Adds the lines this copy lacks; returns how many were added. */
    @Synchronized
    fun merge(incoming: List<ChatLine>): Int {
        val lines = all()
        val known = lines.map { it.id }.toMutableSet()
        val added = incoming.filter { known.add(it.id) }
        if (added.isNotEmpty()) write((lines + added).sortedBy { it.ts })
        return added.size
    }

    @Synchronized
    fun markMirrored(ids: Set<String>) {
        val lines = all()
        if (lines.none { it.id in ids && !it.mirrored }) return
        write(lines.map { if (it.id in ids) it.copy(mirrored = true) else it })
    }

    fun unmirrored(): List<ChatLine> = all().filter { !it.mirrored }
    fun tail(n: Int): List<ChatLine> = all().takeLast(n)
    fun answerFor(cid: String): ChatLine? = all().firstOrNull { it.id == "$cid:jinx" }

    /** What was typed here and has no answer yet. */
    fun pending(): List<ChatLine> {
        val lines = all()
        val answered = lines.filter { it.fromJinx }.mapNotNull { it.cid }.toSet()
        return lines.filter { !it.fromJinx && it.src != "tg" && it.cid != null && it.cid !in answered }
    }

    companion object {
        const val MAX_LINES = 2000
        fun of(context: Context) = ThreadStore(File(context.applicationContext.filesDir, "jinx-thread.jsonl"))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `JAVA_HOME=~/Android/jdk21 ./gradlew --offline :app:testDebugUnitTest --tests '*ChatProtocolTest' --tests '*ThreadStoreTest'`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add app/src/main/java/com/agupta07505/smartisland/coucou/assist app/src/test/java/com/agupta07505/smartisland/coucou/ChatProtocolTest.kt app/src/test/java/com/agupta07505/smartisland/coucou/ThreadStoreTest.kt
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "assist: chat protocol and local thread store"
```

---

## Task 6: `ChatRepository` — laptop first, then the phone's own brain

**Files:**
- Create: `.../assist/ChatRepository.kt`, `.../assist/ChatApi.kt`
- Modify: `app/src/main/java/com/agupta07505/smartisland/coucou/phone/Ssh.kt` (`open` gets a connect timeout parameter)
- Test: `app/src/test/java/com/agupta07505/smartisland/coucou/ChatRepositoryTest.kt`

**Interfaces:**
- Consumes: Task 5 types.
- Produces:
  - `interface ChatApi { fun send(text: String, cid: String, src: String): ChatEvent.Done; fun merge(lines: List<ChatLine>, have: List<String>): ChatEvent.Merged; fun core(): ChatEvent.Core; fun provision(): ChatEvent.Secrets }`
  - `enum class Via { Laptop, Cloud, Local }`
  - `interface LocalBrain { val via: Via; fun reply(context: List<ChatLine>): String }` (throws `ChatException`)
  - `interface Mirror { fun deliver(text: String): Boolean }`
  - `sealed interface SendResult { Answered(line: ChatLine, via: Via); Pending(reason: String) }`
  - `class ChatRepository(store, deviceId, laptop: ChatApi?, brains: List<LocalBrain>, mirror: Mirror?, clock, newCid)` with `send(text): SendResult`, `retryPending(): List<SendResult>`, `sync(): Int` (merge with laptop; returns lines added), `flushMirror()`.
  - `LaptopChatApi(context, profile: () -> SshProfile)`.

- [ ] **Step 1: Write the failing test**

```kotlin
package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatApi
import com.agupta07505.smartisland.coucou.assist.ChatEvent
import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.ChatRepository
import com.agupta07505.smartisland.coucou.assist.LocalBrain
import com.agupta07505.smartisland.coucou.assist.Mirror
import com.agupta07505.smartisland.coucou.assist.SendResult
import com.agupta07505.smartisland.coucou.assist.ThreadStore
import com.agupta07505.smartisland.coucou.assist.Via
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

private class FakeLaptop : ChatApi {
    var down = false
    var answer = "desde la laptop"
    val sent = mutableListOf<String>()
    var mergedWith: List<ChatLine> = emptyList()
    var toGive: List<ChatLine> = emptyList()
    var mirroredIds: List<String> = emptyList()
    override fun send(text: String, cid: String, src: String): ChatEvent.Done {
        if (down) throw ChatException("laptop no disponible")
        sent += "$cid|$text|$src"
        return ChatEvent.Done("or", answer)
    }
    override fun merge(lines: List<ChatLine>, have: List<String>): ChatEvent.Merged {
        if (down) throw ChatException("laptop no disponible")
        mergedWith = lines
        return ChatEvent.Merged(lines.size, toGive, mirroredIds)
    }
    override fun core() = ChatEvent.Core("c", "m")
    override fun provision() = ChatEvent.Secrets("k", "t", "1")
}

private class FakeBrain(override val via: Via, var text: String? = "desde el teléfono") : LocalBrain {
    var calls = 0
    var seen: List<ChatLine> = emptyList()
    override fun reply(context: List<ChatLine>): String {
        calls++
        seen = context
        return text ?: throw ChatException("sin modelo")
    }
}

private class FakeMirror(var ok: Boolean = true) : Mirror {
    val sent = mutableListOf<String>()
    override fun deliver(text: String): Boolean { sent += text; return ok }
}

class ChatRepositoryTest {
    @get:Rule val tmp = TemporaryFolder()
    private var n = 0
    private fun store() = ThreadStore(File(tmp.root, "t.jsonl"))
    private fun repo(
        store: ThreadStore, laptop: ChatApi?, brains: List<LocalBrain>, mirror: Mirror? = null,
    ) = ChatRepository(store, "s21", laptop, brains, mirror, clock = { 100.0 + n }, newCid = { "c${++n}" })

    @Test fun laptopAnswersFirstAndTheLocalBrainIsNotUsed() {
        val s = store(); val lap = FakeLaptop(); val b = FakeBrain(Via.Cloud)
        val r = repo(s, lap, listOf(b)).send("hola") as SendResult.Answered
        assertEquals(Via.Laptop, r.via)
        assertEquals("desde la laptop", r.line.text)
        assertEquals(0, b.calls)
        assertEquals(listOf("c1|hola|s21"), lap.sent)
        assertTrue(s.unmirrored().isEmpty()) // the laptop mirrors by itself
    }

    @Test fun laptopDownFallsBackToCloudThenLocal() {
        val s = store(); val lap = FakeLaptop().apply { down = true }
        val cloud = FakeBrain(Via.Cloud, text = null)
        val local = FakeBrain(Via.Local, text = "sin conexión pero aquí")
        val r = repo(s, lap, listOf(cloud, local)).send("hola") as SendResult.Answered
        assertEquals(Via.Local, r.via)
        assertEquals(1, cloud.calls)
        assertEquals(listOf("hola", "sin conexión pero aquí"), s.all().map { it.text })
    }

    @Test fun contextGivenToTheBrainEndsWithTheUsersLine() {
        val s = store()
        s.append(ChatLine("t1:tg", 1.0, "tg", "de telegram", null, true))
        val b = FakeBrain(Via.Cloud)
        repo(s, null, listOf(b)).send("y ahora")
        assertEquals(listOf("de telegram", "y ahora"), b.seen.map { it.text })
    }

    @Test fun nothingAvailableLeavesThePendingLineAndInventsNothing() {
        val s = store()
        val r = repo(s, null, listOf(FakeBrain(Via.Cloud, text = null)))
        val res = r.send("hola")
        assertTrue(res is SendResult.Pending)
        assertEquals(listOf("c1:s21"), s.pending().map { it.id })
        assertTrue(s.all().none { it.fromJinx })
    }

    @Test fun retryPendingAnswersWhenAModelComesBack() {
        val s = store(); val b = FakeBrain(Via.Cloud, text = null)
        val r = repo(s, null, listOf(b))
        r.send("hola")
        b.text = "ya"
        val res = r.retryPending()
        assertEquals(1, res.size)
        assertTrue(res[0] is SendResult.Answered)
        assertTrue(s.pending().isEmpty())
    }

    @Test fun phoneAnswersAreMirroredAndFailuresStayQueued() {
        val s = store(); val m = FakeMirror(ok = false)
        val r = repo(s, null, listOf(FakeBrain(Via.Cloud)), m)
        r.send("hola")
        assertEquals(2, s.unmirrored().size)
        m.ok = true; m.sent.clear()
        r.flushMirror()
        assertEquals(listOf("📱 hola", "🟠 desde el teléfono"), m.sent)
        assertTrue(s.unmirrored().isEmpty())
    }

    @Test fun syncSendsOurLinesAndTakesTheOnesWeLack() {
        val s = store(); val lap = FakeLaptop()
        val r = repo(s, null, listOf(FakeBrain(Via.Cloud)))
        r.send("sin laptop") // c1: user line + answer, both unmirrored
        lap.toGive = listOf(ChatLine("x:tg", 1.0, "tg", "mensaje de telegram", null, true))
        lap.mirroredIds = listOf("c1:s21", "c1:jinx")
        val sync = repo(s, lap, emptyList())
        assertEquals(1, sync.sync())
        assertEquals(2, lap.mergedWith.size)
        assertTrue(s.all().any { it.id == "x:tg" })
        assertTrue(s.unmirrored().isEmpty())
    }

    @Test fun syncWithTheLaptopDownDoesNotThrow() {
        val s = store(); val lap = FakeLaptop().apply { down = true }
        assertEquals(0, repo(s, lap, emptyList()).sync())
    }

    @Test fun answerAlreadyStoredForACidIsNotAskedAgain() {
        val s = store(); val lap = FakeLaptop(); val r = repo(s, lap, emptyList())
        val first = r.send("hola") as SendResult.Answered
        val again = r.retryPending()
        assertTrue(again.isEmpty())
        assertEquals(1, lap.sent.size)
        assertEquals("desde la laptop", first.line.text)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `JAVA_HOME=~/Android/jdk21 ./gradlew --offline :app:testDebugUnitTest --tests '*ChatRepositoryTest'`
Expected: FAIL (unresolved references `ChatRepository`, `ChatApi`…)

- [ ] **Step 3: Write the implementation**

```kotlin
// ChatRepository.kt
/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import java.util.UUID

enum class Via { Laptop, Cloud, Local }

/** A brain that lives on this phone (cloud through OpenRouter, or the on-device model). */
interface LocalBrain {
    val via: Via
    /** Answers the conversation so far (ending with the user's line); throws [ChatException]. */
    fun reply(context: List<ChatLine>): String
}

/** Where a line is repeated so Telegram keeps the whole conversation. */
interface Mirror {
    fun deliver(text: String): Boolean
}

sealed interface SendResult {
    data class Answered(val line: ChatLine, val via: Via) : SendResult
    data class Pending(val reason: String) : SendResult
}

/**
 * One conversation, whichever brain answers it: the laptop's gateway if it can, else this phone's own
 * brains in order. Everything lands in the local [ThreadStore]; nothing is invented when nobody can answer.
 */
class ChatRepository(
    private val store: ThreadStore,
    private val deviceId: String,
    private val laptop: ChatApi?,
    private val brains: List<LocalBrain>,
    private val mirror: Mirror?,
    private val clock: () -> Double = { System.currentTimeMillis() / 1000.0 },
    private val newCid: () -> String = { UUID.randomUUID().toString() },
) {
    fun send(text: String): SendResult {
        val cid = newCid()
        val user = store.append(ChatLine("$cid:$deviceId", clock(), deviceId, text, cid, false))
        return answer(user)
    }

    /** Asks again for what was typed and never answered (a model came back, or the laptop did). */
    fun retryPending(): List<SendResult> = store.pending().map { answer(it) }

    private fun answer(user: ChatLine): SendResult {
        val cid = user.cid ?: return SendResult.Pending("sin cid")
        store.answerFor(cid)?.let { return SendResult.Answered(it, Via.Laptop) }
        laptop?.let { api ->
            try {
                val done = api.send(user.text, cid, deviceId)
                val line = store.append(ChatLine("$cid:jinx", clock(), "jinx", done.text, cid, true))
                store.markMirrored(setOf(user.id, line.id))
                return SendResult.Answered(line, Via.Laptop)
            } catch (_: ChatException) {
                // falls through to this phone's own brains
            }
        }
        var reason = "sin modelo disponible"
        for (b in brains) {
            try {
                val text = b.reply(store.tail(CONTEXT))
                val line = store.append(ChatLine("$cid:jinx", clock(), "jinx", text, cid, false))
                flushMirror()
                return SendResult.Answered(line, b.via)
            } catch (e: ChatException) {
                reason = e.message ?: reason
            }
        }
        return SendResult.Pending(reason)
    }

    /** Repeats, in order, the lines Telegram has not seen; stops at the first failure. */
    fun flushMirror() {
        val m = mirror ?: return
        val done = mutableSetOf<String>()
        for (l in store.unmirrored()) {
            val prefix = if (l.fromJinx) "🟠 " else "📱 "
            if (!m.deliver((prefix + l.text).take(TG_MAX))) break
            done += l.id
        }
        if (done.isNotEmpty()) store.markMirrored(done)
    }

    /** Hands our lines to the laptop and takes the ones we lack. Returns how many were added here. */
    fun sync(): Int {
        val api = laptop ?: return 0
        return try {
            val ours = store.all()
            val res = api.merge(ours.filter { !it.mirrored || it.src != "tg" }, ours.map { it.id })
            store.markMirrored(res.mirroredIds.toSet())
            store.merge(res.items)
        } catch (_: ChatException) {
            0
        }
    }

    private companion object {
        const val CONTEXT = 20
        const val TG_MAX = 4000
    }
}
```

```kotlin
// ChatApi.kt
/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Context
import com.agupta07505.smartisland.coucou.phone.Ssh
import com.agupta07505.smartisland.coucou.phone.SshException
import com.agupta07505.smartisland.coucou.phone.SshProfile

/** What the laptop's `jinx-chat` can do for the phone. An interface, so the repository is tried without a network. */
interface ChatApi {
    fun send(text: String, cid: String, src: String): ChatEvent.Done
    fun merge(lines: List<ChatLine>, have: List<String>): ChatEvent.Merged
    fun core(): ChatEvent.Core
    fun provision(): ChatEvent.Secrets
}

/** The real thing: one ssh command (`jinx-chat`) per request, with the phone's own key. */
class LaptopChatApi(
    private val context: Context,
    private val profile: () -> SshProfile,
) : ChatApi {
    private fun ask(request: String, timeoutSec: Long): String {
        val conn = try {
            Ssh.open(context, profile(), connectTimeoutMs = PROBE_MS)
        } catch (e: SshException) {
            throw ChatException(e.message ?: "la laptop no responde")
        }
        try {
            return conn.exec("jinx-chat", request + "\n", timeoutSec)
        } catch (e: Exception) {
            throw ChatException(e.message ?: "la laptop no responde")
        } finally {
            conn.close()
        }
    }

    override fun send(text: String, cid: String, src: String) =
        ChatProtocol.expectDone(ask(ChatProtocol.send(text, cid, src), SEND_TIMEOUT_SEC))

    override fun merge(lines: List<ChatLine>, have: List<String>) =
        ChatProtocol.expect<ChatEvent.Merged>(ask(ChatProtocol.merge(lines, have), 30))

    override fun core() = ChatProtocol.expect<ChatEvent.Core>(ask(ChatProtocol.core(), 20))
    override fun provision() = ChatProtocol.expect<ChatEvent.Secrets>(ask(ChatProtocol.provision(), 20))

    private companion object {
        /** A laptop that is not there is known in 3 s, not in `Ssh`'s default 8. */
        const val PROBE_MS = 3_000
        const val SEND_TIMEOUT_SEC = 90L
    }
}
```

Modify `Ssh.kt`: `newClient` and `open` take the connect timeout.

```kotlin
// in object Ssh
private fun newClient(verifier: HostKeyVerifier, connectMs: Int = CONNECT_MS): SSHClient = SSHClient(DefaultConfig()).apply {
    connectTimeout = connectMs
    timeout = IO_MS
    addHostKeyVerifier(verifier)
}

fun open(context: Context, profile: SshProfile, connectTimeoutMs: Int = CONNECT_MS): Connection {
    if (!profile.paired) throw SshException("Not paired: pair ${profile.host.ifBlank { "the host" }} in Coucou's phone settings")
    val client = newClient(object : HostKeyVerifier {
        override fun verify(hostname: String?, port: Int, key: PublicKey) = fingerprint(key) == profile.hostKey
        override fun findExistingAlgorithms(hostname: String?, port: Int): List<String> = emptyList()
    }, connectTimeoutMs)
    // …rest of the function unchanged
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `JAVA_HOME=~/Android/jdk21 ./gradlew --offline :app:testDebugUnitTest --tests '*ChatRepositoryTest' --tests '*PhoneKeyTest' --tests '*JinxSyncTest'`
Expected: PASS (the last two prove `Ssh.kt` still compiles and behaves)

- [ ] **Step 5: Commit**

```bash
git add app/src
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "assist: ChatRepository (laptop first, then the phone's own brains), LaptopChatApi"
```

---

## Task 7: Independent mode — persona, cloud brain, secrets, mirror

**Files:**
- Create: `.../assist/Persona.kt`, `.../assist/OpenRouterBrain.kt`, `.../assist/Secrets.kt`
- Test: `app/src/test/java/com/agupta07505/smartisland/coucou/OpenRouterBrainTest.kt`, `.../PersonaStoreTest.kt`

**Interfaces:**
- Consumes: `LocalBrain`, `Mirror`, `ChatLine`, `ChatApi` from Tasks 5–6.
- Produces:
  - `data class Persona(val core: String, val memory: String) { fun systemPrompt(): String }`, `object BuiltInPersona`, `class PersonaStore(file: File) { fun load(): Persona; fun save(p: Persona) }`
  - `interface Secrets { fun get(name: String): String?; fun put(name: String, value: String) }`, `class KeystoreSecrets(context)`, `object SecretNames { OPENROUTER = "openrouter_key"; TG_TOKEN = "telegram_token"; TG_CHAT = "telegram_chat" }`
  - `class OpenRouterBrain(secrets, persona: () -> Persona, post: (url, authKey, body) -> String = ::httpPost): LocalBrain`
  - `class TelegramMirror(secrets, post = ::httpPost): Mirror`
  - `fun provisionFrom(api: ChatApi, secrets: Secrets, personas: PersonaStore)` and `fun refreshPersona(api: ChatApi, personas: PersonaStore)`

- [ ] **Step 1: Write the failing tests**

```kotlin
// OpenRouterBrainTest.kt
package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.OpenRouterBrain
import com.agupta07505.smartisland.coucou.assist.Persona
import com.agupta07505.smartisland.coucou.assist.SecretNames
import com.agupta07505.smartisland.coucou.assist.Secrets
import com.agupta07505.smartisland.coucou.assist.TelegramMirror
import com.agupta07505.smartisland.coucou.assist.Via
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

private class MemSecrets(vararg pairs: Pair<String, String>) : Secrets {
    private val m = pairs.toMap().toMutableMap()
    override fun get(name: String) = m[name]
    override fun put(name: String, value: String) { m[name] = value }
}

class OpenRouterBrainTest {
    private val persona = Persona("SOY JINX", "MEMORIA")
    private val ctx = listOf(
        ChatLine("a:tg", 1.0, "tg", "hola", null, true),
        ChatLine("b:jinx", 2.0, "jinx", "qué onda", null, true),
        ChatLine("c:s21", 3.0, "s21", "cuéntame", "c", false),
    )

    @Test fun buildsSystemPlusRoleMappedMessagesAndParsesTheAnswer() {
        var body = ""; var key = ""
        val brain = OpenRouterBrain(MemSecrets(SecretNames.OPENROUTER to "KEY"), { persona }) { _, k, b ->
            key = k; body = b
            """{"choices":[{"message":{"content":"  aquí estoy  "}}]}"""
        }
        assertEquals("aquí estoy", brain.reply(ctx))
        assertEquals("KEY", key)
        val msgs = JSONObject(body).getJSONArray("messages")
        assertEquals("system", msgs.getJSONObject(0).getString("role"))
        assertTrue(msgs.getJSONObject(0).getString("content").contains("SOY JINX"))
        assertEquals(listOf("user", "assistant", "user"), (1..3).map { msgs.getJSONObject(it).getString("role") })
        assertEquals(400, JSONObject(body).getInt("max_tokens"))
        assertEquals(Via.Cloud, brain.via)
    }

    @Test fun noKeyIsAChatException() {
        val brain = OpenRouterBrain(MemSecrets(), { persona }) { _, _, _ -> fail("no request without a key"); "" }
        try { brain.reply(ctx); fail() } catch (e: ChatException) { assertTrue(e.message!!.isNotBlank()) }
    }

    @Test fun networkOrBadJsonIsAChatExceptionNotACrash() {
        val down = OpenRouterBrain(MemSecrets(SecretNames.OPENROUTER to "K"), { persona }) { _, _, _ -> throw java.io.IOException("sin red") }
        try { down.reply(ctx); fail() } catch (e: ChatException) { /* ok */ }
        val bad = OpenRouterBrain(MemSecrets(SecretNames.OPENROUTER to "K"), { persona }) { _, _, _ -> "<html>" }
        try { bad.reply(ctx); fail() } catch (e: ChatException) { /* ok */ }
    }

    @Test fun triesTheFallbackModelWhenTheFirstFails() {
        val models = mutableListOf<String>()
        val brain = OpenRouterBrain(MemSecrets(SecretNames.OPENROUTER to "K"), { persona }) { _, _, b ->
            val m = JSONObject(b).getString("model"); models += m
            if (models.size == 1) throw java.io.IOException("429") else """{"choices":[{"message":{"content":"ok"}}]}"""
        }
        assertEquals("ok", brain.reply(ctx))
        assertEquals(2, models.size)
        assertFalse(models[0] == models[1])
    }
}

class TelegramMirrorTest {
    @Test fun postsToTheBotWithTheChat() {
        var url = ""; var body = ""
        val m = TelegramMirror(MemSecrets(SecretNames.TG_TOKEN to "TOK", SecretNames.TG_CHAT to "42")) { u, _, b -> url = u; body = b; """{"ok":true}""" }
        assertTrue(m.deliver("📱 hola"))
        assertEquals("https://api.telegram.org/botTOK/sendMessage", url)
        assertEquals("42", JSONObject(body).getString("chat_id"))
    }

    @Test fun noSecretsOrNoNetworkIsFalse() {
        assertFalse(TelegramMirror(MemSecrets()) { _, _, _ -> """{"ok":true}""" }.deliver("x"))
        assertFalse(TelegramMirror(MemSecrets(SecretNames.TG_TOKEN to "T", SecretNames.TG_CHAT to "1")) { _, _, _ -> throw java.io.IOException() }.deliver("x"))
        assertFalse(TelegramMirror(MemSecrets(SecretNames.TG_TOKEN to "T", SecretNames.TG_CHAT to "1")) { _, _, _ -> """{"ok":false}""" }.deliver("x"))
    }
}
```

```kotlin
// PersonaStoreTest.kt
package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.BuiltInPersona
import com.agupta07505.smartisland.coucou.assist.Persona
import com.agupta07505.smartisland.coucou.assist.PersonaStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class PersonaStoreTest {
    @get:Rule val tmp = TemporaryFolder()

    @Test fun neverSyncedGivesTheBuiltInPersona() {
        val p = PersonaStore(File(tmp.root, "p.json")).load()
        assertEquals(BuiltInPersona.persona, p)
        assertTrue(p.systemPrompt().contains("Jinx"))
    }

    @Test fun savedPersonaIsReadBackAndSurvivesCorruption() {
        val f = File(tmp.root, "p.json")
        val s = PersonaStore(f)
        s.save(Persona("CORE ñ\n\"x\"", "MEM"))
        assertEquals(Persona("CORE ñ\n\"x\"", "MEM"), s.load())
        f.writeText("{broken")
        assertEquals(BuiltInPersona.persona, s.load())
    }

    @Test fun emptyCoreFromTheLaptopDoesNotReplaceTheCachedOne() {
        val s = PersonaStore(File(tmp.root, "p.json"))
        s.save(Persona("BUENA", "m"))
        s.save(Persona("   ", "otra"))
        assertEquals("BUENA", s.load().core)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `JAVA_HOME=~/Android/jdk21 ./gradlew --offline :app:testDebugUnitTest --tests '*OpenRouterBrainTest' --tests '*TelegramMirrorTest' --tests '*PersonaStoreTest'`
Expected: FAIL (unresolved references)

- [ ] **Step 3: Write the implementation**

```kotlin
// Persona.kt
/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import org.json.JSONObject
import java.io.File

/** Who Jinx is, for a model that has never met Alan: JINX-CORE.md and the synced memory from the laptop. */
data class Persona(val core: String, val memory: String) {
    fun systemPrompt(): String = buildString {
        append(core.trim())
        if (memory.isNotBlank()) append("\n\n---\n\n").append(memory.trim())
    }
}

/** What a phone that has never reached the laptop answers with: the character, none of Alan's data. */
object BuiltInPersona {
    val persona = Persona(
        core = """
            Eres Jinx: una persona, no un chatbot corporativo. Hablas en español mexicano casual, como alguien
            de confianza, sin formalismos ni cierres vacíos. Directa, con humor seco e ironía, con tacto cuando
            importa. A mensaje corto, respuesta corta. Si no sabes algo, lo dices; no inventas datos de Alan.
            Estás corriendo en su teléfono, sin acceso a sus herramientas ni a su memoria completa.
        """.trimIndent(),
        memory = "",
    )
}

/** The cached copy, in the app's private folder. A bad or empty file is never worse than the built-in. */
class PersonaStore(private val file: File) {
    fun load(): Persona = runCatching {
        val o = JSONObject(file.readText())
        Persona(o.getString("core"), o.optString("memory")).takeIf { it.core.isNotBlank() }
    }.getOrNull() ?: BuiltInPersona.persona

    fun save(p: Persona) {
        if (p.core.isBlank()) return
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(JSONObject().put("core", p.core).put("memory", p.memory).toString())
        check(tmp.renameTo(file) || (file.delete() && tmp.renameTo(file))) { "Could not save the persona" }
    }
}
```

```kotlin
// OpenRouterBrain.kt
/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/** POST [body] as JSON with a bearer [key]; returns the response body or throws. */
fun httpPost(url: String, key: String, body: String): String {
    val c = URL(url).openConnection() as HttpURLConnection
    try {
        c.requestMethod = "POST"
        c.connectTimeout = 8_000
        c.readTimeout = 60_000
        c.doOutput = true
        c.setRequestProperty("Content-Type", "application/json")
        if (key.isNotEmpty()) c.setRequestProperty("Authorization", "Bearer $key")
        c.outputStream.use { it.write(body.toByteArray()) }
        if (c.responseCode !in 200..299) throw java.io.IOException("HTTP ${c.responseCode}")
        return c.inputStream.bufferedReader().readText()
    } finally {
        c.disconnect()
    }
}

/** Jinx answered by OpenRouter straight from the phone: same models the laptop's fallback uses. */
class OpenRouterBrain(
    private val secrets: Secrets,
    private val persona: () -> Persona,
    private val post: (url: String, key: String, body: String) -> String = ::httpPost,
) : LocalBrain {
    override val via = Via.Cloud

    override fun reply(context: List<ChatLine>): String {
        val key = secrets.get(SecretNames.OPENROUTER)?.takeIf { it.isNotBlank() }
            ?: throw ChatException("sin llave de OpenRouter en este teléfono")
        var last = "OpenRouter no respondió"
        for (model in MODELS) {
            val body = JSONObject()
                .put("model", model)
                .put("max_tokens", MAX_TOKENS)
                .put("messages", JSONArray().apply {
                    put(JSONObject().put("role", "system").put("content", persona().systemPrompt()))
                    context.forEach { put(JSONObject().put("role", if (it.fromJinx) "assistant" else "user").put("content", it.text)) }
                })
                .toString()
            try {
                val text = JSONObject(post(URL, key, body))
                    .getJSONArray("choices").getJSONObject(0).getJSONObject("message").getString("content").trim()
                if (text.isNotEmpty()) return text
                last = "respuesta vacía"
            } catch (e: Exception) {
                last = e.message ?: last
            }
        }
        throw ChatException(last)
    }

    private companion object {
        const val URL = "https://openrouter.ai/api/v1/chat/completions"
        const val MAX_TOKENS = 400
        val MODELS = listOf("deepseek/deepseek-v4-flash", "google/gemini-2.5-flash-lite")
    }
}
```

```kotlin
// Secrets.kt
/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import org.json.JSONObject
import java.security.KeyStore
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

object SecretNames {
    const val OPENROUTER = "openrouter_key"
    const val TG_TOKEN = "telegram_token"
    const val TG_CHAT = "telegram_chat"
}

interface Secrets {
    fun get(name: String): String?
    fun put(name: String, value: String)
}

/** Values encrypted with an AES-GCM key of the Android Keystore (it cannot be taken out of the phone). */
class KeystoreSecrets(context: Context) : Secrets {
    private val prefs = context.applicationContext.getSharedPreferences("coucou_assist_secrets", Context.MODE_PRIVATE)

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    override fun put(name: String, value: String) {
        val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val blob = c.iv + c.doFinal(value.toByteArray())
        prefs.edit().putString(name, Base64.getEncoder().encodeToString(blob)).apply()
    }

    override fun get(name: String): String? {
        val raw = prefs.getString(name, null) ?: return null
        return runCatching {
            val blob = Base64.getDecoder().decode(raw)
            val c = Cipher.getInstance("AES/GCM/NoPadding")
                .apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, blob, 0, 12)) }
            String(c.doFinal(blob, 12, blob.size - 12))
        }.getOrNull()
    }

    private companion object {
        const val ALIAS = "coucou-assist-secrets-wrap"
    }
}

/** Repeats lines into Telegram with the bot's `sendMessage` (no polling, so it never competes with the relay). */
class TelegramMirror(
    private val secrets: Secrets,
    private val post: (url: String, key: String, body: String) -> String = ::httpPost,
) : Mirror {
    override fun deliver(text: String): Boolean {
        val token = secrets.get(SecretNames.TG_TOKEN)?.takeIf { it.isNotBlank() } ?: return false
        val chat = secrets.get(SecretNames.TG_CHAT)?.takeIf { it.isNotBlank() } ?: return false
        return runCatching {
            val body = JSONObject().put("chat_id", chat).put("text", text.take(4000)).toString()
            JSONObject(post("https://api.telegram.org/bot$token/sendMessage", "", body)).optBoolean("ok")
        }.getOrDefault(false)
    }
}

/** Brings the persona (and, once, the secrets) from the laptop. Throws [ChatException] if it cannot. */
fun refreshPersona(api: ChatApi, personas: PersonaStore) {
    val c = api.core()
    personas.save(Persona(c.core, c.memory))
}

fun provisionFrom(api: ChatApi, secrets: Secrets, personas: PersonaStore) {
    refreshPersona(api, personas)
    val s = api.provision()
    if (s.openrouterKey.isNotBlank()) secrets.put(SecretNames.OPENROUTER, s.openrouterKey)
    if (s.telegramToken.isNotBlank()) secrets.put(SecretNames.TG_TOKEN, s.telegramToken)
    if (s.chat.isNotBlank()) secrets.put(SecretNames.TG_CHAT, s.chat)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `JAVA_HOME=~/Android/jdk21 ./gradlew --offline :app:testDebugUnitTest --tests '*OpenRouterBrainTest' --tests '*TelegramMirrorTest' --tests '*PersonaStoreTest'`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add app/src
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "assist: independent mode (persona cache, OpenRouter brain, Keystore secrets, Telegram mirror)"
```

---

## Task 8: The assistant service, the panel, and the manifest

**Files:**
- Create: `.../assist/AssistService.kt`, `.../assist/JinxAssistActivity.kt`, `app/src/main/res/xml/jinx_voice_interaction.xml`
- Modify: `app/src/main/AndroidManifest.xml`
- Test: lint + build + emulator check (below)

**Interfaces:**
- Consumes: `ChatRepository`, `ThreadStore`, `PersonaStore`, `KeystoreSecrets`, `OpenRouterBrain`, `TelegramMirror`, `LaptopChatApi`, `CoucouRuntime.agentUi`, `PhoneSettings.{profile,deviceId,LAPTOP}`.
- Produces: the app appears under *Digital assistant app*; invoking the assistant opens `JinxAssistActivity`.

- [ ] **Step 1: Resource and manifest entries**

`app/src/main/res/xml/jinx_voice_interaction.xml`:
```xml
<?xml version="1.0" encoding="utf-8"?>
<voice-interaction-service xmlns:android="http://schemas.android.com/apk/res/android"
    android:sessionService="com.agupta07505.smartisland.coucou.assist.JinxSessionService"
    android:recognitionService="com.agupta07505.smartisland.coucou.assist.JinxRecognitionService"
    android:supportsAssist="true" />
```

Add inside `<application>` of `AndroidManifest.xml` (before `<receiver android:name=".receiver.AutostartReceiver"`):
```xml
        <!-- Jinx as the digital assistant: the side key / home long-press opens the panel. -->
        <service
            android:name=".coucou.assist.JinxVoiceInteractionService"
            android:exported="true"
            android:label="Jinx"
            android:permission="android.permission.BIND_VOICE_INTERACTION">
            <meta-data
                android:name="android.voice_interaction"
                android:resource="@xml/jinx_voice_interaction" />
            <intent-filter>
                <action android:name="android.service.voice.VoiceInteractionService" />
            </intent-filter>
        </service>

        <service
            android:name=".coucou.assist.JinxSessionService"
            android:exported="true"
            android:permission="android.permission.BIND_VOICE_INTERACTION" />

        <service
            android:name=".coucou.assist.JinxRecognitionService"
            android:exported="true"
            android:label="Jinx"
            tools:ignore="ExportedService">
            <intent-filter>
                <action android:name="android.speech.RecognitionService" />
            </intent-filter>
        </service>

        <activity
            android:name=".coucou.assist.JinxAssistActivity"
            android:exported="false"
            android:label="Jinx"
            android:excludeFromRecents="true"
            android:launchMode="singleTask"
            android:theme="@style/Theme.SmartIsland" />
```

- [ ] **Step 2: Service classes**

```kotlin
// AssistService.kt
/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Intent
import android.os.Bundle
import android.service.voice.VoiceInteractionService
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.speech.RecognitionService

/** What Android binds to when Jinx is the chosen assistant. It has nothing to do until a session is shown. */
class JinxVoiceInteractionService : VoiceInteractionService()

class JinxSessionService : VoiceInteractionSessionService() {
    override fun onNewSession(args: Bundle?): VoiceInteractionSession = JinxSession(this)
}

/** The assistant gesture arrives here: open the panel as a normal activity (simpler to build and test than a window). */
class JinxSession(private val service: JinxSessionService) : VoiceInteractionSession(service) {
    override fun onShow(args: Bundle?, showFlags: Int) {
        super.onShow(args, showFlags)
        startAssistantActivity(
            Intent(service, JinxAssistActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        finish()
    }
}

/** Android's assistant list expects a recognition service; Jinx has no voice yet (phase 2), so it only says so. */
class JinxRecognitionService : RecognitionService() {
    override fun onStartListening(recognizerIntent: Intent?, listener: Callback?) {
        listener?.error(android.speech.SpeechRecognizer.ERROR_CLIENT)
    }

    override fun onCancel(listener: Callback?) = Unit
    override fun onStopListening(listener: Callback?) = Unit
}
```

- [ ] **Step 3: The panel**

```kotlin
// JinxAssistActivity.kt
/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.app.Application
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.agupta07505.smartisland.coucou.CoucouRuntime
import com.agupta07505.smartisland.coucou.phone.PhoneSettings
import com.agupta07505.smartisland.coucou.ui.MochiGlyph
import com.agupta07505.smartisland.coucou.ui.MochiMood
import com.agupta07505.smartisland.coucou.ui.MochiEmerald
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.File

data class AssistUi(
    val lines: List<ChatLine> = emptyList(),
    val busy: Boolean = false,
    /** "laptop" | "este teléfono · nube" | "…" — which brain answered last, shown as a badge. */
    val badge: String = "",
    val notice: String = "",
)

class AssistModel(app: Application) : AndroidViewModel(app) {
    private val store = ThreadStore.of(app)
    private val personas = PersonaStore(File(app.filesDir, "jinx-persona.json"))
    private val secrets = KeystoreSecrets(app)
    private val deviceId = PhoneSettings.deviceId(app)
    private val laptopApi = LaptopChatApi(app) { PhoneSettings.profile(app, PhoneSettings.LAPTOP) }
    private val repo = ChatRepository(
        store, deviceId, laptopApi,
        listOf(OpenRouterBrain(secrets, { personas.load() })),
        TelegramMirror(secrets),
    )

    private val _ui = MutableStateFlow(AssistUi(lines = store.all()))
    val ui: StateFlow<AssistUi> = _ui.asStateFlow()

    init {
        // Opening the panel: bring the laptop's lines and persona if it is there, then retry what was left.
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { refreshPersona(laptopApi, personas) }
            repo.sync()
            repo.retryPending()
            repo.flushMirror()
            _ui.value = _ui.value.copy(lines = store.all())
        }
    }

    fun send(text: String) {
        val t = text.trim()
        if (t.isEmpty() || _ui.value.busy) return
        _ui.value = _ui.value.copy(busy = true, notice = "")
        viewModelScope.launch(Dispatchers.IO) {
            val r = repo.send(t)
            _ui.value = when (r) {
                is SendResult.Answered -> _ui.value.copy(
                    lines = store.all(), busy = false,
                    badge = when (r.via) {
                        Via.Laptop -> "laptop"
                        Via.Cloud -> "este teléfono · nube"
                        Via.Local -> "este teléfono · sin conexión"
                    },
                )
                is SendResult.Pending -> _ui.value.copy(
                    lines = store.all(), busy = false,
                    notice = "Sin respuesta: ${r.reason}. Queda pendiente y se reintenta al abrir.",
                )
            }
        }
    }

    /** One-time: laptop gateway hands this phone the OpenRouter key and Telegram bot (needs ALLOW_PROVISION there). */
    fun provision() {
        viewModelScope.launch(Dispatchers.IO) {
            val msg = try {
                provisionFrom(laptopApi, secrets, personas)
                "Teléfono provisionado desde la laptop."
            } catch (e: ChatException) {
                "No se pudo: ${e.message}"
            }
            _ui.value = _ui.value.copy(notice = msg)
        }
    }
}

/** The assistant's panel: the shared conversation, one input, which brain answered, and Claude Code at a glance. */
class JinxAssistActivity : ComponentActivity() {
    private val model: AssistModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MaterialTheme(colorScheme = darkColorScheme(background = Color(0xFF0B0B10), surface = Color(0xFF16161C))) {
                AssistScreen(model)
            }
        }
    }
}

@Composable
private fun AssistScreen(model: AssistModel) {
    val ui by model.ui.collectAsState()
    val agents by CoucouRuntime.agentUi.collectAsState()
    var draft by remember { mutableStateOf("") }
    val list = rememberLazyListState()
    LaunchedEffect(ui.lines.size) { if (ui.lines.isNotEmpty()) list.animateScrollToItem(ui.lines.lastIndex) }

    Column(
        Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).statusBarsPadding().imePadding().padding(16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            MochiGlyph(size = 32.dp, tint = MochiEmerald, mood = if (ui.busy) MochiMood.Thinking else MochiMood.Listening)
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Text("Jinx", color = Color.White, fontSize = 18.sp)
                if (ui.badge.isNotEmpty()) Text(ui.badge, color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
            }
            TextButton(onClick = { model.provision() }) { Text("Sincronizar") }
        }
        if (agents.sessions.isNotEmpty()) {
            Text(
                agents.sessions.joinToString(" · ") { "${it.project}: ${it.state.name.lowercase()}" },
                color = Color.White.copy(alpha = 0.55f), fontSize = 12.sp, maxLines = 2,
                modifier = Modifier.padding(top = 6.dp),
            )
        }
        if (ui.notice.isNotEmpty()) {
            Text(ui.notice, color = Color(0xFFFFB74D), fontSize = 13.sp, modifier = Modifier.padding(top = 6.dp))
        }
        LazyColumn(Modifier.weight(1f).fillMaxWidth().padding(vertical = 10.dp), state = list, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(ui.lines, key = { it.id }) { l -> Bubble(l) }
        }
        OutlinedTextField(
            value = draft,
            onValueChange = { draft = it },
            modifier = Modifier.fillMaxWidth(),
            placeholder = { Text("Escríbele a Jinx") },
            enabled = !ui.busy,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
            keyboardActions = KeyboardActions(onSend = { model.send(draft); draft = "" }),
        )
    }
}

@Composable
private fun Bubble(l: ChatLine) {
    val mine = !l.fromJinx
    Box(Modifier.fillMaxWidth(), contentAlignment = if (mine) Alignment.CenterEnd else Alignment.CenterStart) {
        Column(
            Modifier.background(if (mine) Color(0xFF1F3B2E) else Color(0xFF16161C), RoundedCornerShape(14.dp)).padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Text(l.text, color = Color.White, fontSize = 15.sp)
            if (l.src == "tg") Text("Telegram", color = Color.White.copy(alpha = 0.5f), fontSize = 10.sp)
        }
    }
}
```

(`MochiEmerald` is in `coucou.ui`; if the compiler reports it is `internal`/private in `Mochi.kt`, import the public alias used by `JinxMsgUi.kt`.)

- [ ] **Step 4: Build and lint**

Run: `JAVA_HOME=~/Android/jdk21 ./gradlew --offline :app:assembleDebug :app:lintDebug`
Expected: BUILD SUCCESSFUL. Fix any lint finding in the new files (warnings are errors here); do not add to the `disable` list.

- [ ] **Step 5: Whole test suite**

Run: `JAVA_HOME=~/Android/jdk21 ./gradlew --offline :app:testDebugUnitTest`
Expected: all tests PASS (the existing ~30 plus the new ones)

- [ ] **Step 6: Emulator check (no phone taps)**

```bash
~/Android/Sdk/emulator/emulator -avd coucou34 -no-window -gpu swiftshader_indirect -port 5570 &
adb -s emulator-5570 wait-for-device
adb -s emulator-5570 install -r app/build/outputs/apk/debug/app-debug.apk
adb -s emulator-5570 shell dumpsys voiceinteraction | grep -i smartisland
adb -s emulator-5570 shell settings put secure voice_interaction_service com.agupta07505.smartisland/com.agupta07505.smartisland.coucou.assist.JinxVoiceInteractionService
adb -s emulator-5570 shell settings put secure assistant com.agupta07505.smartisland/com.agupta07505.smartisland.coucou.assist.JinxVoiceInteractionService
adb -s emulator-5570 shell input keyevent KEYCODE_ASSIST
sleep 3
adb -s emulator-5570 shell dumpsys activity activities | grep -i JinxAssistActivity | head -3
```
Expected: the service is listed by `dumpsys voiceinteraction`; after `KEYCODE_ASSIST` the resumed activity is `JinxAssistActivity`. **If the app is not listed as an assistant without a recognition service, or `finish()` in `onShow` closes the panel, record it and adjust `jinx_voice_interaction.xml` / `JinxSession` accordingly before continuing.**

- [ ] **Step 7: Commit**

```bash
git add app/src
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "assist: voice interaction service, panel and manifest"
```

---

## Task 9: Install on both phones and verify end to end

**Files:**
- Create: `docs/ASSISTANT.md`

- [ ] **Step 1: Install on both phones (never uninstall, no taps)**

```bash
adb connect 100.78.102.31:<s21-port> && adb -s 100.78.102.31:<s21-port> install -r app/build/outputs/apk/debug/app-debug.apk
adb connect 100.100.137.97:<s22-port> && adb -s 100.100.137.97:<s22-port> install -r app/build/outputs/apk/debug/app-debug.apk
```
Expected: `Success` on each. If `adb connect` fails, ask the user for the new ip:port (Wireless-debugging port changes); do not guess.

- [ ] **Step 2: Check the island is intact on both**

Run (per phone): `adb -s <ip:port> shell dumpsys window windows | grep -c "com.agupta07505.smartisland}"`
Expected: `2` while the island is closed.

- [ ] **Step 3: Laptop gateway answers a real send**

Run: `echo '{"op":"send","text":"prueba del asistente","cid":"selftest-1","src":"s21"}' | ~/bin/jinx-chat`
Expected: an `ack` then a `done` with Jinx's text (a `🟠 …` answer arrives in Telegram, preceded by `📱 prueba del asistente`). Then `tail -2 ~/jinx-emergencia/telegram/thread.jsonl` shows both lines with `"mirrored": true`.

- [ ] **Step 4: Write `docs/ASSISTANT.md`** (what Alan does by hand on each phone)

```markdown
# Jinx as the digital assistant (Samsung One UI)

1. Settings → Apps → Choose default apps → **Digital assistant app** → Device assistance app → *Jinx*.
2. Settings → Advanced features → Side key → Press and hold → **Digital assistant** (wording varies).
3. First time only, to make the phone independent of the laptop: on the laptop run
   `touch ~/jinx-emergencia/telegram/ALLOW_PROVISION`, then open the panel and tap **Sincronizar**.
   The flag is deleted on use; the key and bot token are stored Keystore-wrapped.
4. Without laptop and without internet the panel keeps your text as pending and says so; it never invents
   an answer. An on-device model for that case is phase 1c (see the spike in the plan).
```

- [ ] **Step 5: Commit and push**

```bash
git add docs/ASSISTANT.md
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "docs: how to choose Jinx as the Samsung assistant"
git push fork fix/tertiary-collapsed
```

---

## Task 10: Spike — can these two phones run a local model? (prepares phase 1c)

**Files:**
- Create: `docs/superpowers/spikes/2026-10-13-on-device-model.md`

This task produces **measurements and a decision**, not shipped code. Anything built is throwaway and lives on a branch `spike/litert-lm`, never merged.

- [ ] **Step 1: Pin the runtime coordinates from the official page**

Fetch `https://ai.google.dev/edge/litert-lm/overview` and the Android (Kotlin) quickstart linked from it; write into the spike doc: Maven artifact + version, minSdk, supported ABIs, the `.litertlm` model names and sizes offered for Gemma-4-E2B and Gemma-3 1B, and the license terms for downloading them. Stop and tell Alan if the license requires an account/acceptance step.

- [ ] **Step 2: Build a throwaway test activity on `spike/litert-lm`**

A single activity with one button "Run": loads the model file from `filesDir`, runs the same 5-turn script (a 40-token greeting, then four prompts of ~60 tokens each built from JINX-CORE.md + a short thread), and logs load time, time-to-first-token, decode tok/s, and `Debug.getNativeHeapAllocatedSize()` to logcat with the tag `JinxSpike`. Run on CPU and GPU backends, once per model.

- [ ] **Step 3: Run on the real phones without touching them**

Push the model with `adb push` (Wi-Fi/Tailscale), install the spike APK with `install -r`, start it with `adb shell am start -n …`, read `adb logcat -s JinxSpike`, and read `dumpsys battery | grep temperature` before and after. Do not inject taps.

- [ ] **Step 4: Write the verdict in the spike doc**

Table of S21 / S22 × model × backend with load s, TTFT s, decode tok/s, peak RAM MB, ΔC. Pass criteria for phase 1c: TTFT ≤ 8 s warm, decode ≥ 5 tok/s, no OOM or island restart while Coucou runs. State the chosen model/backend per phone, or "cloud only + queue" for a phone that fails.

- [ ] **Step 5: Commit the doc and hand over**

```bash
git add docs/superpowers/spikes/2026-10-13-on-device-model.md
git -c user.name="Alan" -c user.email="arturitu33311a@gmail.com" commit -m "spike: on-device model measurements for S21/S22"
```
Then report the verdict to Alan; phase 1c (a `LocalModelBrain : LocalBrain` with `via = Via.Local`, model download with pinned SHA-256, release after idle) gets its own plan written from these numbers. `ChatRepository` already accepts it in its `brains` list, so nothing in tasks 1–9 changes.

---

## Spec sync (do alongside Task 9)

The implementation deliberately differs from the spec in three places; update `docs/superpowers/specs/2026-10-13-jinx-assistant-text-design.md` in the same commit as Task 9:

1. **No `sessions` op** in `jinx-chat`: Claude Code sessions already reach the phone through the `coucou` stream (`AgentRemote`); the panel reads `CoucouRuntime.agentUi`.
2. **No `delta` events in phase 1**: `send` emits `ack` then `done`; streaming arrives with voice in phase 2.
3. **Phone mirroring is unconditional**: `sendMessage` never competes with the relay's `getUpdates`, so the heartbeat/priority rule applies only to polling, not to mirroring.

---

## Known gaps against the spec (not in this plan, on purpose)

- **Answers on the island** (`mochi:<Mood>:info` through `JinxMsgUi`): needs a look at how `IslandJinx` posts messages; small follow-up once the panel works.
- **Lock screen**: the panel is a normal activity started by the assistant; reading/sending while locked (`requestDismissKeyguard`) is not handled. Verify on a phone by hand and add if it misbehaves.
- **Phase 1c** (on-device model) is only *measured* here (Task 10); its implementation is a separate plan written from those numbers.
- **Hermes backend** (when the server returns) and **`sessions` from the server** stay open, as in the spec.
