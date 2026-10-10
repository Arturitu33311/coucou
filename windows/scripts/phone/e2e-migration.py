#!/usr/bin/env python3
"""Real binary in a private X: does the island leave when the phone is used and COME BACK when the person
returns to the laptop? Compares screenshots (visible / hidden / back)."""
import json, os, socket, subprocess, time, shutil, signal, pathlib, threading
from PIL import Image
import numpy as np

TMP = pathlib.Path(os.environ.get("COUCOU_E2E_DIR", "/tmp/coucou-e2e-migration"))
BIN = os.environ["COUCOU_BIN"]
XVFB = os.environ.get("COUCOU_XVFB") or shutil.which("Xvfb")
D = ":95"
shutil.rmtree(TMP, ignore_errors=True)
for d in ("home/.config/coucou", "home/.local/share/coucou", "run", "shim"):
    (TMP / d).mkdir(parents=True, exist_ok=True)
os.chmod(TMP / "run", 0o700)
(TMP / "shim/gdbus").write_text("#!/bin/sh\necho '(uint64 120000,)'\n")   # the laptop idle for 120 s
(TMP / "shim/loginctl").write_text("#!/bin/sh\necho no\n")
for f in ("gdbus", "loginctl"):
    os.chmod(TMP / "shim" / f, 0o755)
base = json.load(open(os.path.expanduser("~/.config/coucou/settings.json")))
base.update({"remoteEnabled": True, "serverHost": "", "jinxShare": False, "notificationPeek": False,
             "autostart": False, "activeIntegrations": [], "hooksInstalled": False})
(TMP / "home/.config/coucou/settings.json").write_text(json.dumps(base))
env = dict(os.environ, HOME=str(TMP / "home"), XDG_CONFIG_HOME=str(TMP / "home/.config"),
           XDG_DATA_HOME=str(TMP / "home/.local/share"), XDG_RUNTIME_DIR=str(TMP / "run"),
           XDG_CACHE_HOME=str(TMP / "home/.cache"), DISPLAY=D, GDK_BACKEND="x11",
           WEBKIT_DISABLE_COMPOSITING_MODE="1", PATH=f"{TMP/'shim'}:{os.environ['PATH']}")
env.pop("WAYLAND_DISPLAY", None)
procs = []
state = {"idle": 9999, "stop": False}
try:
    procs.append(subprocess.Popen([XVFB, D, "-screen", "0", "1400x700x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    time.sleep(1.5)
    procs.append(subprocess.Popen(["dbus-run-session", "--", BIN], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    sock = TMP / "run/coucou-remote.sock"
    for _ in range(240):
        if sock.exists():
            break
        time.sleep(0.25)
    time.sleep(7)  # page loads, scene clock starts
    s = socket.socket(socket.AF_UNIX); s.settimeout(5); s.connect(str(sock))
    f = s.makefile("rw", buffering=1); f.readline()

    def pump():
        while not state["stop"]:
            f.write(json.dumps({"op": "peer", "device": "s21", "idle": state["idle"], "screen_on": True,
                                "server_ok": False, "server_active": None, "music": False, "news": False}) + "\n")
            f.flush()
            f.readline()
            time.sleep(0.5)
    threading.Thread(target=pump, daemon=True).start()

    def shot(tag):
        p = TMP / f"{tag}.png"
        subprocess.run(["import", "-window", "root", str(p)], env=dict(os.environ, DISPLAY=D))
        return np.array(Image.open(p).convert("RGB")).astype(int)

    # Open the island so there is something to see (the collapsed one draws nothing in a bare X server).
    subprocess.run([BIN, "--toggle"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20)
    time.sleep(3)
    shown = shot("a_laptop_has_it")      # phone idle 9999: the laptop has Mochi
    state["idle"] = 0                    # the person uses the phone
    time.sleep(4)
    hidden = shot("b_phone_has_it")
    state["idle"] = 9999                 # back at the laptop: it is used now (idle 0), the phone goes quiet
    (TMP / "shim/gdbus").write_text("#!/bin/sh\necho '(uint64 0,)'\n")
    time.sleep(8)
    back = shot("c_back_on_laptop")
    log = (TMP / "home/.local/share/coucou/coucou.log").read_text()
    lines = [l for l in log.splitlines() if "presence:" in l]
    print("\n".join(lines))
    acts = [json.loads(l.split("presence: ",1)[1].split(" @",1)[0])["active"] for l in lines]
    # leaves when the phone is used, and COMES BACK when the person returns to the laptop
    ok = False in acts and acts[-1] is True and acts.index(False) < len(acts) - 1
    print("RESULT:", "OK island leaves and comes back" if ok else "PROBLEM", acts)
    s.close()
finally:
    state["stop"] = True
    for p in procs:
        try: p.terminate()
        except Exception: pass
    time.sleep(0.5)
    for p in procs:
        try: p.kill()
        except Exception: pass
    out = subprocess.run(["ps", "-eo", "pid,args"], capture_output=True, text=True).stdout
    for line in out.splitlines()[1:]:
        pid, _, args = line.strip().partition(" ")
        if str(TMP) in args and "migrate_xvfb" not in args:
            try: os.kill(int(pid), signal.SIGTERM)
            except Exception: pass
