# Server-side sources (kept here so the repo is the only copy that matters)

These run on the home server (`server`, Tailscale 100.92.254.125), not on the laptop. They are snapshots of what is
deployed; edit here, then copy to the server and restart the unit.

| Here | Deployed at | Unit | Notes |
|---|---|---|---|
| `catos/catos_server.py`, `catos/dashboard.html` | `~/.hermes/catos/` | `hermes-catos.service` (:8899) | Cat OS, Zynara removed 2026-10-14 |
| `homelab-dashboard/web/src/...` (5 files) | `~/homelab-dashboard/web/src/` | rebuild with `~/homelab-dashboard/deploy_web.sh` | only the files changed to drop Zynara; the rest of that project is not in this repo |

The VGA panel and the Jinx bridge scripts live in `../vga-panel` and `../jinx-assist` with their own `deploy.sh`.

Not here on purpose: secrets (`~/.hermes/secrets`, API keys) and the Hermes agent itself.
