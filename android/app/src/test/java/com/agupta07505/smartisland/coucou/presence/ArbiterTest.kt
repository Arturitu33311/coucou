package com.agupta07505.smartisland.coucou.presence

import com.agupta07505.smartisland.coucou.agents.RemoteMsg
import com.agupta07505.smartisland.coucou.agents.RemoteProtocol
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Which device shows Mochi. The vectors are shared with the PC side (presence.rs reads the same file). */
class ArbiterTest {
    private fun beat(o: JSONObject) = Beat(
        device = o.getString("device"),
        idleS = o.getLong("idle"),
        screenOn = o.getBoolean("screen_on"),
        serverOk = o.getBoolean("server_ok"),
        serverActive = if (o.isNull("server_active")) null else o.getString("server_active"),
    )

    @Test
    fun the_shared_vectors_all_hold() {
        val text = javaClass.classLoader!!.getResourceAsStream("presence-vectors.json")!!.bufferedReader().readText()
        val cases = JSONObject(text).getJSONArray("cases")
        assertTrue(cases.length() >= 15)
        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            val peers = c.getJSONArray("peers").let { a ->
                (0 until a.length()).map { Seen(beat(a.getJSONObject(it)), a.getJSONObject(it).getLong("age_ms")) }
            }
            val previous = if (c.isNull("previous")) null else c.getString("previous")
            assertEquals(c.getString("name"), c.getString("expect"), Arbiter.decide(beat(c.getJSONObject("me")), peers, previous))
        }
    }

    @Test
    fun a_peer_beat_round_trips_from_the_laptop_or_relayed_from_the_other_phone() {
        val line = JSONObject()
            .put("t", "peer").put("device", "laptop").put("idle", 12).put("screen_on", true)
            .put("server_ok", false).put("server_active", JSONObject.NULL).put("music", true).put("news", false)
            .toString()
        val msg = RemoteProtocol.parse(line, 0L) as RemoteMsg.Peer
        assertEquals(12L, msg.beat.idleS)
        assertTrue(msg.music)
        assertNull(msg.beat.serverActive)
        // The other phone's beat comes through the laptop under its own id.
        val relayed = RemoteProtocol.parse(line.replace("laptop", "s22"), 0L) as RemoteMsg.Peer
        assertEquals("s22", relayed.beat.device)
        // A device we do not know is not a thing the link carries.
        assertNull(RemoteProtocol.parse(line.replace("laptop", "vga"), 0L))
        assertEquals(RemoteMsg.PeerGone("s22"), RemoteProtocol.parse("""{"t":"peer_gone","device":"s22"}""", 0L))
        assertNull(RemoteProtocol.parse("""{"t":"peer_gone","device":"vga"}""", 0L))
        // An id we do not know is no server answer, and an absurd idle is clamped.
        val odd = JSONObject(line).put("server_active", "tv").put("idle", Long.MAX_VALUE).toString()
        val p = RemoteProtocol.parse(odd, 0L) as RemoteMsg.Peer
        assertNull(p.beat.serverActive)
        assertEquals(RemoteProtocol.MAX_IDLE_S, p.beat.idleS)
        // The panel is an answer ("nobody in use"), and the server's "pc" is the laptop.
        val vga = RemoteProtocol.parse(JSONObject(line).put("server_active", "vga").toString(), 0L) as RemoteMsg.Peer
        assertEquals("vga", vga.beat.serverActive)
        val pc = RemoteProtocol.parse(JSONObject(line).put("server_active", "pc").toString(), 0L) as RemoteMsg.Peer
        assertEquals("laptop", pc.beat.serverActive)
    }

    @Test
    fun each_phone_knows_which_it_is_from_its_model() {
        assertEquals("s21", Arbiter.idForModel("SM-G998U"))
        assertEquals("s21", Arbiter.idForModel("SM-G998B"))
        assertEquals("s22", Arbiter.idForModel("SM-S908U"))
        assertEquals("s22", Arbiter.idForModel("SM-S908B/DS"))
        assertEquals("s21", Arbiter.idForModel(null))
        assertTrue(Arbiter.KNOWN.containsAll(listOf("s21", "s22", "laptop")))
    }

    @Test
    fun the_phone_beat_carries_state_and_nothing_else() {
        val line = RemoteProtocol.peer(Beat("s21", 4, true, true, "s21"), music = true, news = false)
        val o = JSONObject(line)
        assertEquals("peer", o.getString("op"))
        assertEquals(setOf("op", "device", "idle", "screen_on", "server_ok", "server_active", "music", "news"), o.keys().asSequence().toSet())
    }
}
