"""g2_stt — voz de las gafas a texto. Las G2 no traen reconocimiento de voz: mandan PCM crudo (16 kHz, mono, s16le).

Dos motores, el primero que haya:
  1. JINX_G2_STT_CMD: un comando que lee un WAV y escribe el texto por stdout; `{wav}` es la ruta del WAV
     (p. ej. `whisper-cli -m ~/models/ggml-small.bin -l es -nt -f {wav}`).
  2. faster-whisper, si está instalado en el servidor (`pip install faster-whisper`; modelo JINX_G2_STT_MODEL, "small").
Sin ninguno lanza SttUnavailable con el motivo (no inventa texto). Sin dependencias fuera de la biblioteca estándar.
"""
import os
import shlex
import subprocess
import tempfile
import wave

RATE = 16000
MAX_SECONDS = 60
MAX_BYTES = RATE * 2 * MAX_SECONDS
MIN_BYTES = RATE * 2 // 4  # menos de un cuarto de segundo no es una frase


class SttUnavailable(Exception):
    pass


class SttBadAudio(Exception):
    pass


def check_pcm(pcm):
    if not isinstance(pcm, (bytes, bytearray)) or len(pcm) < MIN_BYTES:
        raise SttBadAudio("audio demasiado corto")
    if len(pcm) > MAX_BYTES:
        raise SttBadAudio("audio demasiado largo (máx. %d s)" % MAX_SECONDS)
    if len(pcm) % 2:
        raise SttBadAudio("PCM s16le con un número impar de bytes")


def write_wav(path, pcm):
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(bytes(pcm))


def _clean(text):
    return " ".join((text or "").split())


def transcribe(pcm, lang="es", env=None, run=subprocess.run, whisper_loader=None):
    """Texto de un PCM 16 kHz mono s16le. Lanza SttBadAudio / SttUnavailable; texto vacío = no se entendió nada."""
    env = os.environ if env is None else env
    check_pcm(pcm)
    cmd = env.get("JINX_G2_STT_CMD", "").strip()
    with tempfile.TemporaryDirectory() as d:
        wav = os.path.join(d, "voz.wav")
        write_wav(wav, pcm)
        if cmd:
            argv = [a.replace("{wav}", wav) for a in shlex.split(cmd)]
            try:
                r = run(argv, capture_output=True, text=True, timeout=90)
            except (OSError, subprocess.TimeoutExpired) as e:
                raise SttUnavailable("el comando de voz falló: %s" % type(e).__name__)
            if r.returncode != 0:
                raise SttUnavailable("el comando de voz salió con %d" % r.returncode)
            return _clean(r.stdout)
        model = (whisper_loader or _load_faster_whisper)(env.get("JINX_G2_STT_MODEL", "small"))
        segments, _info = model.transcribe(wav, language=lang or None, vad_filter=True)
        return _clean(" ".join(s.text for s in segments))


_MODEL = {}


def _load_faster_whisper(name):
    if name in _MODEL:
        return _MODEL[name]
    try:
        from faster_whisper import WhisperModel  # type: ignore
    except ImportError:
        raise SttUnavailable("no hay motor de voz: instala faster-whisper o pon JINX_G2_STT_CMD")
    _MODEL[name] = WhisperModel(name, device="cpu", compute_type="int8")
    return _MODEL[name]
