# Runs on the server (sent on stdin by remote_agents.rs; nothing is installed there).
# argv: "list"  |  "messages" <session id> <offset>
# list      -> the output of `claude agents --json` (one JSON line), or {"error": "..."}
# messages  -> first line: the byte offset to read from; then the transcript's bytes from there (capped).
import glob, json, os, shutil, subprocess, sys

TAIL = 256 * 1024
CAP = 2 * 1024 * 1024


def claude():
    found = shutil.which("claude")
    if found:
        return found
    for rel in (".local/bin/claude", ".claude/local/claude", ".npm-global/bin/claude"):
        p = os.path.join(os.path.expanduser("~"), rel)
        if os.path.exists(p):
            return p
    return None


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    if what == "list":
        exe = claude()
        if not exe:
            print(json.dumps({"error": "claude is not installed on the server"}))
            return
        try:
            r = subprocess.run([exe, "agents", "--json"], capture_output=True, text=True, timeout=20, stdin=subprocess.DEVNULL)
        except Exception as e:  # noqa: BLE001
            print(json.dumps({"error": "claude agents failed: %s" % type(e).__name__}))
            return
        if r.returncode != 0:
            print(json.dumps({"error": "claude agents failed"}))
            return
        print(r.stdout.strip() or "[]")
        return
    if what == "messages":
        sid, offset = sys.argv[2], int(sys.argv[3])
        paths = glob.glob(os.path.join(os.path.expanduser("~"), ".claude", "projects", "*", sid + ".jsonl"))
        if not paths:
            print("-1")
            return
        with open(paths[0], "rb") as f:
            size = os.fstat(f.fileno()).st_size
            start = 0 if offset > size else (size - TAIL if offset == 0 and size > TAIL else offset)
            f.seek(start)
            data = f.read(CAP)
        sys.stdout.write("%d\n" % start)
        sys.stdout.flush()
        sys.stdout.buffer.write(data)
        return
    print(json.dumps({"error": "unknown request"}))


main()
