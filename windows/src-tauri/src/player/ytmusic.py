# Glass player helper: `python3 -I - <op> <arg>`. Prints a JSON list of items, or {"error": "..."}.
import json, socket, sys

# Try IPv4 addresses first: on a network whose IPv6 route is broken, requests would otherwise wait for
# the IPv6 connection to time out (~25 s) on every call. IPv6 stays as the fallback.
_getaddrinfo = socket.getaddrinfo


def _ipv4_first(*args, **kwargs):
    return sorted(_getaddrinfo(*args, **kwargs), key=lambda info: info[0] != socket.AF_INET)


socket.getaddrinfo = _ipv4_first


def art(thumbs):
    thumbs = [t for t in (thumbs or []) if t.get("url")]
    ok = sorted((t for t in thumbs if t.get("width", 0) >= 60), key=lambda t: t["width"])
    return (ok[0] if ok else thumbs[-1])["url"] if thumbs else ""


def names(xs):
    return ", ".join(a["name"] for a in (xs or []) if a.get("name"))


def item(kind, id_, title, sub, thumbs):
    if not id_ or not title:
        return None
    return {"source": "ytmusic", "kind": kind, "id": id_, "title": title, "subtitle": sub or "", "artUrl": art(thumbs)}


def song(s, fallback_thumbs=None):
    return item("track", s.get("videoId"), s.get("title"), names(s.get("artists")), s.get("thumbnails") or fallback_thumbs)


def main(op, arg):
    from ytmusicapi import YTMusic

    yt = YTMusic()
    out = []
    if op == "search":
        for r in yt.search(arg, filter="songs"):
            out.append(song(r))
        for r in yt.search(arg, filter="artists"):
            out.append(item("artist", r.get("browseId"), r.get("artist"), "", r.get("thumbnails")))
    elif op == "artist":
        a = yt.get_artist(arg)
        for s in (a.get("songs") or {}).get("results") or []:
            out.append(song(s))
        for b in (a.get("albums") or {}).get("results") or []:
            out.append(item("album", b.get("browseId"), b.get("title"), b.get("year", ""), b.get("thumbnails")))
    elif op == "album":
        a = yt.get_album(arg)
        for s in a.get("tracks") or []:
            out.append(song(s, a.get("thumbnails")))
    elif op == "video":
        # The music video for "artist title": the first video result (official videos come first).
        for r in yt.search(arg, filter="videos", limit=5):
            if r.get("videoId"):
                out.append(item("track", r.get("videoId"), r.get("title"), names(r.get("artists")), r.get("thumbnails")))
                break
    elif op == "radio":
        # The songs YouTube Music plays after this one (its "up next" radio), without the song itself.
        wp = yt.get_watch_playlist(videoId=arg, limit=25)
        for s in wp.get("tracks") or []:
            if s.get("videoId") != arg:
                out.append(song(s, s.get("thumbnail")))
    else:
        raise ValueError("unknown op: " + op)
    print(json.dumps([i for i in out if i], ensure_ascii=False))


try:
    main(sys.argv[1], sys.argv[2])
except Exception as e:
    print(json.dumps({"error": str(e) or e.__class__.__name__}))
