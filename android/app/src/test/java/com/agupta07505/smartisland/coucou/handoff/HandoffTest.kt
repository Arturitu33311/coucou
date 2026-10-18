package com.agupta07505.smartisland.coucou.handoff

import com.agupta07505.smartisland.coucou.agents.RemoteMsg
import com.agupta07505.smartisland.coucou.agents.RemoteProtocol
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HandoffTest {
    private val doc: JSONObject = JSONObject(
        javaClass.classLoader!!.getResourceAsStream("wire-fixtures.json")!!.bufferedReader().readText(),
    )
    private val offerLine = doc.getJSONObject("laptop_to_phone").getJSONObject("handoff").toString()
    private val doneLine = doc.getJSONObject("laptop_to_phone").getJSONObject("handoff_done").toString()

    @Test
    fun only_https_pages_of_the_known_services_are_ever_opened() {
        for (ok in listOf(
            "https://open.spotify.com/track/6OwjpJLqayrcTuzBvxoGMd",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=151s",
            "https://open.spotify.com/search/JESUS%20Fuerza%20Regida",
            "https://youtu.be/dQw4w9WgXcQ?t=5",
        )) assertNotNull(ok, HandoffRules.safe(ok))
        for (bad in listOf(
            "http://open.spotify.com/track/6OwjpJLqayrcTuzBvxoGMd", // not https
            "https://open.spotify.com.evil.example/track/x",
            "https://user:pw@open.spotify.com/track/x",
            "https://open.spotify.com:8443/track/x",
            "intent://scan/#Intent;scheme=zxing;end",
            "javascript:alert(1)",
            "file:///sdcard/x",
            "spotify:track:6OwjpJLqayrcTuzBvxoGMd",
            "https://example.com/",
            "",
        )) assertNull(bad, HandoffRules.safe(bad))
    }

    @Test
    fun the_position_is_shown_as_a_clock() {
        assertEquals("2:31", HandoffRules.clock(151_000))
        assertEquals("0:00", HandoffRules.clock(-5))
        assertEquals("1:02:03", HandoffRules.clock(3_723_000))
    }

    @Test
    fun the_laptops_offer_is_read_by_the_phone_it_is_for_and_ignored_by_the_other() {
        val mine = RemoteProtocol.parse(offerLine, 0L, "s22")
        assertTrue(mine is RemoteMsg.Handoff)
        val o = (mine as RemoteMsg.Handoff).offer
        assertEquals("h-1", o.id)
        assertEquals("track", o.kind)
        assertEquals(151_000L, o.posMs)
        assertEquals("Bose QC Earbuds II", o.headset)
        assertTrue(!o.auto)
        assertNull(RemoteProtocol.parse(offerLine, 0L, "s21"))
        // Without an id of its own (tests) nothing is filtered.
        assertTrue(RemoteProtocol.parse(offerLine, 0L) is RemoteMsg.Handoff)

        val done = RemoteProtocol.parse(doneLine, 0L, "s22") as RemoteMsg.HandoffDone
        assertTrue(done.ok)
        assertNull(RemoteProtocol.parse(doneLine, 0L, "s21"))
    }

    @Test
    fun an_offer_without_an_id_or_with_an_absurd_one_is_dropped() {
        assertNull(RemoteProtocol.parse(JSONObject(offerLine).put("id", "").toString(), 0L))
        assertNull(RemoteProtocol.parse(JSONObject(offerLine).put("id", "x".repeat(65)).toString(), 0L))
    }

    @Test
    fun the_accept_is_the_one_word_the_laptop_reads() {
        val line = JSONObject(RemoteProtocol.handoffAccept("h-1"))
        val want = doc.getJSONObject("phone_to_laptop").getJSONObject("handoff_accept")
        assertEquals(want.keys().asSequence().toSet(), line.keys().asSequence().toSet())
        assertEquals("handoff_accept", line.getString("op"))
        assertEquals("h-1", line.getString("id"))
    }
}
