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

    // ── Audio focus: only for sound that goes on, never for a ping ────────────────────────────

    /** Chunks 20 ms apart from [from] for [ms] milliseconds, returning each step. */
    private fun feed(gate: PlayerGate, from: Long, ms: Long): List<GateStep> =
        (0..ms / 20).map { gate.next(from + it * 20) }

    @Test
    fun a_short_sound_plays_without_ever_taking_the_focus() {
        // A notification, a click of the island, a Claude Code "done": a few seconds at most.
        val steps = feed(PlayerGate(), 1_000, 3_900)
        assertTrue("a short sound must never ask for focus: it would pause the phone's music", steps.all { it == GateStep.Play })
    }

    @Test
    fun a_sound_that_goes_on_asks_for_focus_once_and_plays_after_it_is_granted() {
        val gate = PlayerGate()
        val steps = feed(gate, 0, 4_000)
        assertEquals("only the chunk that makes it sustained asks", 1, steps.count { it == GateStep.AskFocus })
        gate.focusGranted()
        assertEquals(GateStep.Play, gate.next(4_020))
    }

    @Test
    fun a_refused_request_is_asked_again_and_the_chunk_is_not_played() {
        val gate = PlayerGate()
        feed(gate, 0, 4_000)
        assertEquals(GateStep.AskFocus, gate.next(4_020))
        assertEquals(GateStep.AskFocus, gate.next(4_040))
        gate.focusGranted()
        assertEquals(GateStep.Play, gate.next(4_060))
    }

    @Test
    fun a_gap_in_the_sound_starts_over_and_the_next_burst_must_be_sustained_again() {
        val gate = PlayerGate()
        feed(gate, 0, 4_000)
        gate.focusGranted()
        assertEquals(GateStep.Reset, gate.next(4_000 + 401))
        // After the reset the focus has been given back and a new short sound must not take it.
        assertTrue(feed(gate, 4_421, 3_000).all { it == GateStep.Play })
    }

    @Test
    fun losing_focus_stops_playing_until_it_is_back() {
        val gate = PlayerGate()
        feed(gate, 0, 4_000)
        gate.focusGranted()
        assertEquals(GateStep.Play, gate.next(4_020))
        gate.focusLost() // a call, or another app took the speaker
        assertEquals(GateStep.AskFocus, gate.next(4_040))
        gate.focusGranted()
        assertEquals(GateStep.Play, gate.next(4_060))
    }

    @Test
    fun giving_the_focus_back_on_purpose_is_not_a_loss() {
        val young = PlayerGate()
        young.next(0)
        young.focusGranted()
        young.focusReleased() // handed back on purpose: a young burst just plays on
        assertEquals(GateStep.Play, young.next(20))

        val lost = PlayerGate()
        lost.next(0)
        lost.focusGranted()
        lost.focusLost() // taken by someone else: must ask again before playing, even in a young burst
        assertEquals(GateStep.AskFocus, lost.next(20))
    }
}
