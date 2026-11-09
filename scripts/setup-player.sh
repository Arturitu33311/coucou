#!/usr/bin/env bash
# Prepares this Linux laptop for Coucou's glass player (search and play Spotify + YT Music from the island).
# Safe to run again: it only installs what is missing or out of date. No sudo; it says what to install
# with apt if something system-wide is missing.
#
#   scripts/setup-player.sh                      # everything
#   scripts/setup-player.sh --client-id <id>     # also store the Spotify app's Client ID in Coucou's settings
#
# What it puts in place (all under $HOME):
#   ~/.local/bin/yt-dlp                     official release, checked against its SHA2-256SUMS
#   ~/.local/share/coucou/ytm-venv          Python venv with ytmusicapi (the YT Music helper's interpreter)
#   ~/.local/bin/spotifyd                   official "full" release (with MPRIS), checked against its .sha512
#   ~/.config/systemd/user/coucou-spotifyd.service   started by Coucou on demand, never at login
#   ~/.config/coucou/yt-dlp.conf            yt-dlp options for Coucou only: IPv4 first, a JavaScript runtime
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SPOTIFYD_VERSION="v0.4.2"
BIN="$HOME/.local/bin"
VENV="$HOME/.local/share/coucou/ytm-venv"
CONF_DIR="$HOME/.config/coucou"
UNIT_DIR="$HOME/.config/systemd/user"
CLIENT_ID=""

while [ $# -gt 0 ]; do
  case "$1" in
    --client-id) CLIENT_ID="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$BIN" "$CONF_DIR" "$UNIT_DIR"

echo "== yt-dlp"
if [ -x "$BIN/yt-dlp" ]; then
  "$BIN/yt-dlp" -U >/dev/null 2>&1 || echo "   (could not update yt-dlp now; keeping $("$BIN/yt-dlp" --version))"
else
  (cd "$TMP" && curl -fsSLO https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp \
              && curl -fsSLO https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS \
              && grep ' yt-dlp$' SHA2-256SUMS | sha256sum -c --quiet -)
  install -m 755 "$TMP/yt-dlp" "$BIN/yt-dlp"
fi
echo "   yt-dlp $("$BIN/yt-dlp" --version)"

echo "== YT Music helper (ytmusicapi)"
[ -x "$VENV/bin/python3" ] || python3 -m venv "$VENV"
"$VENV/bin/pip" install -q --upgrade pip ytmusicapi
echo "   ytmusicapi $("$VENV/bin/python3" -c 'import ytmusicapi; print(ytmusicapi.__version__)')"

echo "== spotifyd"
if [ -x "$BIN/spotifyd" ] && "$BIN/spotifyd" --version | grep -q "${SPOTIFYD_VERSION#v}"; then
  echo "   $("$BIN/spotifyd" --version) already installed"
else
  base="https://github.com/Spotifyd/spotifyd/releases/download/$SPOTIFYD_VERSION"
  (cd "$TMP" && curl -fsSLO "$base/spotifyd-linux-x86_64-full.tar.gz" \
              && curl -fsSLO "$base/spotifyd-linux-x86_64-full.sha512" \
              && sha512sum -c --quiet spotifyd-linux-x86_64-full.sha512 \
              && tar -xzf spotifyd-linux-x86_64-full.tar.gz)
  install -m 755 "$TMP/spotifyd" "$BIN/spotifyd"
  echo "   $("$BIN/spotifyd" --version) installed"
fi
install -m 644 "$REPO_ROOT/linux/systemd/coucou-spotifyd.service" "$UNIT_DIR/coucou-spotifyd.service"
systemctl --user daemon-reload
echo "   unit coucou-spotifyd.service installed (Coucou starts it when it needs it)"

echo "== yt-dlp options for Coucou"
node_bin=""
for n in "$HOME"/.nvm/versions/node/v*/bin/node "$(command -v node || true)"; do
  [ -x "$n" ] || continue
  major="$("$n" --version | sed 's/^v\([0-9]*\).*/\1/')"
  if [ "${major:-0}" -ge 22 ]; then node_bin="$n"; fi
done
{
  echo "# Written by scripts/setup-player.sh — used only by Coucou's player (through mpv)."
  echo "--force-ipv4"
  if [ -n "$node_bin" ]; then echo "--js-runtimes node:$node_bin"; fi
} > "$CONF_DIR/yt-dlp.conf"
if [ -n "$node_bin" ]; then
  echo "   $CONF_DIR/yt-dlp.conf (IPv4 first, JavaScript runtime $node_bin)"
else
  echo "   $CONF_DIR/yt-dlp.conf (IPv4 first). No Node >= 22 found: YouTube may hide some formats."
fi

echo "== system pieces (apt)"
missing=()
dpkg -s mpv >/dev/null 2>&1 || missing+=(mpv)
dpkg -s mpv-mpris >/dev/null 2>&1 || missing+=(mpv-mpris)
command -v xdotool >/dev/null 2>&1 || missing+=(xdotool)
if [ ${#missing[@]} -gt 0 ]; then
  echo "   Missing: ${missing[*]}  →  sudo apt install ${missing[*]}"
else
  echo "   mpv, mpv-mpris and xdotool are installed"
fi

if [ -n "$CLIENT_ID" ]; then
  echo "== Spotify Client ID"
  python3 - "$CONF_DIR/settings.json" "$CLIENT_ID" <<'PY'
import json, sys, os, shutil, time
path, cid = sys.argv[1], sys.argv[2]
data = {}
if os.path.exists(path):
    shutil.copy2(path, f"{path}.bak-player-{time.strftime('%Y%m%d-%H%M%S')}")
    with open(path) as f:
        data = json.load(f)
data["spotifyClientId"] = cid
tmp = path + ".tmp"
with open(tmp, "w") as f:
    json.dump(data, f, indent=2, ensure_ascii=False)
os.replace(tmp, path)
print(f"   spotifyClientId set in {path} (backup kept)")
PY
fi

cat <<'EOF'

Next, once:
  1. Sign spotifyd in to your Spotify account:   ~/.local/bin/spotifyd authenticate
  2. In Coucou, open the player (Super+Alt+M) and press "Connect Spotify".
EOF
