#!/home/alan/.hermes/hermes-agent/venv/bin/python
"""monitor_autofix — Self-healing server monitor: restart, then deep-debug with subagents."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

HERMES = Path.home() / ".hermes"
STATE_FILE = HERMES / "state" / "monitor_autofix_state.json"
LOG_FILE = HERMES / "logs" / "monitor_autofix.log"
DISK_THRESHOLD = 85
OPENROUTER_WARN = 3.0

CRITICAL_SERVICES = [
    ("hermes-gateway", True),
    ("hermes-dashboard-web", False),
    ("hermes-dashboard-aggregator", False),
    ("hermes-jinx-api", False),
    ("hermes-vga-panel", False),
    ("hermes-mochi-presence", False),
    ("hermes-plataformas", False),
    ("hermes-catos", False),
]


def _env() -> dict:
    """Cron has no user session: point systemctl --user at the user bus."""
    env = dict(os.environ)
    env.setdefault("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
    env.setdefault("DBUS_SESSION_BUS_ADDRESS", f"unix:path={env['XDG_RUNTIME_DIR']}/bus")
    return env


def log(msg: str) -> None:
    ts = datetime.now(timezone.utc).isoformat()
    # cron already appends stderr to LOG_FILE; only echo on a terminal (was logging every line twice)
    if sys.stderr.isatty():
        print(f"[{ts}] {msg}", file=sys.stderr)
    LOG_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(LOG_FILE, "a") as f:
        f.write(f"[{ts}] {msg}\n")


def load_state() -> dict:
    if STATE_FILE.exists():
        try:
            return json.loads(STATE_FILE.read_text())
        except (json.JSONDecodeError, OSError):
            return {}
    return {}


def save_state(state: dict) -> None:
    STATE_FILE.parent.mkdir(parents=True, exist_ok=True)
    STATE_FILE.write_text(json.dumps(state, indent=2, default=str))


def run(cmd: list[str], timeout: int = 30) -> tuple[int, str]:
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=_env())
        return r.returncode, (r.stdout + r.stderr).strip()
    except FileNotFoundError:
        return -1, "command not found"
    except subprocess.TimeoutExpired:
        return -2, "timed out"
    except Exception as e:
        return -3, str(e)


UNKNOWN = "unknown"


def check_service(name: str, is_system: bool = False) -> tuple[bool | None, str]:
    """True = active, False = really down, None = could not tell (bus/systemctl error: never restart on that)."""
    cmd = ["systemctl", "is-active", name] if is_system else ["systemctl", "--user", "is-active", name]
    code, out = run(cmd)
    out = out.strip()
    if out in ("active", "activating", "reloading"):
        return out == "active", out
    if out in ("inactive", "failed", "deactivating"):
        return False, out
    return None, f"{UNKNOWN}: {out[:120]}"


def check_disk() -> tuple[bool, str]:
    code, out = run(["df", "/", "--output=pcent"])
    if code != 0:
        return False, f"df error: {out}"
    try:
        pct = int(out.strip().split("\n")[-1].replace("%", "").strip())
    except (IndexError, ValueError):
        return False, f"parse error: {out}"
    return pct < DISK_THRESHOLD, f"{pct}%"


def check_openrouter() -> tuple[bool, str]:
    venv = str(HERMES / "hermes-agent" / "venv" / "bin" / "python")
    script = str(HERMES / "scripts" / "openrouter_saldo.py")
    code, out = run([venv, script])
    if code != 0:
        return True, f"skip (error: {out})"
    try:
        s = float(out.strip())
    except ValueError:
        return True, f"skip (parse: {out})"
    return s > OPENROUTER_WARN, f"${s:.2f}"


def collect_diagnostics(service: str, is_system: bool) -> str:
    """Gather logs and config for debugging a failed service."""
    parts = []
    parts.append(f"=== systemctl status {service} ===")
    flag = [] if is_system else ["--user"]
    _, out = run(["systemctl"] + flag + ["status", service, "--no-pager", "-l", "-n", "30"])
    parts.append(out[:2000])
    parts.append(f"=== journalctl -u {service} (last 30 lines) ===")
    _, jout = run(["journalctl"] + flag + ["-u", service, "--no-pager", "-n", "30"])
    parts.append(jout[:2000])
    return "\n".join(parts)


def notify(message: str) -> None:
    """Deliver through notify_alan.sh, the one piece that owns the Telegram route (token off argv, exit 0 only if Telegram said ok)."""
    code, out = run([str(HERMES / "scripts" / "notify_alan.sh"), f"[monitor_autofix] {message}"], timeout=90)
    log(f"notified ({'ok' if code == 0 else 'FAILED ' + out[-80:]}): {message[:100]}")


def fix_restart(service: str, is_system: bool = False) -> tuple[bool, str]:
    log(f"auto-fix: restarting {service}")
    cmd = ["systemctl", "restart", service] if is_system else ["systemctl", "--user", "restart", service]
    run(cmd, timeout=15)
    time.sleep(3)
    return check_service(service, is_system=is_system)


def request_deep_debug(service: str, status: str, is_system: bool, diagnostics: str) -> None:
    """Leave a debug request for Jinx to investigate with subagents."""
    req = HERMES / "state" / "autofix_debug_request.json"
    try:
        pending = json.loads(req.read_text()).get("pending", {})
    except (OSError, json.JSONDecodeError, AttributeError):
        pending = {}
    data = {
        "service": service,
        "status": status,
        "is_system": is_system,
        "diagnostics": diagnostics[:3000],
        "requested_at": datetime.now(timezone.utc).isoformat(),
        "attempted_fix": "restart x2",
    }
    pending[service] = data
    # top level = latest request (the shape the server-maintenance skill documents); "pending" = every service waiting
    req.write_text(json.dumps({**data, "pending": pending}, indent=2))
    log(f"Deep debug requested for {service} — see {req}")


def main() -> int:
    dry_run = "--dry-run" in sys.argv
    if "--self-check" in sys.argv:
        print("monitor_autofix.py: self-check OK")
        return 0

    state = load_state()
    alerts = []
    recovers = []
    debug_requests = []  # services needing deep investigation

    def track(key: str, ok: bool) -> str:
        e = state.setdefault(key, {"last_ok": True, "fails": 0})
        if ok:
            was_bad = not e["last_ok"]
            e["last_ok"] = True; e["fails"] = 0
            return "recovered" if was_bad else "ok"
        e["last_ok"] = False
        e["fails"] = e.get("fails", 0) + 1
        f = e["fails"]
        if f == 1: return "new_fail"
        if f >= 3 and f % 3 == 0: return "repeat_fail"
        return "ongoing"

    # Services: check -> restart (x2) -> deep debug
    for svc, is_sys in CRITICAL_SERVICES:
        ok, status = check_service(svc, is_system=is_sys)
        if ok is None:  # could not tell: say so once in the log, do not touch the service or the counters
            log(f"skip {svc}: {status}")
            continue
        act = track(f"svc:{svc}", ok)
        if ok and act == "recovered":
            recovers.append(f"{svc} recuperado")
        if not ok and act in ("new_fail", "repeat_fail"):
            if not dry_run:
                # Simple fix: restart (up to 2 attempts)
                fok, s2 = fix_restart(svc, is_sys)
                if not fok:
                    fok, s2 = fix_restart(svc, is_sys)
                if fok:
                    recovers.append(f"{svc} reparado automaticamente")
                else:
                    # Simple restart failed — collect diagnostics and queue deep debug
                    diag = collect_diagnostics(svc, is_sys)
                    request_deep_debug(svc, s2, is_sys, diag)
                    debug_requests.append(svc)
                    alerts.append(f"{svc}: {s2} (restart no funciono, debug encolado)")
            else:
                alerts.append(f"{svc}: {status}")

    # Disk
    dok, dinfo = check_disk()
    da = track("disk", dok)
    if dok and da == "recovered":
        recovers.append(f"disco recuperado: {dinfo}")
    if not dok and da in ("new_fail", "repeat_fail"):
        alerts.append(f"disco al {dinfo}")

    # OpenRouter
    rok, rinfo = check_openrouter()
    ra = track("openrouter", rok)
    if rok and ra == "recovered":
        recovers.append(f"OpenRouter ok: {rinfo}")
    if not rok and ra in ("new_fail", "repeat_fail"):
        alerts.append(f"OpenRouter bajo: {rinfo}")

    save_state(state)

    if not alerts and not recovers:
        # Clean up any stale debug request files
        req = HERMES / "state" / "autofix_debug_request.json"
        if req.exists():
            req.unlink()
        log("monitor OK")
        return 0

    parts = []
    if recovers: parts.append("Reparado: " + "; ".join(recovers[:5]))
    if alerts: parts.append("Fallo: " + "; ".join(alerts[:5]))
    report = " | ".join(parts)
    log(report)

    if debug_requests:
        notify(f"Servicios que no pude reparar solos y requieren debug: {', '.join(debug_requests)}. Cuando puedas dime y los reviso con agentes de codigo.")
    elif alerts:
        notify(f"Reporte monitor: {report[:300]}")
    elif recovers:
        notify(f"Auto-fixes: {'; '.join(recovers[:3])}")

    return 1 if alerts else 0


if __name__ == "__main__":
    sys.exit(main())