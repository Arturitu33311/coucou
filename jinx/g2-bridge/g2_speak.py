"""g2_speak — Jinx's voice for the phones: text in, MP3 out, with the key staying in the server's vault.

Used by g2_bridge's POST /api/g2/speak. It reuses jinx_speak (the laptop's `jinx-voice-alert` calls the same code
over ssh): ElevenLabs v4 with the Jinx voice, then v4 without settings, then flash with the tags stripped.
A reply read by the phone is longer than a notice, so it keeps more words (REPLY_WORDS) than jinx_speak's 25.

A small budget keeps a loop or a leaked key from spending the ElevenLabs credit: at most MAX_PER_HOUR calls an hour.
"""
import json
import os
import re
import sys
import threading
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
for p in (os.path.join(os.path.expanduser("~"), ".hermes", "scripts"), os.path.join(HERE, "..", "jinx-assist", "server")):
    if os.path.isdir(p) and p not in sys.path:
        sys.path.append(p)

REPLY_WORDS = 60
MAX_PER_HOUR = 60
# Streaming: raw 16-bit mono PCM at 24 kHz (what the phone writes straight into an AudioTrack, no decoder).
PCM_FORMAT = "pcm_24000"
PCM_RATE = 24000
CHUNK = 4096


class SpeakError(Exception):
    pass


class Budget:
    def __init__(self, per_hour=MAX_PER_HOUR, now=time.time):
        self.per_hour, self.now, self.hits, self.lock = per_hour, now, [], threading.Lock()

    def take(self):
        with self.lock:
            t = self.now()
            self.hits = [h for h in self.hits if t - h < 3600]
            if len(self.hits) >= self.per_hour:
                return False
            self.hits.append(t)
            return True


def prepare(js, text, kind, words=REPLY_WORDS):
    """The tagged line to speak (jinx_speak.prepare with a longer cap for conversation)."""
    t = js.speech_tags.ensure_tag(text, kind)
    t = re.sub(r"https?://\S+", "", t)
    t = js.SPEECH_NOISE.sub("", re.sub(r"[*_`#>]+", "", t))
    t = re.sub(r"\s+", " ", t).strip()
    return js.cap_words(t, words)


def speak(text, kind="info", js=None, key=None):
    """(mp3 bytes, model). Raises SpeakError with a short reason."""
    if js is None:
        try:
            import jinx_speak as js  # noqa: PLC0415 - deployed next to the vault scripts
        except ImportError as e:
            raise SpeakError("jinx_speak is not installed on the server") from e
    kind = kind if kind in js.speech_tags.DEFAULT_TAG else "info"
    line = prepare(js, text, kind)
    if not js.speech_tags.strip_tags(line):
        raise SpeakError("empty text")
    key = key or js._key()
    if not key:
        raise SpeakError("no ElevenLabs key in the vault")
    try:
        mp3, _model, _settings = js.synthesize(line, key)
    except Exception as e:  # noqa: BLE001
        raise SpeakError(str(e)[:120]) from e
    return mp3


def stream(text, kind="info", js=None, key=None, opener=urllib.request.urlopen):
    """Generator of PCM chunks (16-bit mono 24 kHz) as ElevenLabs produces them, like « OK Coucou » does on the laptop.

    Raises SpeakError before the first chunk when nothing can be had, so the caller can still answer with an error;
    once audio has started there is no taking it back. Same chain as jinx_speak.synthesize: v4 with the settings,
    v4 without them, flash with the tags stripped (only a refusal of the request's shape moves to the next one).
    """
    if js is None:
        try:
            import jinx_speak as js  # noqa: PLC0415
        except ImportError as e:
            raise SpeakError("jinx_speak is not installed on the server") from e
    kind = kind if kind in js.speech_tags.DEFAULT_TAG else "info"
    line = prepare(js, text, kind)
    if not js.speech_tags.strip_tags(line):
        raise SpeakError("empty text")
    key = key or js._key()
    if not key:
        raise SpeakError("no ElevenLabs key in the vault")
    attempts = [(js.MODEL_V4, js.V4_SETTINGS, line), (js.MODEL_V4, None, line),
                (js.MODEL_FLASH, None, js.speech_tags.strip_tags(line))]
    for model, settings, t in attempts:
        body = {"text": t, "model_id": model}
        if settings:
            body["voice_settings"] = settings
        req = urllib.request.Request(
            f"https://api.elevenlabs.io/v1/text-to-speech/{js.VOICE_ID}/stream?output_format={PCM_FORMAT}",
            data=json.dumps(body).encode(), method="POST",
            headers={"xi-api-key": key, "Content-Type": "application/json"})
        try:
            r = opener(req, timeout=45)
        except urllib.error.HTTPError as e:
            if e.code not in (400, 404, 422):
                raise SpeakError(f"ElevenLabs {e.code}") from e
            continue
        except Exception as e:  # noqa: BLE001
            raise SpeakError(str(e)[:120]) from e
        try:
            first = r.read(CHUNK)
            if not first:
                continue
            yield first
            while True:
                chunk = r.read(CHUNK)
                if not chunk:
                    break
                yield chunk
        finally:
            r.close()
        return
    raise SpeakError("ElevenLabs refused every variant of the request")
