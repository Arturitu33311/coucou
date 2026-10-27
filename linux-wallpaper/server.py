#!/usr/bin/env python3
"""Coucou Linux wallpaper: a tiny local server behind the animated desktop wallpaper (wallpaper.html).

It listens on 127.0.0.1 only and gives the page what the phone's wallpaper reads on Android:
  /state    what is playing (any MPRIS player, through `playerctl`), battery, Wi-Fi level
  /art      the cover of the song, fetched from the URL the player itself published (so the page can read its colours)
  /lyrics   time-synced lyrics from LRCLIB (lrclib.net). Opt-in: only with --lyrics, because it sends artist, title
            and album of each song to that service; each song is looked up once and cached.

Standard library only. Nothing is written outside ~/.cache/coucou-wallpaper.
"""
import argparse
import hashlib
import http.client
import ipaddress
import json
import os
import re
import socket
import ssl
import subprocess
import time
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
CACHE = os.path.join(os.path.expanduser("~"), ".cache", "coucou-wallpaper")
AGENT = "Coucou-Linux-wallpaper (https://github.com/Arturitu33311/coucou)"
LYRICS_ON = False
CURRENT_ART = ""
PORT = 8765

_state_cache = (0.0, None)


def run(cmd, timeout=1.5):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout).stdout.strip()
    except Exception:
        return ""


def player_state():
    sep = "|||"
    fmt = sep.join(["{{status}}", "{{title}}", "{{artist}}", "{{album}}", "{{mpris:artUrl}}", "{{mpris:length}}", "{{position}}"])
    out = run(["playerctl", "metadata", "--format", fmt])
    if not out:
        return None
    p = out.split(sep)
    if len(p) < 7:
        return None
    status, title, artist, album, art, length, pos = p[:7]
    if not title:
        return None
    global CURRENT_ART
    CURRENT_ART = art      # kept here: /art serves this and never a URL from the request
    to_int = lambda s: int(s) if s.strip().lstrip("-").isdigit() else 0
    return {
        "status": status, "title": title, "artist": artist, "album": album,
        "art": "/art?u=" + urllib.parse.quote(art, safe="") if art else "",
        "lengthMs": to_int(length) // 1000, "positionMs": to_int(pos) // 1000,
    }


def battery_state():
    base = "/sys/class/power_supply"
    try:
        for name in sorted(os.listdir(base)):
            if name.startswith("BAT"):
                with open(f"{base}/{name}/capacity") as f:
                    pct = int(f.read().strip())
                with open(f"{base}/{name}/status") as f:
                    st = f.read().strip()
                return {"pct": pct, "charging": st in ("Charging", "Full")}
    except Exception:
        pass
    return None


def wifi_state():
    out = run(["nmcli", "-t", "-f", "ACTIVE,SIGNAL", "dev", "wifi"])
    for line in out.splitlines():
        a, _, sig = line.partition(":")
        if a == "yes" and sig.isdigit():
            return {"connected": True, "level": min(4, int(sig) * 5 // 100)}
    eth = run(["nmcli", "-t", "-f", "TYPE,STATE", "dev"])
    return {"connected": "ethernet:connected" in eth, "level": 0, "wired": "ethernet:connected" in eth}


def state():
    global _state_cache
    now = time.time()
    if now - _state_cache[0] < 0.4 and _state_cache[1] is not None:
        return _state_cache[1]
    s = {"player": player_state(), "battery": battery_state(), "wifi": wifi_state(),
         "lyricsEnabled": LYRICS_ON, "serverTime": int(now * 1000)}
    _state_cache = (now, s)
    return s


def _public_host(host, port=443):
    """The addresses to connect to, or None unless every address the name resolves to is public (a global address:
    not loopback, private, link-local, reserved or multicast). The caller connects to THESE addresses, so the name is
    never resolved a second time (which is what DNS rebinding relies on)."""
    try:
        infos = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    except OSError:
        return None
    if not infos or not all(ipaddress.ip_address(i[4][0]).is_global for i in infos):
        return None
    return [i[4][0] for i in infos]


def _get_pinned(url, limit=8_000_000):
    """GET [url] over http(s) to the checked address, with the original name for Host and TLS; redirects are not followed."""
    u = urllib.parse.urlparse(url)
    https = u.scheme == "https"
    port = u.port or (443 if https else 80)
    ips = _public_host(u.hostname, port)
    if ips is None:
        raise ValueError("cover host is not a public address")
    host = u.hostname

    class Pinned(http.client.HTTPSConnection if https else http.client.HTTPConnection):
        def connect(self):
            last = None
            for ip in ips:                      # only addresses that were checked; the name is not resolved again
                try:
                    sock = socket.create_connection((ip, port), timeout=4)
                    self.sock = ssl.create_default_context().wrap_socket(sock, server_hostname=host) if https else sock
                    return
                except OSError as e:
                    last = e
            raise last or OSError("no address")

    conn = Pinned(host, port, timeout=6)
    try:
        conn.request("GET", (u.path or "/") + ("?" + u.query if u.query else ""), headers={"User-Agent": AGENT})
        r = conn.getresponse()
        if r.status != 200:
            raise ValueError(f"cover server answered {r.status}")
        data = r.read(limit + 1)
        if len(data) > limit:
            raise ValueError("cover too big")
        return data
    finally:
        conn.close()


def _art_dirs():
    home = os.path.expanduser("~")
    return [os.path.realpath(p) for p in ("/tmp", "/var/tmp", os.path.join(home, ".cache"), os.path.join(home, ".local", "share"),
                                          os.path.join(home, ".var", "app"), os.path.join(home, ".config"), os.path.join(home, "snap"))]


def fetch_art(url):
    """The cover the player published, and only that: file:// only for images inside cache / temp folders, http(s)
    only to public hosts and without following redirects."""
    u = urllib.parse.urlparse(url)
    if u.scheme == "file":
        path = os.path.realpath(urllib.parse.unquote(u.path))
        ok_dir = any(path == d or path.startswith(d + os.sep) for d in _art_dirs())
        if not ok_dir or not path.lower().endswith((".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp")):
            raise ValueError("not an allowed cover file")
        if os.path.getsize(path) > 8_000_000:
            raise ValueError("too big")
        with open(path, "rb") as f:
            return f.read()
    if u.scheme in ("http", "https") and u.hostname:
        return _get_pinned(url)
    raise ValueError("unsupported cover URL")


def lyrics(artist, title, album, duration):
    """LRC text, "" when the song has none; raises on a server error (not cached, so it is retried)."""
    os.makedirs(CACHE, exist_ok=True)
    key = hashlib.sha1(f"{artist}|{title}".encode()).hexdigest()[:16]
    f = os.path.join(CACHE, key + ".lrc")
    if os.path.exists(f) and (os.path.getsize(f) > 0 or time.time() - os.path.getmtime(f) < 7 * 86400):
        return open(f, encoding="utf-8").read()
    first = re.split(r",| & | feat\.| ft\.", artist)[0].strip()
    clean = re.sub(r"\s*[(\[].*?[)\]]", "", title)
    clean = re.sub(r"\s+-\s+.*$", "", clean).strip()

    def get(url):
        last = None
        for attempt in range(3):
            if attempt:
                time.sleep(2.5 * attempt)
            try:
                req = urllib.request.Request(url, headers={"User-Agent": AGENT})
                with urllib.request.urlopen(req, timeout=8) as r:
                    return json.loads(r.read())
            except urllib.error.HTTPError as e:
                if e.code == 404:
                    return None
                last = e
            except Exception as e:
                last = e
        raise last or RuntimeError("no answer")

    q = {"artist_name": artist, "track_name": title}
    if album:
        q["album_name"] = album
    if duration > 0:
        q["duration"] = str(duration)
    found = get("https://lrclib.net/api/get?" + urllib.parse.urlencode(q))
    text = (found or {}).get("syncedLyrics") or ""
    if not text:
        hits = get("https://lrclib.net/api/search?" + urllib.parse.urlencode({"track_name": clean, "artist_name": first})) or []
        for h in hits:
            near = duration <= 0 or abs((h.get("duration") or 0) - duration) <= 4
            if h.get("syncedLyrics") and near:
                text = h["syncedLyrics"]
                break
    with open(f, "w", encoding="utf-8") as out:
        out.write(text)
    return text


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        # DNS rebinding: a web page can point its own name at 127.0.0.1; its requests then carry that name as Host
        if self.headers.get("Host", "") not in (f"127.0.0.1:{PORT}", f"localhost:{PORT}"):
            return self.send(403, b"forbidden", "text/plain")
        u = urllib.parse.urlparse(self.path)
        q = urllib.parse.parse_qs(u.query)
        try:
            if u.path in ("/", "/index.html"):
                with open(os.path.join(HERE, "wallpaper.html"), "rb") as f:
                    return self.send(200, f.read(), "text/html; charset=utf-8")
            if u.path == "/state":
                return self.send(200, json.dumps(state()).encode(), "application/json")
            if u.path == "/art":
                state()                                   # refresh what is playing; the "u" in the request is only a cache-buster
                if not CURRENT_ART:
                    return self.send(404, b"no cover", "text/plain")
                return self.send(200, fetch_art(CURRENT_ART), "image/jpeg")
            if u.path == "/lyrics":
                if not LYRICS_ON:
                    return self.send(200, b"", "text/plain")
                g = lambda k: q.get(k, [""])[0]
                now = (state() or {}).get("player") or {}
                if (g("artist"), g("title")) != (now.get("artist"), now.get("title")):
                    return self.send(404, b"not the track that is playing", "text/plain")   # only for what is playing
                text = lyrics(g("artist"), g("title"), g("album"), int(g("duration") or 0))
                return self.send(200, text.encode("utf-8"), "text/plain; charset=utf-8")
        except Exception as e:
            return self.send(502, str(e).encode(), "text/plain")
        self.send(404, b"not found", "text/plain")


def main():
    global LYRICS_ON
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--lyrics", action="store_true", help="look up synced lyrics on lrclib.net (sends artist, title, album)")
    args = ap.parse_args()
    global PORT
    LYRICS_ON = args.lyrics
    PORT = args.port
    srv = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"Coucou wallpaper on http://127.0.0.1:{args.port}/  (lyrics {'on' if LYRICS_ON else 'off'})", flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
