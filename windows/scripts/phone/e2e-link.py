#!/usr/bin/env python3
"""End-to-end of the phone link against the real Coucou binary (private X server, session bus, HOME).

A fake phone speaks the protocol over the unix socket: beats, task lists, hostile lines.
Needs: Xvfb, dbus-run-session, a built Coucou (COUCOU_BIN=.../target/debug/coucou); a copy of your
settings.json is read (never written). Prints PASS/FAIL per check, exits non-zero on any failure.
"""
import json, os, socket, subprocess, sys, time, shutil, signal, pathlib

T0 = int(time.time()) - 10000
TMP = pathlib.Path(os.environ.get("COUCOU_E2E_DIR", "/tmp/coucou-e2e"))
BIN = os.environ["COUCOU_BIN"]
XVFB = os.environ.get("COUCOU_XVFB") or shutil.which("Xvfb")
DISPLAY = ":97"

def reap():
    """Session helpers (gvfs fuse, portals, at-spi) outlive the bus: end the ones that live in TMP."""
    out = subprocess.run(["ps", "-eo", "pid,args"], capture_output=True, text=True).stdout
    for line in out.splitlines()[1:]:
        pid, _, args = line.strip().partition(" ")
        if str(TMP) in args and "e2e.py" not in args:
            try:
                os.kill(int(pid), signal.SIGTERM)
            except Exception:
                pass
    time.sleep(1)

reap()
if TMP.exists():
    shutil.rmtree(TMP, ignore_errors=True)
for d in ("home/.config/coucou", "home/.local/share/coucou", "run", "shim"):
    (TMP / d).mkdir(parents=True, exist_ok=True)
os.chmod(TMP / "run", 0o700)

# GNOME's idle monitor is not in Xvfb: the shim says the laptop has been idle for 120 s and unlocked.
(TMP / "shim/gdbus").write_text("#!/bin/sh\necho '(uint64 120000,)'\n")
(TMP / "shim/loginctl").write_text("#!/bin/sh\necho no\n")
for f in ("gdbus", "loginctl"):
    os.chmod(TMP / "shim" / f, 0o755)

base = json.load(open(os.path.expanduser("~/.config/coucou/settings.json")))
base.update({"remoteEnabled": True, "serverHost": "", "jinxShare": False, "notificationPeek": False,
             "autostart": False, "activeIntegrations": [], "hooksInstalled": False})
(TMP / "home/.config/coucou/settings.json").write_text(json.dumps(base))
# A task already on the laptop, older than what the phone will say.
(TMP / "home/.local/share/coucou/tasks.json").write_text(json.dumps([
    {"id": "L-1", "title": "laptop task", "done": False, "created": T0, "updatedAt": T0},
    {"id": "S-1", "title": "stale on laptop", "done": False, "created": T0, "updatedAt": T0},
]))

env = dict(os.environ)
env.update({
    "HOME": str(TMP / "home"), "XDG_CONFIG_HOME": str(TMP / "home/.config"),
    "XDG_DATA_HOME": str(TMP / "home/.local/share"), "XDG_RUNTIME_DIR": str(TMP / "run"),
    "XDG_CACHE_HOME": str(TMP / "home/.cache"),
    "DISPLAY": DISPLAY, "PATH": f"{TMP/'shim'}:{os.environ['PATH']}",
    "WEBKIT_DISABLE_COMPOSITING_MODE": "1", "GDK_BACKEND": "x11",
})
env.pop("WAYLAND_DISPLAY", None)

procs = []
def cleanup():
    for p in procs:
        try:
            p.terminate()
        except Exception:
            pass
    time.sleep(0.5)
    for p in procs:
        try:
            p.kill()
        except Exception:
            pass
    reap()

fails = 0
def check(name, ok, extra=""):
    global fails
    print(("PASS " if ok else "FAIL ") + name + (f"  [{extra}]" if extra and not ok else ""))
    if not ok:
        fails += 1

try:
    x = subprocess.Popen([XVFB, DISPLAY, "-screen", "0", "1280x800x24", "-nolisten", "tcp"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    procs.append(x)
    time.sleep(1.5)
    app = subprocess.Popen(["dbus-run-session", "--", BIN], env=env,
                           stdout=open(TMP / "app.out", "w"), stderr=subprocess.STDOUT)
    procs.append(app)

    sock_path = TMP / "run/coucou-remote.sock"
    for _ in range(240):
        if sock_path.exists():
            break
        time.sleep(0.25)
    check("the phone socket appears when the link is on", sock_path.exists())
    if not sock_path.exists():
        print(open(TMP / "app.out").read()[-1500:]); print([ (p.name, p.read_text()[-1500:]) for p in (TMP/"home/.local/share/coucou").glob("*.log")])
        sys.exit(1)

    def connect():
        s = socket.socket(socket.AF_UNIX)
        s.settimeout(5)
        s.connect(str(sock_path))
        return s, s.makefile("rw", buffering=1)

    def ask(f, obj, want=None):
        f.write(json.dumps(obj) + "\n"); f.flush()
        while True:
            line = json.loads(f.readline())
            if want is None or line.get("t") == want:
                return line

    s, f = connect()
    hello = json.loads(f.readline())
    check("hello on connect", hello.get("t") == "hello")

    beat = lambda idle, **k: dict({"op": "peer", "device": "s21", "idle": idle, "screen_on": True,
                                   "server_ok": False, "server_active": None, "music": False, "news": False}, **k)

    r = ask(f, beat(2))
    check("phone used 2 s ago, laptop idle 120 s: the phone has Mochi", r.get("t") == "peer" and r.get("active") is False, r)
    check("the laptop's beat carries its own state", r.get("device") == "laptop" and r.get("idle", 0) >= 100 and r.get("screen_on") is True, r)

    r = ask(f, beat(500))
    check("the phone idle for 500 s: the laptop takes Mochi back", r.get("active") is True, r)

    r = ask(f, beat(500, screen_on=False))
    check("a phone with its screen off never has Mochi", r.get("active") is True, r)

    r = ask(f, beat(0, server_ok=True, server_active="s21"))
    check("server answers only for the phone: laptop does not reach it, idle decides (phone)", r.get("active") is False, r)

    r = ask(f, beat(500, server_ok=True, server_active="laptop"))
    check("mixed server view with the phone idle: laptop", r.get("active") is True, r)

    # hostile lines
    r = ask(f, {"op": "peer", "device": "laptop", "idle": 0}, want="ack")
    check("a beat claiming to be the laptop is refused", r.get("ok") is False, r)
    r = ask(f, {"op": "run", "cmd": "id"}, want="ack")
    check("an unknown op is refused", r.get("ok") is False, r)
    r = ask(f, beat(10**30))
    check("an absurd idle is clamped, not fatal", r.get("t") == "peer", r)

    # tasks: the phone's newer change wins, the laptop's own survives, the phone gets the merged list
    phone_tasks = [
        {"id": "S-1", "title": "stale on laptop", "done": True, "created": T0, "completedAt": T0 + 1000,
         "updatedAt": T0 + 1000, "deleted": False},
        {"id": "P-1", "title": "phone task", "done": False, "created": T0 + 500, "updatedAt": T0 + 500, "deleted": False},
    ]
    r = ask(f, {"op": "tasks_sync", "tasks": phone_tasks}, want="tasks")
    ids = {t["id"]: t for t in r["tasks"]}
    check("the merged list answers the phone", set(ids) == {"L-1", "S-1", "P-1"}, list(ids))
    check("the phone's newer change won", ids.get("S-1", {}).get("done") is True and ids["S-1"]["updatedAt"] == T0 + 1000, ids.get("S-1"))
    check("the laptop's own task is kept", ids.get("L-1", {}).get("title") == "laptop task")
    stored = json.loads((TMP / "home/.local/share/coucou/tasks.json").read_text())
    check("the laptop saved the merge", {t["id"] for t in stored} == {"L-1", "S-1", "P-1"})

    # an older edit from the phone changes nothing; a deletion beats an older edit and travels as a tombstone
    old = [{"id": "L-1", "title": "phone edit, older", "done": False, "created": T0, "updatedAt": T0 - 500}]
    r = ask(f, {"op": "tasks_sync", "tasks": old}, want="tasks")
    check("an older change does not win", {t["id"]: t for t in r["tasks"]}["L-1"]["title"] == "laptop task")
    dele = [{"id": "P-1", "title": "phone task", "done": False, "created": T0 + 500, "updatedAt": T0 + 2000, "deleted": True}]
    r = ask(f, {"op": "tasks_sync", "tasks": dele}, want="tasks")
    p1 = {t["id"]: t for t in r["tasks"]}.get("P-1", {})
    check("a deletion beats the older edit", p1.get("deleted") is True, p1)

    # bounded list
    r = ask(f, {"op": "tasks_sync", "tasks": [{"id": f"x{i}", "title": "t"} for i in range(1001)]}, want="ack")
    check("an oversize task list is refused", r.get("ok") is False, r)

    # the link going down must not wedge anything: a new connection works at once
    s.close()
    time.sleep(0.5)
    s2, f2 = connect()
    json.loads(f2.readline())
    r = ask(f2, beat(500))
    check("a reconnect is served", r.get("t") == "peer" and r.get("active") is True, r)

    check("the app is still alive", app.poll() is None)
finally:
    cleanup()

print("FAILED" if fails else "ALL PASSED", fails)
sys.exit(1 if fails else 0)
