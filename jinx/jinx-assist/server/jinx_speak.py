#!/usr/bin/env python3
"""jinx_speak — synthesizes a notice that ALREADY carries ElevenLabs v4 tags, with Jinx's voice.

Runs ON THE SERVER (~/.hermes/scripts/jinx_speak.py, Hermes venv python), called over ssh by the laptop's
`jinx-voice-alert`. The ElevenLabs key never leaves the server's vault.

Jinx writes the notice once, tags included (producers add TAG_GUIDE to their instruction); the written copies
lose the tags (speech_tags.strip_tags) and this script speaks the tagged one. An untagged notice gets the default
tag of its kind.

stdin : one JSON line  {"text": "<tagged or plain>", "kind": "info|warn|alert"}
stdout: one JSON line  {"ok": true, "line": "<tagged line spoken>", "audio_b64": "...", "model": "eleven_v4",
                        "voice_settings": true|false}
                       or {"ok": false, "error": "..."}

Synthesis chain: eleven_v4 with the settings the Telegram plugin uses -> eleven_v4 without them (if the API refuses
the settings) -> eleven_flash_v2_5 with the tags stripped (flash would read them aloud).
"""
from __future__ import annotations

import base64
import json
import re
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import speech_tags  # noqa: E402

VOICE_ID = "smfxcYtxs2hlvFrxbKIg"            # Jinx's voice (same as the plugin and OK Coucou)
MODEL_V4 = "eleven_v4"
MODEL_FLASH = "eleven_flash_v2_5"
V4_SETTINGS = {"stability": 0.0, "style": 0.35, "similarity_boost": 0.8}   # as the elevenlabs-emotion plugin
SECRETS_BIN = "/home/alan/.hermes/hermes-agent/venv/bin/hermes-secrets"
MAX_WORDS = 25

# For producers (voz_jinx.redactar(tags=True)): appended to the instruction so the notice is written with tags.
TAG_GUIDE = """Este aviso también se dirá en voz alta con Eleven v4, que lee las etiquetas entre corchetes como dirección de actuación.
- Escribe 1 o 2 etiquetas en inglés y minúscula, con corchetes ASCII [así] (nunca 【así】), pegadas justo antes de lo que colorean.
- Describen CÓMO se dice, no lo que haces: [warmly] [softly] [concerned] [urgently] [tired] [sarcastic] [curious]
  [sighs] [short pause] [quietly]. Nombra una cualidad concreta; no mezcles emociones opuestas.
- El aviso hablado cabe en unas 25 palabras. El texto va en español; horas, cifras y nombres, tal cual."""

SPEECH_NOISE = re.compile(r"[\U0001F000-\U0001FFFF☀-➿️]")


def prepare(text: str, kind: str) -> str:
    """The tagged line to speak: tag guaranteed, emoji/URLs/markdown out, at most MAX_WORDS spoken words."""
    t = speech_tags.ensure_tag(text, kind)
    t = re.sub(r"^\s*(\[[^\]]*\]\s*)?🕓[^·]*·\s*", r"\1", t)      # '🕓 3 h ·' age prefix
    t = re.sub(r"https?://\S+", "", t)
    t = SPEECH_NOISE.sub("", re.sub(r"[*_`#>]+", "", t))
    t = re.sub(r"\s+", " ", t).strip()
    return cap_words(t)


def spoken_words(text: str) -> int:
    return len(speech_tags.strip_tags(text).split())


def cap_words(text: str, limit: int = MAX_WORDS) -> str:
    """At most `limit` spoken words, cutting at a sentence end when there is one (tags do not count)."""
    if spoken_words(text) <= limit:
        return text
    out = ""
    for s in re.split(r"(?<=[.!?…])\s+", text):
        if spoken_words((out + " " + s).strip()) > limit:
            break
        out = (out + " " + s).strip()
    if out:
        return out
    return " ".join(text.split()[: limit + 2]).rstrip(",;:") + "…"


def _post(key: str, text: str, model: str, settings: dict | None) -> bytes:
    body = {"text": text, "model_id": model}
    if settings:
        body["voice_settings"] = settings
    req = urllib.request.Request(
        f"https://api.elevenlabs.io/v1/text-to-speech/{VOICE_ID}?output_format=mp3_44100_128",
        data=json.dumps(body).encode(), method="POST",
        headers={"xi-api-key": key, "Content-Type": "application/json", "accept": "audio/mpeg"})
    with urllib.request.urlopen(req, timeout=45) as r:
        data = r.read()
    if len(data) < 1024:
        raise ValueError(f"audio too small ({len(data)} B)")
    return data


def synthesize(line: str, key: str, post=_post) -> tuple[bytes, str, bool]:
    """(mp3, model used, voice_settings sent)."""
    attempts = [(MODEL_V4, V4_SETTINGS, line), (MODEL_V4, None, line),
                (MODEL_FLASH, None, speech_tags.strip_tags(line))]
    for model, settings, text in attempts:
        try:
            return post(key, text, model, settings), model, settings is not None
        except urllib.error.HTTPError as e:
            # Only "the API did not accept this request shape" (settings/model) merits another shape; a bad key,
            # no credits or a rate limit would fail the same way: stop.
            if e.code not in (400, 404, 422):
                raise
    raise RuntimeError("ElevenLabs refused every variant of the request")


def _key() -> str:
    out = subprocess.run([SECRETS_BIN, "get", "ELEVENLABS_API_KEY"], capture_output=True, text=True, timeout=10)
    return out.stdout.strip()


def main() -> int:
    try:
        req = json.loads(sys.stdin.readline())
        kind = req.get("kind") if req.get("kind") in speech_tags.DEFAULT_TAG else "info"
        line = prepare(str(req.get("text", "")), kind)
        if not speech_tags.strip_tags(line):
            raise ValueError("empty text")
        key = _key()
        if not key:
            raise RuntimeError("no ElevenLabs key in the vault")
        mp3, model, with_settings = synthesize(line, key)
        print(json.dumps({"ok": True, "line": line, "audio_b64": base64.b64encode(mp3).decode(), "model": model,
                          "voice_settings": with_settings}))
        return 0
    except Exception as e:  # noqa: BLE001
        print(json.dumps({"ok": False, "error": str(e)[:200]}))
        return 1


if __name__ == "__main__":
    sys.exit(main())
