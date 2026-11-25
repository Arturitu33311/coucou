package com.agupta07505.smartisland.coucou.presence

import com.agupta07505.smartisland.coucou.ui.MochiMood
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pure presence logic: no Android needed. */
class MochiPresenceTest {

    @Test
    fun moods_the_server_sends_are_valid() {
        assertEquals("Happy", validMood("Happy"))
        assertEquals("Alert", validMood("Alert"))
        assertEquals("Grumpy", validMood("Grumpy"))
        assertNull(validMood("Furious"))
        assertNull(validMood("Working"))
        assertNull(validMood(null))
    }

    @Test
    fun category_maps_to_mochi_mood() {
        assertEquals(MochiMood.Happy, jinxMoodOf("mochi:Happy:info"))
        assertEquals(MochiMood.Sleepy, jinxMoodOf("mochi:Sleepy:warn"))
        assertEquals(MochiMood.Listening, jinxMoodOf("mochi:Bogus:info"))
        assertEquals(MochiMood.Listening, jinxMoodOf(null))
        assertEquals(MochiMood.Listening, jinxMoodOf("telegram"))
    }

    @Test
    fun only_alert_rings() {
        assertTrue(jinxIsAlert("mochi:Alert:alert"))
        assertFalse(jinxIsAlert("mochi:Happy:info"))
        assertFalse(jinxIsAlert("mochi:Alert:warn"))
        assertFalse(jinxIsAlert(null))
    }

    @Test
    fun inbox_lines_become_messages() {
        val body = JSONObject()
            .put("messages", org.json.JSONArray().apply {
                put(JSONObject().put("id", 7).put("text", "hola").put("mood", "Happy")
                    .put("kind", "info").put("source", "nudge").put("ts", 1000))
                put(JSONObject().put("id", 8).put("text", "  ").put("kind", "alert"))
                put(JSONObject().put("id", 9).put("text", "urgente").put("kind", "doom"))
            })
            .toString()
        val msgs = MochiPresence.parseInbox(body)
        assertEquals(2, msgs.size)
        assertEquals(7L, msgs[0].id)
        assertEquals("Happy", msgs[0].mood)
        assertEquals("nudge", msgs[0].source)
        assertEquals("info", msgs[1].kind)
        assertEquals("urgente", msgs[1].text)
    }

    @Test
    fun the_heartbeat_answer_carries_her_background_mood() {
        assertEquals("happy", MochiPresence.readJinxMood(JSONObject("""{"active_device":"vga","jinx_mood":"happy"}""")))
        assertNull(MochiPresence.readJinxMood(JSONObject("""{"active_device":"vga","jinx_mood":null}""")))
        assertNull(MochiPresence.readJinxMood(JSONObject("""{"active_device":"vga","jinx_mood":"Happy"}""")))
        assertNull(MochiPresence.readJinxMood(JSONObject("""{"active_device":"vga"}""")))
    }

    @Test
    fun a_late_message_says_how_late_and_is_never_dropped() {
        val m = PresenceMsg(1, "Arrancamos el día", null, "info", "notify", 1_000)
        assertEquals(m, MochiPresence.withAge(m, 1_000 + 59 * 60))
        assertEquals("🕓 5 h · Arrancamos el día", MochiPresence.withAge(m, 1_000 + 5 * 3600 + 20).text)
        assertEquals("🕓 2 d · Arrancamos el día", MochiPresence.withAge(m, 1_000 + 49 * 3600).text)
        assertEquals(m, MochiPresence.withAge(m, 500))
    }

    @Test
    fun the_server_names_are_read_as_ours() {
        assertEquals("laptop", Arbiter.fromServer("pc"))
        assertEquals("s22", Arbiter.serverAnswer("s22"))
        assertEquals("vga", Arbiter.serverAnswer("vga"))
        assertEquals("laptop", Arbiter.serverAnswer("pc"))
        assertNull(Arbiter.serverAnswer("tv"))
        assertNull(Arbiter.serverAnswer(""))
        assertNull(Arbiter.serverAnswer(null))
    }
}

