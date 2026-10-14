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
}
