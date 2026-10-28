package com.agupta07505.smartisland.coucou.audiolink

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The laptop's test (`audiolink_wire.rs`) reads the same file. */
class AudioLinkWireTest {
    private val doc = JSONObject(
        javaClass.classLoader!!.getResourceAsStream("audiolink-vectors.json")!!.bufferedReader().readText(),
    )

    private fun unhex(s: String) = ByteArray(s.length / 2) { s.substring(it * 2, it * 2 + 2).toInt(16).toByte() }

    @Test
    fun every_shared_wire_case_holds() {
        val cases = doc.getJSONArray("wire_cases")
        assertTrue("the vector file lost its wire cases", cases.length() >= 10)
        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            val name = c.getString("name")
            val got = AudioLinkWire.decode(unhex(c.getString("hex")))
            when (c.getString("result")) {
                "audio", "heartbeat", "hello", "pause_phone", "swap_prompt" -> {
                    assertTrue("$name: $got", got is Decoded.Got)
                    got as Decoded.Got
                    val kind = when (got.frame.kind) {
                        FrameKind.Audio -> "audio"
                        FrameKind.Heartbeat -> "heartbeat"
                        FrameKind.Hello -> "hello"
                        FrameKind.PausePhone -> "pause_phone"
                        FrameKind.SwapPrompt -> "swap_prompt"
                    }
                    assertEquals(name, c.getString("result"), kind)
                    assertEquals(name, c.getInt("consumed"), got.consumed)
                }
                "need_more" -> assertEquals(name, Decoded.NeedMore, got)
                "error" -> assertTrue("$name: $got", got is Decoded.Bad)
                else -> error("unknown result in $name")
            }
        }
    }

    @Test
    fun the_lines_the_phone_writes_are_the_ones_the_laptop_reads() {
        val cases = doc.getJSONArray("line_cases")
        var checked = 0
        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            when (c.getString("result")) {
                "heartbeat" -> {
                    assertEquals(c.getString("line"), AudioLinkWire.heartbeatLine())
                    checked++
                }
                "state" -> {
                    val at = when (c.getString("headphones")) {
                        "phone" -> Where.Phone
                        "pc" -> Where.Pc
                        else -> Where.None
                    }
                    assertEquals(
                        c.getString("name"),
                        c.getString("line"),
                        AudioLinkWire.stateLine(at, c.getBoolean("call"), c.getBoolean("phone_playing")),
                    )
                    checked++
                }
            }
        }
        assertTrue("no line case was checked", checked >= 3)
    }

    @Test
    fun decode_reads_only_the_bytes_it_is_told_about() {
        val buf = unhex("0100020102") + ByteArray(10)
        assertEquals(Decoded.NeedMore, AudioLinkWire.decode(buf, 4))
    }

    @Test
    fun an_audio_frame_of_exactly_the_limit_decodes_and_one_more_does_not() {
        val ok = byteArrayOf(1, (AudioLinkWire.MAX_AUDIO shr 8).toByte(), AudioLinkWire.MAX_AUDIO.toByte()) + ByteArray(AudioLinkWire.MAX_AUDIO)
        assertEquals(3 + AudioLinkWire.MAX_AUDIO, (AudioLinkWire.decode(ok) as Decoded.Got).consumed)
        val over = byteArrayOf(1, ((AudioLinkWire.MAX_AUDIO + 1) shr 8).toByte(), (AudioLinkWire.MAX_AUDIO + 1).toByte())
        assertTrue(AudioLinkWire.decode(over) is Decoded.Bad)
    }
}
