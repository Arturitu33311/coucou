/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.audiolink

/**
 * What travels between the laptop and this phone for the audio relay, with no I/O. The laptop's twin is
 * `audiolink_wire.rs`; both read `audiolink-vectors.json`.
 *
 * Laptop → phone: frames `[type: u8][len: u16 big-endian][payload]` (1 audio, 2 heartbeat, 3 hello).
 * Phone → laptop: one JSON object per line.
 */
enum class FrameKind(val id: Int) { Audio(1), Heartbeat(2), Hello(3), PausePhone(4), SwapPrompt(5) }

class Frame(val kind: FrameKind, val payload: ByteArray)

sealed interface Decoded {
    data class Got(val frame: Frame, val consumed: Int) : Decoded
    data object NeedMore : Decoded
    data class Bad(val why: String) : Decoded
}

/** What the laptop asks the person to do about the headphones. */
enum class SwapAsk { Release, Connect }

/** The payload of a `SwapPrompt` frame: 1 is "let go of the headphones", 2 is "connect them here"; anything else is nothing. */
fun swapAskOf(payload: ByteArray): SwapAsk? = when {
    payload.size != 1 -> null
    payload[0].toInt() == 1 -> SwapAsk.Release
    payload[0].toInt() == 2 -> SwapAsk.Connect
    else -> null
}

/** What the phone is playing, as it goes to the laptop. */
data class NowPlaying(
    val title: String,
    val artist: String,
    val album: String,
    /** "Playing", "Paused" or "Stopped". */
    val status: String,
    val positionMs: Long,
    val lengthMs: Long,
)

/**
 * The now-playing lines (laptop twin: `audiolink_wire.rs`). `op` is always the first key: the laptop tells how long a
 * line may be from its first bytes.
 */
object NowWire {
    const val MAX_TEXT_BYTES = 200
    const val MAX_ART = 65536

    /** The laptop says in its hello that it reads `now` and `art` lines; an older one would drop the link at them. */
    fun helloAllowsNow(hello: ByteArray): Boolean = String(hello, Charsets.UTF_8).replace(" ", "").contains("\"now\":1")

    /** Control characters become spaces and the text is cut to `MAX_TEXT_BYTES` of UTF-8 without splitting a character. */
    fun clean(s: String): String {
        val flat = s.map { if (it.code < 0x20 || it.code == 0x7f) ' ' else it }.joinToString("")
        var bytes = 0
        val out = StringBuilder()
        var i = 0
        while (i < flat.length) {
            val cp = flat.codePointAt(i)
            val n = when {
                cp < 0x80 -> 1
                cp < 0x800 -> 2
                cp < 0x10000 -> 3
                else -> 4
            }
            if (bytes + n > MAX_TEXT_BYTES) break
            bytes += n
            out.appendCodePoint(cp)
            i += Character.charCount(cp)
        }
        return out.toString()
    }

    private fun quoted(s: String) = buildString {
        append('"')
        for (c in s) when (c) {
            '"' -> append("\\\"")
            '\\' -> append("\\\\")
            else -> append(c)
        }
        append('"')
    }

    fun nowLine(n: NowPlaying): String =
        """{"op":"now","title":${quoted(clean(n.title))},"artist":${quoted(clean(n.artist))},"album":${quoted(clean(n.album))},""" +
            """"status":"${n.status}","pos":${n.positionMs.coerceAtLeast(0)},"len":${n.lengthMs.coerceAtLeast(0)}}"""

    /** The cover, or no cover at all when `image` is empty. `hash` is letters and digits only. */
    fun artLine(hash: String, image: ByteArray): String {
        require(image.size <= MAX_ART) { "cover too big" }
        require(hash.length <= 64 && hash.all { it.isLetterOrDigit() && it.code < 128 }) { "bad hash" }
        return """{"op":"art","hash":"$hash","b64":"${java.util.Base64.getEncoder().encodeToString(image)}"}"""
    }
}

object AudioLinkWire {
    const val MAX_AUDIO = 8192
    const val MAX_HELLO = 256
    const val MAX_LINE = 512

    /** Reads the first frame in the first `size` bytes of `buf`. A header that cannot be valid is `Bad` at once. */
    fun decode(buf: ByteArray, size: Int = buf.size): Decoded {
        if (size < 3) return Decoded.NeedMore
        val (kind, max) = when (val t = buf[0].toInt() and 0xff) {
            1 -> FrameKind.Audio to MAX_AUDIO
            2 -> FrameKind.Heartbeat to 0
            3 -> FrameKind.Hello to MAX_HELLO
            4 -> FrameKind.PausePhone to 0
            5 -> FrameKind.SwapPrompt to 1
            else -> return Decoded.Bad("unknown type $t")
        }
        val len = ((buf[1].toInt() and 0xff) shl 8) or (buf[2].toInt() and 0xff)
        if (len > max) return Decoded.Bad("length $len is over $max for $kind")
        if (size < 3 + len) return Decoded.NeedMore
        return Decoded.Got(Frame(kind, buf.copyOfRange(3, 3 + len)), 3 + len)
    }

    fun heartbeatLine(): String = """{"op":"hb"}"""

    fun stateLine(headphones: Where, call: Boolean, phonePlaying: Boolean): String {
        val at = when (headphones) {
            Where.Phone -> "phone"
            Where.Pc -> "pc"
            Where.None -> "none"
        }
        return """{"op":"state","headphones":"$at","call":$call,"phone_playing":$phonePlaying}"""
    }
}
