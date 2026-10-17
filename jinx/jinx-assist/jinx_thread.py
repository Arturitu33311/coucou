"""Hilo compartido de la conversación de Jinx (Telegram + teléfonos + gateway).

Una línea JSON por mensaje en <base>/thread.jsonl:
  {"id","ts","src","text","cid","mirrored"}  (+ "mood" opcional en las líneas de Jinx)
src: tg | s21 | s22 | pc | jinx.  id = "<cid>:<src>" si hay cid, si no un uuid.
"mood" es uno de EMOTES (los emotes de Mochi) o no está: nunca se guarda null ni un nombre desconocido.
Todas las operaciones toman un flock exclusivo y reescriben de forma atómica.
"""
import contextlib
import fcntl
import json
import os
import time
import uuid

HIST_MAX = 20

# Los nombres válidos de ánimo: los emotes de Mochi (PC: BotEmoteName; teléfono: MochiMood.emote).
EMOTES = ("happy", "love", "proud", "surprised", "excited", "confused", "shy", "sad", "curious", "bored",
          "scared", "grumpy", "smug", "laughing", "crying", "relieved", "determined", "flustered", "sleepy")


# Cerebros de respaldo que se anotan en la línea de Jinx ("hermes" no se anota: es lo normal).
VIAS = ("or", "local")

def valid_mood(m):
    """El emote tal cual si es uno de EMOTES; si no, None."""
    return m if isinstance(m, str) and m in EMOTES else None


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

    def append(self, src, text, cid=None, ts=None, mirrored=False, mood=None, via=None):
        with self._locked():
            lines = self._read()
            lid = _line_id(src, cid)
            for d in lines:
                if d["id"] == lid:
                    return d
            d = {"id": lid, "ts": time.time() if ts is None else ts, "src": src,
                 "text": text, "cid": cid, "mirrored": mirrored}
            if valid_mood(mood):
                d["mood"] = mood
            if via in VIAS:
                d["via"] = via  # qué cerebro contestó, cuando no fue Hermes (para marcarlo en Telegram)
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
            by_id = {d["id"]: d for d in lines}
            added = []
            flags_changed = False
            for d in incoming:
                if not _valid(d):
                    continue
                if d["id"] in known:
                    # "espejado" sólo sube: nunca se desmarca, y se adopta si otro dispositivo ya lo espejó.
                    cur = by_id[d["id"]]
                    if d.get("mirrored") is True and not cur["mirrored"]:
                        cur["mirrored"] = True
                        flags_changed = True
                    continue
                mood = valid_mood(d.get("mood"))
                incoming_via = d.get("via")
                d = {"id": d["id"], "ts": d["ts"], "src": d["src"], "text": d["text"],
                     "cid": d.get("cid"), "mirrored": bool(d.get("mirrored", False))}
                if mood:
                    d["mood"] = mood
                if incoming_via in VIAS:
                    d["via"] = incoming_via
                known.add(d["id"])
                by_id[d["id"]] = d
                lines.append(d)
                added.append(d)
            if added or flags_changed:
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
