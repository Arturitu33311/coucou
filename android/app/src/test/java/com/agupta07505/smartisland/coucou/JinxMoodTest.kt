package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.JinxMood
import com.agupta07505.smartisland.coucou.ui.MochiMood
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

class JinxMoodTest {
    @Test fun `the phone reads Jinx's replies as the PC does`() {
        val text = javaClass.classLoader!!.getResourceAsStream("jinx-mood-vectors.json")!!.bufferedReader().readText()
        val cases = JSONObject(text).getJSONArray("cases")
        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            val want = if (c.isNull("emote")) null else c.getString("emote")
            assertEquals(c.getString("text"), want, JinxMood.emote(c.getString("text")))
        }
    }

    @Test fun `every emote of the rules has a face on the phone`() {
        for (e in listOf("crying", "laughing", "love", "scared", "grumpy", "sad", "flustered", "shy", "smug", "confused",
            "surprised", "excited", "proud", "relieved", "determined", "curious", "sleepy", "bored", "happy")) {
            assertNotNull(e, JinxMood.moodOf(e))
        }
        assertNull(JinxMood.moodOf(null))
        assertEquals(MochiMood.Laughing, JinxMood.moodOfText("jajaja"))
    }

    private fun line(src: String, text: String, ts: Double) = ChatLine("x$ts", ts, src, text, null, true)

    @Test fun `the chat's Mochi wears her last reply while it is fresh`() {
        val now = 1_000_000.0
        assertEquals(MochiMood.Thinking, JinxMood.headerMood(listOf(line("jinx", "jaja", now)), busy = true, nowS = now))
        assertEquals(MochiMood.Laughing, JinxMood.headerMood(listOf(line("s21", "hola", now - 5), line("jinx", "jaja", now)), false, now))
        // Old news, or the user spoke last: he listens.
        assertEquals(MochiMood.Listening, JinxMood.headerMood(listOf(line("jinx", "jaja", now - 3600)), false, now))
        assertEquals(MochiMood.Listening, JinxMood.headerMood(listOf(line("jinx", "jaja", now), line("s21", "ok", now)), false, now))
        // A neutral reply, and a millisecond stamp.
        assertEquals(MochiMood.Listening, JinxMood.headerMood(listOf(line("jinx", "El servidor responde.", now)), false, now))
        assertEquals(MochiMood.Sad, JinxMood.headerMood(listOf(line("jinx", "lo siento", now * 1000)), false, now))
    }

    @Test fun `the mood she sent wins over her words, and a bad one is ignored`() {
        assertEquals("sad", JinxMood.resolve("sad", "jajaja 😂"))
        assertEquals("laughing", JinxMood.resolve(null, "jajaja"))
        assertEquals("laughing", JinxMood.resolve("furious", "jajaja"))
        assertEquals("laughing", JinxMood.resolve("Sad", "jajaja")) // exact names only
        assertNull(JinxMood.resolve(null, "El servidor responde."))
        assertEquals(MochiMood.Sad, JinxMood.moodOfReply("sad", "jajaja"))
        assertEquals(MochiMood.Laughing, JinxMood.moodOfReply(null, "jajaja"))
    }

    @Test fun `the chat's Mochi uses the mood of the line before reading its words`() {
        val now = 1_000_000.0
        val withMood = ChatLine("m", now, "jinx", "El servidor responde.", null, true, mood = "proud")
        assertEquals(MochiMood.Proud, JinxMood.headerMood(listOf(withMood), false, now))
        val contradicting = ChatLine("n", now, "jinx", "jaja 😂", null, true, mood = "crying")
        assertEquals(MochiMood.Crying, JinxMood.headerMood(listOf(contradicting), false, now))
        // A mood that is not Mochi's falls back to the words.
        val bogus = ChatLine("o", now, "jinx", "jaja", null, true, mood = "nope")
        assertEquals(MochiMood.Laughing, JinxMood.headerMood(listOf(bogus), false, now))
    }

    @Test fun `the nineteen names are all faces on the phone`() {
        assertEquals(19, JinxMood.EMOTES.size)
        assertEquals(19, JinxMood.EMOTES.toSet().size)
        for (e in JinxMood.EMOTES) assertNotNull(e, JinxMood.moodOf(e))
        for (e in JinxMood.EMOTES) assertEquals(e, JinxMood.validEmote(e))
    }

    @Test fun `between replies the chat's Mochi wears her background mood`() {
        val now = 1_000_000.0
        assertEquals(MochiMood.Happy, JinxMood.headerMood(emptyList(), false, now, background = MochiMood.Happy))
        assertEquals(MochiMood.Sad, JinxMood.headerMood(listOf(line("jinx", "jaja", now - 3600)), false, now, MochiMood.Sad))
        assertEquals(MochiMood.Grumpy, JinxMood.headerMood(listOf(line("jinx", "El servidor responde.", now)), false, now, MochiMood.Grumpy))
        // A fresh reply's own mood still wins, and so does her thinking.
        assertEquals(MochiMood.Laughing, JinxMood.headerMood(listOf(line("jinx", "jaja", now)), false, now, MochiMood.Sad))
        assertEquals(MochiMood.Thinking, JinxMood.headerMood(emptyList(), true, now, MochiMood.Sad))
        assertEquals(MochiMood.Listening, JinxMood.headerMood(emptyList(), false, now, null))
    }
}
