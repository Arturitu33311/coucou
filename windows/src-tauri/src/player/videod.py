# The desktop player's video resolver: one long-lived process (`python3 -I -c <this>`), one JSON line per request
# on stdin and one per answer on stdout, so yt-dlp and ytmusicapi are imported once and every answer is cached.
#
#   → {"id": 7, "title": "Houdini", "artists": ["Dua Lipa"], "length": 186}
#   ← {"id": 7, "video": {"id": "suAR1PYFNYA", "url": "https://…", "offset": 0.0, "measured": false}}
#     (or "video": null, or "error")
#   → {"id": 8, "op": "refine", "title": …, "artists": […], "length": 186, "video": "suAR1PYFNYA"}
#   ← {"id": 8, "offset": 2.74}   (a "measured": false pick's real offset, null when it does not line up)
#
# "video" is the song's official music video (never a lyric/live/remix cut) and the direct URL of its ≤720p
# picture stream, which mpv plays without running yt-dlp again. `offset` is where the song starts inside the
# video, in seconds: 0 for a video as long as the song; for a longer one (an intro of its own) it is measured by
# lining up the two audio tracks, and a video that does not line up is not used.
import array, json, math, operator, os, re, socket, subprocess, sys, threading, time, unicodedata
from concurrent.futures import ThreadPoolExecutor

_getaddrinfo = socket.getaddrinfo
socket.getaddrinfo = lambda *a, **k: sorted(_getaddrinfo(*a, **k), key=lambda i: i[0] != socket.AF_INET)

HOME = os.path.expanduser("~")
YTDLP_ZIP = os.path.join(HOME, ".local/bin/yt-dlp")
YTDLP_CONF = os.path.join(HOME, ".config/coucou/yt-dlp.conf")
CACHE_FILE = os.path.join(os.environ.get("XDG_CACHE_HOME") or os.path.join(HOME, ".cache"), "coucou", "videos.json")
VIDEO_FORMAT = "bestvideo[height<=?720][vcodec^=avc1]/bestvideo[height<=?720]/best[height<=?720]"

# Words that mark another cut of the song: lyric videos (the page already shows the lyrics), live takes, remixes,
# "official audio" stills... A word in the song's own title (a "Remix" the user plays) does not count.
NOT_THE_VIDEO = [
    "lyric", "lyrics", "letra", "letras", "karaoke", "visualizer", "visualiser", "live", "en vivo", "en directo",
    "tour", "concert", "concierto", "cover", "remix", "acapella", "a cappella", "8d", "slowed", "reverb", "sped up",
    "nightcore", "extended", "loop", "hour", "instrumental", "tutorial", "reaction", "mashup", "traducida",
    "subtitulado", "audio", "version", "mix", "edit", "fanmade", "amv", "clean", "bass boosted", "parody", "parodia",
    "teaser", "trailer", "behind the scenes", "performance", "session",
]
SAME_LENGTH_S = 5      # this close to the song's length: same timeline, offset 0
MAX_INTRO_S = 120      # a longer official video is lined up by its audio, up to this much extra
MIN_CORR = 0.40        # the alignment's band correlation must reach this (true pairs 0.59-0.99, other songs <= 0.25)
MIN_MARGIN = 0.18      # ... and beat the best start a second or more away by this much (true >= 0.25, others <= 0.14)
URL_MARGIN_S = 600     # a stream URL this close to its expiry is resolved again

# ── matching ──────────────────────────────────────────────────────────────────


def norm(s):
    s = unicodedata.normalize("NFKD", s or "").encode("ascii", "ignore").decode().lower()
    return re.sub(r"[^a-z0-9]+", " ", s).strip()


def has_words(words, text):
    return bool(words) and re.search(r"(^| )" + re.escape(words) + r"( |$)", text) is not None


def candidates(results, title, artists, length_s):
    """Official music videos of the song, closest in length first, each with its extra length (s, ≥ -5)."""
    bare = norm(re.sub(r"\(.*?\)|\[.*?\]| - .*$", "", title or "")) or norm(title)
    own = norm(title)
    wanted = {norm(a) for a in artists if norm(a)}
    out = []
    for r in results:
        rt = norm(r.get("title"))
        if not r.get("videoId") or r.get("videoType") != "MUSIC_VIDEO_TYPE_OMV" or not has_words(bare, rt):
            continue
        if any(has_words(w, rt) and not has_words(w, own) for w in NOT_THE_VIDEO):
            continue
        if not ({norm(a.get("name")) for a in r.get("artists") or []} & wanted):
            continue
        d = r.get("duration_seconds") or 0
        extra = d - length_s if (length_s and d) else 0
        if length_s and (not d or extra < -SAME_LENGTH_S or extra > MAX_INTRO_S):
            continue
        out.append((abs(extra), extra, r))
    out.sort(key=lambda c: c[0])
    return [(extra, r) for _, extra, r in out]


# ── audio alignment ───────────────────────────────────────────────────────────
#
# Two recordings of one song (the music-video audio and the song's own audio) are lined up on their loudness
# contour in a few frequency bands: ffmpeg splits the audio into BANDS and hands back one s16 channel per band,
# each band's log energy is taken every 10 ms and its slow trend (a 2 s moving average) removed. The two feature
# sets are compared with a Pearson correlation per band, averaged over the bands: coarse at 20 frames/s over every
# possible start, then the best few candidates at 100 frames/s. The correlation of the winner is the confidence
# (0.9-1.0 for the same master, 0.6-0.7 for a remixed cut, ≤0.25 for another song), and the winner must also stand
# clear of the best candidate a second or more away (repetitive music correlates with itself).

RATE, HOP = 8000, 80  # decoded audio at 8 kHz; one feature frame every 10 ms
FPS = RATE // HOP
BANDS = ((100, 400), (400, 1000), (1000, 2000), (2000, 3800))
NB = len(BANDS)
TREND_FRAMES = 201     # moving-average length removed from each band's log energy
COARSE = 5             # first pass at FPS / COARSE frames/s
CANDIDATES = 4         # coarse peaks refined at full rate
MIN_OVERLAP = 0.75     # a start that leaves less than this share of the song inside the video is not tried
try:
    _dot = math.sumprod
except AttributeError:  # Python < 3.12
    _dot = lambda a, b: sum(map(operator.mul, a, b))

# (amerge refuses NB inputs that are all mono, join places each band on its own channel)
_FILTER = ("[0:a]aformat=sample_fmts=fltp:channel_layouts=mono,aresample=%d,asplit=%d%s;%s;%sjoin=inputs=%d:"
           "channel_layout=4.0:map=%s,aformat=sample_fmts=s16" % (
    RATE, NB, "".join(f"[a{i}]" for i in range(NB)),
    ";".join(f"[a{i}]highpass=f={lo},lowpass=f={hi}[b{i}]" for i, (lo, hi) in enumerate(BANDS)),
    "".join(f"[b{i}]" for i in range(NB)), NB, "|".join(f"{i}.0-{c}" for i, c in enumerate(("FL", "FR", "FC", "BC")))))


def decode(url, seconds, headers=None):
    """The first `seconds` of `url` as NB band-passed s16 channels (interleaved); raises when there is no audio."""
    hdr = "".join(f"{k}: {v}\r\n" for k, v in (headers or {}).items())
    cmd = ["ffmpeg", "-v", "error", "-nostdin", "-reconnect", "1", "-reconnect_streamed", "1",
           "-reconnect_delay_max", "2"] + (["-headers", hdr] if hdr else []) + [
        "-t", str(seconds), "-i", url, "-vn", "-filter_complex", _FILTER, "-f", "s16le", "-"]
    p = subprocess.run(cmd, capture_output=True, timeout=120)
    if len(p.stdout) < RATE * NB * 2 * 2:  # not even two seconds: ffmpeg says why (a 403 on an expired/refused URL...)
        raise RuntimeError("no audio: " + (p.stderr.decode("utf-8", "replace").strip()[-160:] or f"ffmpeg exit {p.returncode}"))
    return p.stdout


def _detrend(e, w):
    """`e` minus its centred moving average of w (odd) samples, the average over what exists at the ends."""
    n, h = len(e), w // 2
    pre = [0.0]
    for x in e:
        pre.append(pre[-1] + x)
    if n <= w:
        mean = pre[-1] / n if n else 0.0
        return [x - mean for x in e]
    mid = [(b - a) / w for a, b in zip(pre, pre[w:])]
    head = [pre[i + h + 1] / (i + h + 1) for i in range(h)]
    tail = [(pre[n] - pre[n - h - j]) / (h + j) for j in range(h, 0, -1)]
    return [x - m for x, m in zip(e, head + mid + tail)]


def features(raw):
    """Per band: log energy every 10 ms with its slow trend removed (a list of NB lists)."""
    pcm = array.array("h")
    pcm.frombytes(raw[: len(raw) // (2 * NB) * (2 * NB)])
    if sys.byteorder == "big":
        pcm.byteswap()
    out = []
    for b in range(NB):
        ch, e = pcm[b::NB], []
        for i in range(0, len(ch) - HOP + 1, HOP):
            f = ch[i:i + HOP]
            e.append(math.log(math.sqrt(_dot(f, f) / HOP) + 1.0))
        out.append(_detrend(e, TREND_FRAMES))
    return out


def envelope(url, seconds, headers=None):
    return features(decode(url, seconds, headers))


class _Bands:
    """The feature set with prefix sums, so a window's mean and variance cost nothing per lag."""

    def __init__(self, bands):
        self.x = [memoryview(array.array("d", b)) for b in bands]
        self.n = len(bands[0]) if bands else 0
        self.p1, self.p2 = [], []
        for b in bands:
            s1, s2, a, c = [0.0], [0.0], 0.0, 0.0
            for v in b:
                a += v
                c += v * v
                s1.append(a)
                s2.append(c)
            self.p1.append(s1)
            self.p2.append(s2)

    def coarse(self, k):
        return _Bands([[sum(b[i:i + k]) / k for i in range(0, len(b) - k + 1, k)] for b in (list(m) for m in self.x)])


def _ncc(v, s, lag):
    """Mean over the bands of the Pearson correlation of s[max(0,-lag):] against v[max(0,lag):], or -1."""
    s0, v0 = max(0, -lag), max(0, lag)
    n = min(s.n - s0, v.n - v0)
    if n < 2 or n < MIN_OVERLAP * s.n:
        return -1.0
    tot = 0.0
    for b in range(NB):
        sv, vv = v.p1[b][v0 + n] - v.p1[b][v0], v.p2[b][v0 + n] - v.p2[b][v0]
        ss, sq = s.p1[b][s0 + n] - s.p1[b][s0], s.p2[b][s0 + n] - s.p2[b][s0]
        cov = _dot(v.x[b][v0:v0 + n], s.x[b][s0:s0 + n]) - sv * ss / n
        den = math.sqrt(max(vv - sv * sv / n, 1e-9) * max(sq - ss * ss / n, 1e-9))
        tot += cov / den
    return tot / NB


def measure_lag(video_env, song_env, extra_s):
    """(start of the song in the video in s, correlation, margin over the best other candidate), or None."""
    v, s = _Bands(video_env), _Bands(song_env)
    if not v.n or not s.n:
        return None
    vc, sc = v.coarse(COARSE), s.coarse(COARSE)
    cps = FPS // COARSE
    lags = range(-2 * cps, int((extra_s + 8) * cps) + 1)
    curve = [(_ncc(vc, sc, l), l) for l in lags]
    curve = [c for c in curve if c[0] > -1.0]
    if not curve:
        return None
    peaks = []
    for val, l in sorted(curve, reverse=True):
        if all(abs(l - p) > cps for p in peaks):  # one candidate per second
            peaks.append(l)
        if len(peaks) == CANDIDATES:
            break
    found = []
    for l in peaks:
        c = l * COARSE
        found.append(max((_ncc(v, s, f), f) for f in range(c - COARSE - 2, c + COARSE + 3)))
    found.sort(reverse=True)
    best, lag = found[0]
    other = found[1][0] if len(found) > 1 else 0.0
    return lag / FPS, best, best - other


def align(video_env, song_env, extra_s):
    """Where the song starts in the video (s), or None when the two do not line up."""
    r = measure_lag(video_env, song_env, extra_s)
    if r and r[1] >= MIN_CORR and r[2] >= MIN_MARGIN:
        return r[0]
    return None


def selftest():
    """No network: a smooth random feature set, and the same shifted by N frames with noise."""
    import random

    def smooth(rng, n):
        x, out = 0.0, []
        for _ in range(n + 40):
            x = 0.9 * x + rng.gauss(0, 1)
            out.append(x)
        return [sum(out[i:i + 30]) / 30 for i in range(n)]  # ≈ 0.3 s of memory, like a loudness contour

    rng = random.Random(7)
    video = [smooth(rng, 7000) for _ in range(NB)]
    t0 = time.time()
    for shift in (0, 610, 2250):
        song = [[x + rng.gauss(0, 0.1) for x in b[shift:shift + 4000]] for b in video]
        r = measure_lag(video, song, 40)
        assert r and abs(r[0] - shift / FPS) <= 0.011 and align(video, song, 40) is not None, (shift, r)
        print(f"selftest: shifted {shift / FPS:5.2f} s -> {r[0]:5.2f} s  corr {r[1]:.2f} margin {r[2]:.2f}")
    other = [smooth(rng, 4000) for _ in range(NB)]
    r = measure_lag(video, other, 40)
    assert align(video, other, 40) is None, r
    print(f"selftest: unrelated   -> rejected  corr {r[1]:.2f} margin {r[2]:.2f}")
    print(f"selftest: ok ({time.time() - t0:.1f} s)")


# ── yt-dlp / ytmusicapi ───────────────────────────────────────────────────────


class Resolver:
    def __init__(self):
        if os.path.isfile(YTDLP_ZIP):
            sys.path.insert(0, YTDLP_ZIP)
        import yt_dlp
        from ytmusicapi import YTMusic

        self.yt_dlp = yt_dlp
        argv = ["--config-locations", YTDLP_CONF] if os.path.isfile(YTDLP_CONF) else []
        self.opts = dict(yt_dlp.parse_options(argv).ydl_opts, quiet=True, no_warnings=True, skip_download=True,
                         noprogress=True, logger=_Quiet())
        self.ytm = YTMusic()
        self.lock = threading.Lock()
        self.cache = {"picks": {}, "urls": {}}
        try:
            with open(CACHE_FILE) as f:
                self.cache.update(json.load(f))
        except Exception:
            pass

    def save(self):
        with self.lock:
            data = json.dumps(self.cache)
        try:
            os.makedirs(os.path.dirname(CACHE_FILE), exist_ok=True)
            tmp = CACHE_FILE + ".tmp"
            with open(tmp, "w") as f:
                f.write(data)
            os.replace(tmp, CACHE_FILE)
        except Exception:
            pass

    def extract(self, video_id, fmt):
        with self.yt_dlp.YoutubeDL(dict(self.opts, format=fmt)) as y:
            return y.extract_info(f"https://www.youtube.com/watch?v={video_id}", download=False)

    def stream(self, video_id):
        """The picture stream's URL, from the cache while it has not expired."""
        with self.lock:
            hit = self.cache["urls"].get(video_id)
        if hit and hit["expires"] - time.time() > URL_MARGIN_S:
            return hit["url"]
        info = self.extract(video_id, VIDEO_FORMAT)
        url = info.get("url") or ""
        m = re.search(r"[?&]expire=(\d+)", url)
        with self.lock:
            self.cache["urls"][video_id] = {"url": url, "expires": int(m.group(1)) if m else time.time() + 3600}
        return url

    def audio(self, video_id):
        info = self.extract(video_id, "bestaudio/best")
        return info.get("url"), info.get("http_headers")

    def song_id(self, title, artists):
        for r in self.ytm.search(" ".join([artists[0] if artists else "", title]), filter="songs", limit=5):
            rt = norm(r.get("title"))
            if r.get("videoId") and (rt == norm(title) or has_words(norm(title), rt)):
                return r.get("videoId"), r.get("duration_seconds") or 0
        return None, 0

    def pick(self, title, artists, length):
        key = "\u001f".join([norm(title), ",".join(sorted(norm(a) for a in artists)), str(round(length or 0))])
        with self.lock:
            if key in self.cache["picks"]:
                return self.cache["picks"][key]
        results = self.ytm.search(" ".join([artists[0] if artists else "", title]), filter="videos", limit=20)
        found = None
        cands = candidates(results, title, artists, length)
        # about the same length: shown at once from the song's own time; `refine` measures the few seconds
        # it may still be off (a short intro) while it plays
        same = [r for extra, r in cands if abs(extra) <= SAME_LENGTH_S]
        if same:
            found = {"id": same[0]["videoId"], "offset": 0.0, "measured": False}
        else:
            longer = [(extra, r) for extra, r in cands if extra > SAME_LENGTH_S][:2]
            if longer:
                sid, _ = self.song_id(title, artists)
                if sid:
                    try:
                        found = self.measure(sid, longer)
                    except Exception as e:  # a failed download: no answer now, but not remembered either
                        print(json.dumps({"log": f"align {title}: {e}"}), file=sys.stderr, flush=True)
                        return None
        with self.lock:
            self.cache["picks"][key] = found
        return found

    def audio_features(self, video_id, seconds):
        """Band features of the first `seconds` of the video's audio. YouTube hands out the odd stream URL that
        answers 403 (ffmpeg then writes nothing), so a failed download is resolved again, up to three times."""
        err = None
        for _ in range(3):
            try:
                url, hdr = self.audio(video_id)
                if url:
                    return envelope(url, seconds, hdr)
                err = RuntimeError("no audio url")
            except Exception as e:
                err = e
        raise err

    def measure(self, song_id, longer):
        """The first longer official video whose audio lines up with the song's: its id and offset.
        Raises when a download failed and nothing matched, so a network hiccup is not cached as "no video"."""
        failed = None
        with ThreadPoolExecutor(1 + len(longer)) as pool:  # every resolve and download in parallel
            song_f = pool.submit(self.audio_features, song_id, 40)
            vids = [(extra, r, pool.submit(self.audio_features, r["videoId"], int(min(extra, MAX_INTRO_S) + 50)))
                    for extra, r in longer]
            song_env = song_f.result()
            for extra, r, f in vids:
                try:
                    off = align(f.result(), song_env, extra)
                except Exception as e:
                    failed, off = e, None
                if off is not None:
                    return {"id": r["videoId"], "offset": round(max(0.0, off), 2), "measured": True}
        if failed:
            raise failed
        return None

    def refine(self, req):
        """The measured offset of a video about as long as the song (the pick was shown with 0): stored in the
        pick, so the next time it starts right away. None when it does not line up (then 0 stays)."""
        title, artists, vid = req.get("title") or "", req.get("artists") or [], req.get("video") or ""
        key = "\u001f".join([norm(title), ",".join(sorted(norm(a) for a in artists)), str(round(req.get("length") or 0))])
        with self.lock:
            hit = self.cache["picks"].get(key)
        if hit and hit.get("measured"):
            return hit.get("offset")
        sid, _ = self.song_id(title, artists)
        if not sid or not vid:
            return None
        found = self.measure(sid, [(SAME_LENGTH_S, {"videoId": vid})])
        with self.lock:
            entry = self.cache["picks"].get(key)
            if entry and entry.get("id") == vid:
                entry["measured"] = True
                entry["offset"] = found["offset"] if found else 0.0
        return found["offset"] if found else None

    def answer(self, req):
        found = self.pick(req.get("title") or "", req.get("artists") or [], req.get("length") or 0)
        if not found:
            return None
        return dict(found, url=self.stream(found["id"]))


class _Quiet:
    def debug(self, msg):
        pass

    def warning(self, msg):
        pass

    def error(self, msg):
        print(json.dumps({"log": str(msg)[:300]}), file=sys.stderr, flush=True)


def main():
    out_lock = threading.Lock()

    def reply(obj):
        with out_lock:
            sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
            sys.stdout.flush()

    try:
        r = Resolver()
    except Exception as e:
        reply({"id": 0, "error": f"resolver: {e}"})
        return
    reply({"id": 0, "ready": True})
    pool = ThreadPoolExecutor(3)

    def handle(req):
        try:
            if req.get("op") == "refine":
                reply({"id": req.get("id"), "offset": r.refine(req)})
            else:
                reply({"id": req.get("id"), "video": r.answer(req)})
            r.save()
        except Exception as e:
            reply({"id": req.get("id"), "error": str(e) or e.__class__.__name__})

    for line in sys.stdin:
        try:
            req = json.loads(line)
        except ValueError:
            continue
        pool.submit(handle, req)


if __name__ == "__main__":
    selftest() if "--selftest" in sys.argv[1:] else main()
