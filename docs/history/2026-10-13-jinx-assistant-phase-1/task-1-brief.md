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

