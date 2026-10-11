#!/usr/bin/env python3
"""Tests of the server-side script (src-tauri/src/activities.py): `python3 windows/scripts/test-activities.py`.

Runs entirely on this machine: a fake Sonarr on localhost, a fake deluge-console output, a
temporary "backup" file. Nothing touches a real server.
"""
import http.server
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "..", "src-tauri", "src", "activities.py")
spec = importlib.util.spec_from_file_location("activities", SCRIPT)
act = importlib.util.module_from_spec(spec)
spec.loader.exec_module(act)

failures = []


def check(cond, msg):
    if not cond:
        failures.append(msg)
        print("FAIL", msg)


# ── deluge-console info ─────────────────────────────────────────────────────────
DELUGE = """Name: Some.Show.S01E01.1080p
ID: ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12
State: Downloading Down Speed: 1.2 MiB/s Up Speed: 0.0 KiB/s ETA: 5m 3s
Seeds: 10 (50) Peers: 2 (10) Availability: 10.00
Size: 100.0 MiB/1.0 GiB Ratio: 0.000
Progress: 9.77% [####~~~~~]

Name: Old.Movie.1999
ID: ffffffffffffffffffffffffffffffffffffffff
State: Seeding Up Speed: 5.0 KiB/s
Progress: 100.00% [########]

Name: Waiting.Show.S02E05
ID: 1111111111111111111111111111111111111111
State: Queued
Progress: 0.00% [~~~~~]

Name: Broken.Thing
ID: 2222222222222222222222222222222222222222
State: Error
Progress: 33.30% [###~~~]
"""
d = act.parse_deluge(DELUGE)
check([x["state"] for x in d] == ["active", "queued", "error"], "deluge states (seeding is left out): %r" % d)
check(abs(d[0]["progress"] - 0.0977) < 1e-6 and d[0]["eta"] == 303, "deluge progress and eta: %r" % d[0])
check(all(len(x["title"]) <= 80 for x in d), "titles are short")
check(act.parse_deluge("") == [], "empty")

# ── eta ─────────────────────────────────────────────────────────────────────────
check(act.eta_seconds("5m 3s") == 303 and act.eta_seconds("1h 2m") == 3720 and act.eta_seconds("2d 3h") == 183600, "eta")
check(act.eta_seconds("∞") is None and act.eta_seconds("") is None, "no eta")

# ── Sonarr / Radarr queue ───────────────────────────────────────────────────────
QUEUE = {"records": [
    {"id": 1, "title": "A.Show.S01E01", "size": 1000, "sizeleft": 250, "timeleft": "00:12:34", "status": "downloading", "trackedDownloadStatus": "ok"},
    {"id": 2, "title": "Q.Show.S01E02", "size": 1000, "sizeleft": 1000, "status": "queued", "trackedDownloadStatus": "ok"},
    {"id": 3, "title": "Bad.Show", "size": 1000, "sizeleft": 500, "status": "downloading", "trackedDownloadStatus": "error"},
    {"id": 4, "title": "Done.Show", "size": 1000, "sizeleft": 0, "status": "completed", "trackedDownloadStatus": "ok"},
    {"id": 5, "title": "Long.Time", "size": 10, "sizeleft": 5, "timeleft": "1.02:03:04", "status": "downloading"},
]}
q = act.parse_arr(QUEUE, "Sonarr")
check([x["state"] for x in q] == ["active", "queued", "error", "active"], "arr states: %r" % [x["state"] for x in q])
check(abs(q[0]["progress"] - 0.75) < 1e-9 and q[0]["eta"] == 754, "arr progress and eta: %r" % q[0])
check(q[3]["eta"] == 86400 + 2 * 3600 + 3 * 60 + 4, "arr eta with days")
check(act.parse_arr(None, "x") == [] and act.parse_arr({"records": "no"}, "x") == [], "arr garbage")


# ── a real HTTP round trip against a fake Sonarr, key checked ───────────────────
class Handler(http.server.BaseHTTPRequestHandler):
    seen = []

    def do_GET(self):
        Handler.seen.append((self.path, self.headers.get("X-Api-Key")))
        body = json.dumps(QUEUE).encode()
        self.send_response(200 if self.headers.get("X-Api-Key") == "secret" else 401)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(body if self.headers.get("X-Api-Key") == "secret" else b"{}")

    def log_message(self, *a):
        pass


srv = http.server.HTTPServer(("127.0.0.1", 0), Handler)
threading.Thread(target=srv.serve_forever, daemon=True).start()
url = "http://127.0.0.1:%d" % srv.server_address[1]
got = act.arr({"app": "Sonarr", "url": url, "key": "secret"})
check(len(got) == 4 and Handler.seen[0] == ("/api/v3/queue?pageSize=50", "secret"), "arr over http: %r" % got)
check(act.arr({"app": "x", "url": "https://example.com", "key": "k"}) == [], "only plain http endpoints")
check(act.arr({"app": "x", "url": url, "key": ""}) == [], "no key, no call")

# ── a job that runs now and then ────────────────────────────────────────────────
with tempfile.NamedTemporaryFile() as f:
    now = time.time()
    check(act.age({"name": "Backup", "path": f.name, "stale_hours": 30}, now)[0]["state"] == "done", "fresh backup")
    old = act.age({"name": "Backup", "path": f.name, "stale_hours": 30}, now + 40 * 3600)[0]
    check(old["state"] == "error" and old["detail"] == "40 h ago", "stale backup: %r" % old)
check(act.age({"name": "Gone", "path": "/nonexistent/zzz"})[0]["state"] == "error", "missing path")

# ── collect: unknown sources and broken ones are skipped, the rest survives ─────
conf = {"sources": [{"type": "nope"}, "junk", {"type": "arr", "app": "Sonarr", "url": url, "key": "secret"},
                    {"type": "arr", "app": "Dead", "url": "http://127.0.0.1:1", "key": "k"}]}
c = act.collect(conf)
check(len(c) == 4 and all(x["app"] == "Sonarr" for x in c), "collect keeps what works: %r" % c)
check(act.collect({"sources": "not a list"}) is not None, "bad config does not raise")

# ── the script itself prints one JSON line ──────────────────────────────────────
with tempfile.TemporaryDirectory() as home:
    os.makedirs(os.path.join(home, ".config", "coucou"))
    with open(os.path.join(home, ".config", "coucou", "activities.json"), "w") as f:
        json.dump({"sources": [{"type": "arr", "app": "Sonarr", "url": url, "key": "secret"}]}, f)
    r = subprocess.run([sys.executable, SCRIPT], capture_output=True, text=True, env={**os.environ, "HOME": home}, timeout=20)
    lines = [l for l in r.stdout.splitlines() if l.strip()]
    check(r.returncode == 0 and len(lines) == 1, "one line of output")
    out = json.loads(lines[0])
    check(len(out["activities"]) == 4 and "secret" not in r.stdout, "the key never leaves the server")

srv.shutdown()
print("FAILED" if failures else "all ok")
sys.exit(1 if failures else 0)
