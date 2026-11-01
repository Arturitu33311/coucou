package com.agupta07505.smartisland.coucou.audiolink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class AudioLinkServiceLogicTest {
    // ── C1: nothing is written to the ssh stream on the thread that reports a change ─────────────

    @Test
    fun the_outbox_writes_on_its_own_thread_and_keeps_the_order() {
        val threads = CopyOnWriteArrayList<Thread>()
        val lines = CopyOnWriteArrayList<String>()
        val done = CountDownLatch(3)
        val outbox = Outbox { line ->
            threads += Thread.currentThread()
            lines += line
            done.countDown()
        }
        outbox.post("one")
        outbox.post("two")
        outbox.post("three")
        assertTrue(done.await(5, TimeUnit.SECONDS))
        outbox.close()
        assertEquals(listOf("one", "two", "three"), lines.toList())
        assertTrue("written on the caller's thread", threads.none { it === Thread.currentThread() })
    }

    @Test
    fun a_failing_write_does_not_stop_the_outbox_and_post_never_throws() {
        val lines = CopyOnWriteArrayList<String>()
        val done = CountDownLatch(1)
        val outbox = Outbox { line ->
            if (line == "bad") throw java.io.IOException("socket closed")
            lines += line
            done.countDown()
        }
        outbox.post("bad")
        outbox.post("good")
        assertTrue(done.await(5, TimeUnit.SECONDS))
        outbox.close()
        assertEquals(listOf("good"), lines.toList())
        outbox.post("after close") // must not throw
    }

    // ── I1: a short blip keeps the session; a long absence ends it ───────────────────────────────

    @Test
    fun a_session_survives_a_short_blip_and_ends_after_a_long_absence() {
        assertFalse(shouldLeave(goneSinceMs = null, nowMs = 10_000))
        assertFalse(shouldLeave(goneSinceMs = 10_000, nowMs = 10_000 + 6_999))
        assertTrue(shouldLeave(goneSinceMs = 10_000, nowMs = 10_000 + 7_000))
    }

    // ── I4: audio focus is taken only when sound arrives, again after a gap, and never assumed ───

    @Test
    fun the_first_chunk_asks_for_focus_and_nothing_plays_until_it_is_granted() {
        val gate = PlayerGate()
        assertEquals(GateStep.AskFocus, gate.next(1_000))
        // The request was refused: the next chunk asks again and is not played.
        assertEquals(GateStep.AskFocus, gate.next(1_020))
        gate.focusGranted()
        assertEquals(GateStep.Play, gate.next(1_040))
    }

    @Test
    fun a_gap_in_the_sound_starts_over_and_gives_focus_back() {
        val gate = PlayerGate()
        gate.next(0)
        gate.focusGranted()
        assertEquals(GateStep.Play, gate.next(20))
        assertEquals(GateStep.Reset, gate.next(20 + 401))
        // After a reset focus must be asked for again.
        assertEquals(GateStep.AskFocus, gate.next(20 + 421))
    }

    @Test
    fun losing_focus_stops_playing_until_it_is_back() {
        val gate = PlayerGate()
        gate.next(0)
        gate.focusGranted()
        assertEquals(GateStep.Play, gate.next(20))
        gate.focusLost()
        assertNotEquals(GateStep.Play, gate.next(40))
    }
}
