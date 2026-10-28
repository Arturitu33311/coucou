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
enum class FrameKind(val id: Int) { Audio(1), Heartbeat(2), Hello(3) }

class Frame(val kind: FrameKind, val payload: ByteArray)

sealed interface Decoded {
    data class Got(val frame: Frame, val consumed: Int) : Decoded
    data object NeedMore : Decoded
    data class Bad(val why: String) : Decoded
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
