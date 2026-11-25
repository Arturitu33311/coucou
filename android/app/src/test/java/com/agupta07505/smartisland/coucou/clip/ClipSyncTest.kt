package com.agupta07505.smartisland.coucou.clip

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ClipSyncTest {
    @Test
    fun a_code_survives() = assertEquals("482913", ClipSync.clean("482913"))

    @Test
    fun trims_and_keeps_newline_and_tab() {
        assertEquals("a\n\tb", ClipSync.clean("  a\n\tb \n"))
    }

    @Test
    fun control_characters_are_dropped() {
        assertEquals("ab", ClipSync.clean("a\u0007\u001b[0m".replace("[0m", "") + "b"))
        assertEquals("hola", ClipSync.clean("ho\u0000la"))
    }

    @Test
    fun empty_whitespace_and_null_are_nothing() {
        assertNull(ClipSync.clean(null))
        assertNull(ClipSync.clean(""))
        assertNull(ClipSync.clean("  \n\t "))
        assertNull(ClipSync.clean("\u0007\u0000"))
    }

    @Test
    fun the_limit_is_four_thousand_characters_and_longer_is_not_cut_but_dropped() {
        assertEquals(4000, ClipSync.clean("x".repeat(4000))!!.length)
        assertNull(ClipSync.clean("x".repeat(4001)))
    }

    @Test
    fun an_emoji_outside_the_bmp_survives_and_counts_once() {
        assertEquals("hola 😀", ClipSync.clean("hola 😀"))
        assertEquals(4000, ClipSync.clean("😀".repeat(4000))!!.codePointCount(0, 8000))
    }

    @Test
    fun fingerprints_are_stable_and_distinguish_texts() {
        // Pinned: the laptop computes the same value (clip_sync.rs fingerprint tests).
        assertEquals(-3750763034362895579L, ClipSync.fingerprint(""))
        assertEquals(ClipSync.fingerprint("482913"), ClipSync.fingerprint("482913"))
        assertNotEquals(ClipSync.fingerprint("482913"), ClipSync.fingerprint("482914"))
        assertEquals(-5808556873153909620L, ClipSync.fingerprint("a"))      // 0xaf63dc4c8601ec8c
        assertEquals(-3961981059849565792L, ClipSync.fingerprint("482913")) // 0xc9043737b0d749a0
    }

    @Test
    fun the_wire_line_has_op_text_and_time() {
        val o = JSONObject(ClipSync.line("482913", 1_700_000_000_000))
        assertEquals("clip", o.getString("op"))
        assertEquals("482913", o.getString("text"))
        assertEquals(1_700_000_000_000, o.getLong("at"))
    }

    @Test
    fun the_same_text_twice_in_a_row_is_sent_once() {
        val g = LoopGuard()
        assertTrue(g.shouldSend("x", 0))
        assertFalse(g.shouldSend("x", 10_000))
        assertTrue(g.shouldSend("y", 11_000))
        assertTrue(g.shouldSend("x", 12_000)) // not twice in a row any more
    }

    @Test
    fun an_echo_of_what_just_arrived_is_not_sent_back() {
        val g = LoopGuard()
        g.noteReceived("x", 0)
        assertFalse(g.shouldSend("x", 1_000))
        assertTrue(g.shouldSend("x", 5_001)) // after five seconds it is a new copy
    }

    @Test
    fun the_log_line_of_a_refused_read_is_recognized_only_for_our_package() {
        val ours = "com.agupta07505.smartisland"
        val aosp = "W/ClipboardService( 1234): Denying clipboard access to $ours, application is not in focus nor is it a system service for user 0"
        assertTrue(ClipLogTrigger.isDenialFor(aosp, ours))
        assertFalse(ClipLogTrigger.isDenialFor(aosp.replace(ours, "org.kde.kdeconnect_tp"), ours))
        assertTrue(ClipLogTrigger.isDenialFor("clipboard access denied for $ours", ours))
        assertFalse(ClipLogTrigger.isDenialFor("W/ActivityManager: $ours crashed", ours))
        assertFalse(ClipLogTrigger.isDenialFor("clipboard changed by $ours", ours)) // no refusal in it
    }
}
