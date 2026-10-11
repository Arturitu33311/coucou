"""What the server is busy with, for the island's pill and the Today tab.

Coucou runs this over ssh (the script arrives on stdin, nothing is installed on the server and
no argument or user text is part of the command). It prints ONE line of JSON:

    {"at": 1760000000, "activities": [
        {"id": "deluge:ab12", "app": "Deluge", "kind": "download", "title": "Some.Show.S01E01",
         "progress": 0.42, "eta": 300, "state": "active"},
        {"id": "backup:Backup", "app": "Backup", "kind": "backup", "title": "Backup",
         "progress": null, "eta": null, "state": "done", "detail": "3 h ago"}]}

state is "active", "queued", "done" or "error". Nothing is guessed: what it looks at is listed in
~/.config/coucou/activities.json on the server (written by hand; without it only deluge-console is
tried, and only if it is installed):

    {"sources": [
        {"type": "deluge"},
        {"type": "arr", "app": "Sonarr", "url": "http://127.0.0.1:8989", "key": "..."},
        {"type": "arr", "app": "Radarr", "url": "http://127.0.0.1:7878", "key": "..."},
        {"type": "age", "name": "Backup", "path": "/mnt/backups/latest", "stale_hours": 30}]}

API keys stay on the server: only names, progress and states are printed. Every source fails
quietly (it simply contributes nothing).
"""
import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.request

CONF = os.path.expanduser("~/.config/coucou/activities.json")
MAX = 12


def load_conf():
    try:
        with open(CONF, encoding="utf-8") as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except Exception:
        return {}


def eta_seconds(text):
    """'5m 3s', '1h 2m', '2d 3h', '45s' -> seconds (None when it is not a duration)."""
    total, seen = 0, False
    for n, unit in re.findall(r"(\d+)\s*([dhms])", text or ""):
        total += int(n) * {"d": 86400, "h": 3600, "m": 60, "s": 1}[unit]
        seen = True
    return total if seen else None


def parse_deluge(text, label="Deluge"):
    """The output of `deluge-console info`: one block per torrent, blank-line separated."""
    out = []
    for block in re.split(r"\n\s*\n", text or ""):
        name = re.search(r"^Name:\s*(.+)$", block, re.M)
        if not name:
            continue
        state = re.search(r"^State:\s*([A-Za-z]+)", block, re.M)
        prog = re.search(r"Progress:\s*([\d.]+)%", block)
        eta = re.search(r"ETA:\s*([0-9dhms ]+?)(?:\s{2,}|$|\n)", block)
        tid = re.search(r"^ID:\s*(\S+)", block, re.M)
        st = (state.group(1) if state else "").lower()
        if st in ("downloading", "checking", "moving", "allocating"):
            mapped = "active"
        elif st == "queued":
            mapped = "queued"
        elif st == "error":
            mapped = "error"
        else:
            continue  # seeding, paused: not something the island needs to say
        out.append({
            "id": "deluge:" + (tid.group(1)[:12] if tid else name.group(1)[:20]),
            "app": label,
            "kind": "download",
            "title": name.group(1).strip()[:80],
            "progress": min(1.0, max(0.0, float(prog.group(1)) / 100.0)) if prog else None,
            "eta": eta_seconds(eta.group(1)) if eta else None,
            "state": mapped,
        })
    return out


def deluge(src):
    exe = shutil.which("deluge-console")
    if not exe:
        return []
    r = subprocess.run([exe, "info"], capture_output=True, text=True, timeout=8)
    return parse_deluge(r.stdout, src.get("label", "Deluge"))


def parse_arr(data, app):
    """Sonarr / Radarr `/api/v3/queue`: records with size, sizeleft, timeleft, status."""
    out = []
    records = data.get("records") if isinstance(data, dict) else None
    for rec in records if isinstance(records, list) else []:
        if not isinstance(rec, dict):
            continue
        size = rec.get("size") or 0
        left = rec.get("sizeleft") or 0
        status = str(rec.get("status", "")).lower()
        tracked = str(rec.get("trackedDownloadStatus", "")).lower()
        if tracked == "error" or status in ("failed", "warning"):
            state = "error"
        elif status in ("queued", "delay", "paused"):
            state = "queued"
        elif status in ("completed", "importpending", "imported"):
            continue
        else:
            state = "active"
        t = str(rec.get("timeleft", ""))
        m = re.match(r"(?:(\d+)\.)?(\d+):(\d+):(\d+)", t)
        eta = None
        if m:
            d, h, mi, s = (int(x or 0) for x in m.groups())
            eta = d * 86400 + h * 3600 + mi * 60 + s
        out.append({
            "id": "%s:%s" % (app.lower(), rec.get("id")),
            "app": app,
            "kind": "download",
            "title": str(rec.get("title", "?"))[:80],
            "progress": (1.0 - left / size) if size else None,
            "eta": eta,
            "state": state,
        })
    return out


def arr(src):
    app = str(src.get("app", "Arr"))
    url = str(src.get("url", "")).rstrip("/")
    key = str(src.get("key", ""))
    if not url.startswith("http://") or not key:
        return []  # only the server's own plain http endpoints; nothing is sent elsewhere
    req = urllib.request.Request(url + "/api/v3/queue?pageSize=50", headers={"X-Api-Key": key})
    with urllib.request.urlopen(req, timeout=4) as r:
        return parse_arr(json.load(r), app)


def age(src, now=None):
    """A job that runs now and then (a backup): when did it last write, and is that too long ago?"""
    now = now or time.time()
    name = str(src.get("name", "Job"))
    path = os.path.expanduser(str(src.get("path", "")))
    if not path or not os.path.exists(path):
        return [{"id": "age:" + name, "app": name, "kind": "backup", "title": name, "progress": None,
                 "eta": None, "state": "error", "detail": "not found"}]
    hours = (now - os.path.getmtime(path)) / 3600.0
    stale = float(src.get("stale_hours", 36))
    if hours < 1:
        detail = "%d min ago" % max(1, round(hours * 60))
    elif hours < 48:
        detail = "%d h ago" % round(hours)
    else:
        detail = "%d d ago" % round(hours / 24)
    return [{"id": "age:" + name, "app": name, "kind": "backup", "title": name, "progress": None, "eta": None,
             "state": "error" if hours > stale else "done", "detail": detail}]


HANDLERS = {"deluge": deluge, "arr": arr, "age": age}


def collect(conf=None):
    conf = load_conf() if conf is None else conf
    sources = conf.get("sources")
    if not isinstance(sources, list):
        sources = [{"type": "deluge"}]
    out = []
    for src in sources:
        if not isinstance(src, dict):
            continue
        handler = HANDLERS.get(src.get("type"))
        if not handler:
            continue
        try:
            out.extend(handler(src))
        except Exception:
            continue
    return out[:MAX]


if __name__ == "__main__":
    print(json.dumps({"at": int(time.time()), "activities": collect()}, ensure_ascii=False))
