#!/usr/bin/env python3
"""jinx_act_worker — applies the buttons pressed on Jinx's notices (Hecho / Pospón / Descartar).

Runs ON THE SERVER, once a minute from cron (`* * * * * ~/.hermes/hermes-agent/venv/bin/python
~/.hermes/scripts/jinx_act_worker.py`). A device presses a button -> the presence server queues it
(`POST /api/mochi/act`) -> this worker takes it from `GET /api/mochi/acts`, applies it and clears it
(`POST /api/mochi/acts/done`). Nothing here talks to a phone or a PC.

What a press does:
  ref "pend:<id>"   done -> the pendiente is marked hecho      dismiss -> descartado
  any notice        snooze (1 h) / tomorrow (09:00) -> the same notice is re-sent later, with its buttons
  anything else     acknowledged, nothing to apply (the press already hid the buttons on every device)

A press that fails (store unreachable) stays queued and is retried; after MAX_TRIES it is dropped and logged,
so one bad press cannot block the rest.
"""
from __future__ import annotations

import json
import os
import sys
import time
import urllib.request
from pathlib import Path

HERMES = Path(os.environ.get("HERMES_HOME", Path.home() / ".hermes"))
PRESENCE = os.environ.get("MOCHI_PRESENCE_LOCAL", "http://127.0.0.1:8897")
STATE = HERMES / "state" / "act_worker.json"
LOG = HERMES / "logs" / "act_worker.log"
MAX_TRIES = 3
SNOOZE_SECONDS = 3600
PEND_ACTIONS = {"done": "hecho", "dismiss": "descartado"}


def when_for(action: str, now: float, hour: int = 9) -> float | None:
    """Epoch the notice should come back for a snooze action, else None. 'tomorrow' = next day at `hour`:00 local."""
    if action == "snooze":
        return now + SNOOZE_SECONDS
    if action == "tomorrow":
        t = time.localtime(now)
        tomorrow = time.mktime((t.tm_year, t.tm_mon, t.tm_mday + 1, hour, 0, 0, 0, 0, -1))
        return tomorrow
    return None


def process(act: dict, now: float, resolve, schedule) -> str:
    """Applies one queued press. `resolve(pend_id, accion) -> dict` and `schedule(epoch, msg)` are the effects
    (injected so this stays testable). Returns a short outcome for the log; raises when it should be retried."""
    action, ref = act.get("action"), act.get("ref") or ""
    when = when_for(action, now)
    if when is not None:
        schedule(when, act.get("msg") or {})
        return f"snoozed until {time.strftime('%d %H:%M', time.localtime(when))}"
    if ref.startswith("pend:") and action in PEND_ACTIONS:
        res = resolve(ref[len("pend:"):], PEND_ACTIONS[action])
        if not res.get("ok"):
            raise RuntimeError(res.get("error", "resolve failed"))
        return f"{ref} -> {PEND_ACTIONS[action]}"
    return "acknowledged"


def due_resends(queue: list[dict], now: float) -> tuple[list[dict], list[dict]]:
    """(due, still waiting) split of the snoozed notices."""
    return [q for q in queue if q["at"] <= now], [q for q in queue if q["at"] > now]


# ---------- effects ----------

def log(msg: str) -> None:
    LOG.parent.mkdir(parents=True, exist_ok=True)
    with LOG.open("a") as f:
        f.write(f"{time.strftime('%Y-%m-%dT%H:%M:%S')} {msg}\n")


def load_state() -> dict:
    try:
        return json.loads(STATE.read_text())
    except (OSError, ValueError):
        return {}


def save_state(st: dict) -> None:
    STATE.parent.mkdir(parents=True, exist_ok=True)
    STATE.write_text(json.dumps(st))


def call(path: str, body: dict | None = None) -> dict:
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(PRESENCE + path, data=data, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=8) as r:
        return json.loads(r.read())


def resolve_pendiente(pend_id: str, accion: str) -> dict:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    sys.path.insert(0, str(HERMES / "plugins" / "pendientes"))
    import coucou_pendientes  # type: ignore
    import pendientes_store as store  # type: ignore
    return coucou_pendientes.handle({"op": "resolve", "id": pend_id, "accion": accion}, store)


def notify_telegram(text: str) -> None:
    """Text-only fallback through the funnel's own sender, skipping the mochi (it already holds the notice)."""
    import subprocess
    subprocess.run([str(HERMES / "scripts" / "notify_alan.sh"), text], env=dict(os.environ, MOCHI_ROUTE="0"),
                   capture_output=True, timeout=90)


def resend(msg: dict, deliver=None, telegram=None) -> None:
    """Posts the snoozed notice again with its buttons (its text says it is a reminder).
    When nobody is looking at any device the presence server still KEEPS the notice (it shows on the next device in
    use) but reports it as unwatched: that is delivered, not failed (retrying would duplicate it every 5 min), and
    the reminder is also sent to Telegram so it is not lost."""
    if deliver is None:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import mochi_route  # type: ignore
        deliver = mochi_route.deliver
    telegram = telegram or notify_telegram
    text = "⏰ Te lo recuerdo: " + str(msg.get("text", "")).strip()
    ok, info = deliver(text, msg.get("mood"), msg.get("kind", "info"), msg.get("source", ""),
                       actions=msg.get("actions"), ref=msg.get("ref"))
    if ok:
        return
    if str(info).startswith("nadie mirando"):
        telegram(text)
        return
    raise RuntimeError(info)


def main() -> int:
    now = time.time()
    st = load_state()
    st.setdefault("snoozed", [])
    st.setdefault("tries", {})
    try:
        acts = call("/api/mochi/acts").get("acts", [])
    except Exception as e:  # noqa: BLE001  (presence server down: try again next minute)
        log(f"presence unreachable: {e}")
        acts = []
    done_ids = []

    def schedule(epoch: float, msg: dict) -> None:
        st["snoozed"].append({"at": epoch, "msg": msg})

    for act in acts:
        key = str(act["id"])
        try:
            out = process(act, now, resolve_pendiente, schedule)
            log(f"act #{act['id']} {act.get('action')} by {act.get('by')}: {out}")
            done_ids.append(act["id"])
            st["tries"].pop(key, None)
        except Exception as e:  # noqa: BLE001
            st["tries"][key] = st["tries"].get(key, 0) + 1
            log(f"act #{act['id']} failed ({st['tries'][key]}/{MAX_TRIES}): {e}")
            if st["tries"][key] >= MAX_TRIES:
                done_ids.append(act["id"])
                st["tries"].pop(key, None)
    if done_ids:
        try:
            call("/api/mochi/acts/done", {"ids": done_ids})
        except Exception as e:  # noqa: BLE001
            log(f"could not clear {done_ids}: {e}")

    due, waiting = due_resends(st["snoozed"], now)
    st["snoozed"] = waiting
    for item in due:
        try:
            resend(item["msg"])
            log(f"re-sent snoozed notice: {str(item['msg'].get('text', ''))[:50]!r}")
        except Exception as e:  # noqa: BLE001
            item["at"] = now + 300          # try again in 5 minutes
            st["snoozed"].append(item)
            log(f"re-send failed, retrying in 5 min: {e}")
    save_state(st)
    return 0


if __name__ == "__main__":
    sys.exit(main())
