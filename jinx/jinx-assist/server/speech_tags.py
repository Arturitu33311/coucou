#!/usr/bin/env python3
"""speech_tags — ElevenLabs v4 audio tags travel INSIDE a notice and are removed from every written copy.

Jinx writes a notice once, with tags ("[warmly] Tienes lab en 15 minutos"). The funnels that show or send text
(mochi_route, notify_alan.sh, jinx_send) call `split()`/`strip_tags()` so no screen or chat ever shows "[warmly]";
the tagged version is kept apart as `speech` for the voice path.

A tag is lowercase, English, in single ASCII brackets: [warmly] [short pause] [sighs, amused]. That is narrow on
purpose: it never touches markers like [BUSCAR] / [ALAN: …], markdown links [text](url) or [[internal]] markers.

CLI (for bash):  speech_tags.py strip   <stdin → stdout, tags removed>
"""
from __future__ import annotations

import re
import sys

# 【warmly】 (full-width) is what some models emit; v4 only reads ASCII brackets.
_FULLWIDTH = str.maketrans({"【": "[", "】": "]"})
TAG_RE = re.compile(r"(?<!\[)\[[a-z][a-z ,'\-]{1,40}\](?![\(\]])")
DEFAULT_TAG = {"alert": "[urgently]", "warn": "[concerned]", "info": "[warmly]"}


def normalize(text: str) -> str:
    return text.translate(_FULLWIDTH)


def has_tags(text: str) -> bool:
    return bool(TAG_RE.search(normalize(text)))


def strip_tags(text: str) -> str:
    """The written copy: tags gone, spaces tidy, line breaks kept."""
    t = TAG_RE.sub("", normalize(text))
    t = re.sub(r"[ \t]{2,}", " ", t)
    t = re.sub(r" +([,.;:!?…])", r"\1", t)
    return "\n".join(line.strip() for line in t.split("\n")).strip()


def ensure_tag(text: str, kind: str = "info") -> str:
    """The spoken copy always has at least one tag (an untagged notice gets the default one for its kind)."""
    t = normalize(text).strip()
    return t if TAG_RE.search(t) else f"{DEFAULT_TAG.get(kind, DEFAULT_TAG['info'])} {t}"


def split(text: str, kind: str = "info") -> tuple[str, str | None]:
    """(written text, tagged speech or None). `speech` is only returned when the notice really carried tags:
    an untagged one gets its default tag later, at the voice path, so a plain notice is not duplicated."""
    written = strip_tags(text)
    return written, (normalize(text).strip() if has_tags(text) else None)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "strip":
        sys.stdout.write(strip_tags(sys.stdin.read()))
    else:
        print(__doc__)
        sys.exit(2)
