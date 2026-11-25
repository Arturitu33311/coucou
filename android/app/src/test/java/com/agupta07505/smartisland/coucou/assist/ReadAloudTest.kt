package com.agupta07505.smartisland.coucou.assist

import com.agupta07505.smartisland.coucou.presence.PresenceMsg
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class ReadAloudTest {
    private val now = 1_800_000_000_000L
    private fun msg(kind: String = "info", ageS: Long = 60) =
        PresenceMsg(1, "Tienes lab en 15 minutos", null, kind, "x", now / 1000 - ageS)
    private fun ok(m: PresenceMsg, hour: Int = 12, st: ReadAloudState = ReadAloudState(), enabled: Boolean = true, chat: Boolean = false) =
        ReadAloudPolicy.allow(enabled, m, now, hour, 100, st, chat)

    @Test fun offByDefaultAndWhileReadingTheChat() {
        assertFalse(ok(msg(), enabled = false))
        assertFalse(ok(msg(), chat = true))
        assertTrue(ok(msg()))
    }

    @Test fun quietHoursAndOldNotices() {
        assertFalse(ok(msg(), hour = 23))
        assertFalse(ok(msg(), hour = 3))
        assertTrue(ok(msg(), hour = 7))
        assertFalse(ok(msg(ageS = 3601)))
    }

    @Test fun cooldownDailyCapAndAlerts() {
        val justSpoke = ReadAloudState(100, 1, now - 60_000)
        assertFalse(ok(msg("info"), st = justSpoke))
        assertTrue(ok(msg("alert"), st = justSpoke))
        assertTrue(ok(msg("info"), st = ReadAloudState(100, 1, now - ReadAloudPolicy.COOLDOWN_MS)))
        val full = ReadAloudState(100, ReadAloudPolicy.MAX_PER_DAY, 0)
        assertFalse(ok(msg("alert"), st = full))
        // A new day starts the count again.
        assertTrue(ReadAloudPolicy.allow(true, msg(), now, 12, 101, full, false))
        assertEquals(ReadAloudState(101, 1, now), ReadAloudPolicy.spent(full, now, 101))
        assertEquals(ReadAloudState(100, 3, now), ReadAloudPolicy.spent(ReadAloudState(100, 2, 0), now, 100))
    }

    private class FakeSecrets(private val key: String?) : Secrets {
        override fun get(name: String) = key.takeIf { name == SecretNames.G2_KEY }
        override fun put(name: String, value: String) = Unit
    }

    @Test fun jinxVoiceNeedsKeyAndHttps() {
        val dir = File.createTempFile("jinxvoice", "").let { it.delete(); it.mkdirs(); it }
        var seen: Triple<String, String, String>? = null
        val post = { u: String, k: String, b: String -> seen = Triple(u, k, b); ByteArray(2048) { 7 } }
        assertNull(JinxVoice(FakeSecrets(null), { "https://s.ts.net:8443" }, dir, post).fetch("hola"))
        assertNull(JinxVoice(FakeSecrets("k"), { "http://s:8898" }, dir, post).fetch("hola"))
        assertNull(seen)
        val f = JinxVoice(FakeSecrets("k"), { "https://s.ts.net:8443/" }, dir, post).fetch("[warmly] hola")!!
        assertEquals(2048, f.length().toInt())
        assertEquals("https://s.ts.net:8443/api/g2/speak", seen!!.first)
        assertEquals("k", seen!!.second)
        assertTrue(seen!!.third.contains("[warmly] hola"))
        dir.deleteRecursively()
    }

    @Test fun jinxVoiceFailsQuietly() {
        val dir = File.createTempFile("jinxvoice", "").let { it.delete(); it.mkdirs(); it }
        assertNull(JinxVoice(FakeSecrets("k"), { "https://s" }, dir, { _, _, _ -> throw java.io.IOException("HTTP 502") }).fetch("hola"))
        dir.deleteRecursively()
    }

    @Test fun longAnswersStartWithTheirFirstSentences() {
        assertEquals(listOf("Listo."), Speaker.splitForSpeech("Listo."))
        val s1 = "Tienes una tarea abierta en la lista y nada nuevo de Zynara."
        val s2 = "Mañana tienes laboratorio a las diez, así que descansa y carga el teléfono antes de dormir para que no te falle."
        val s3 = "Ah, y el servidor sigue arriba."
        val parts = Speaker.splitForSpeech("[warmly] $s1 $s2 $s3")
        assertEquals(2, parts.size)
        assertTrue(parts[0].length <= 170)
        assertTrue(parts[0].startsWith("[warmly] Tienes"))
        assertEquals("[warmly] $s1 $s2 $s3", parts.joinToString(" "))
        // One very long sentence cannot be cut at a sentence end: it stays whole.
        assertEquals(1, Speaker.splitForSpeech("palabra ".repeat(60).trim()).size)
    }

    @Test fun tagsAreKeptForHerVoiceAndDroppedForTheSystemOne() {
        val t = "**[warmly]** Mira https://x.co/a esto"
        assertEquals("Mira esto", Speaker.spokenForm(t))
        assertEquals("[warmly] Mira esto", Speaker.remoteForm(t))
    }
}
