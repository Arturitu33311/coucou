package com.agupta07505.smartisland.coucou.audiolink

import org.junit.Assert.assertEquals
import org.junit.Test

class JitterTunerTest {
    private fun tuner() = JitterTuner(baseMs = 120, capacityMs = 600, stepMs = 80, decayAfterMs = 60_000, minGapMs = 2_000)

    @Test
    fun a_good_network_keeps_the_small_buffer() {
        val t = tuner()
        for (s in 0..300) assertEquals(null, t.update(0, 0, s * 1000L, true))
        assertEquals(120, t.bufferMs)
    }

    @Test
    fun each_real_cut_grows_the_buffer_by_a_step_up_to_the_capacity() {
        val t = tuner()
        assertEquals(200, t.update(1, 0, 1_000, true))
        assertEquals(280, t.update(2, 0, 4_000, true))
        var n = 2
        var now = 4_000L
        var last: Int? = null
        while (t.bufferMs < 600) { n++; now += 3_000; last = t.update(n, 0, now, true) }
        assertEquals(600, last)
        assertEquals(null, t.update(n + 1, 0, now + 3_000, true)) // at the capacity: no change, never beyond
        assertEquals(600, t.bufferMs)
    }

    @Test
    fun a_burst_the_buffer_could_not_hold_grows_it_too() {
        val t = tuner()
        assertEquals(200, t.update(0, 1, 1_000, true))
        assertEquals(280, t.update(0, 5, 4_000, true))
        // Overflows count even when the laptop is silent right now: a burst is a burst.
        assertEquals(360, t.update(0, 6, 7_000, lastChunkLoud = false))
    }

    @Test
    fun one_burst_of_drops_does_not_send_it_to_the_maximum() {
        val t = tuner()
        assertEquals(200, t.update(0, 1, 1_000, true))
        // Seven more drops within the next two seconds: still only one step.
        assertEquals(null, t.update(0, 8, 1_500, true))
        assertEquals(null, t.update(0, 9, 2_900, true))
        assertEquals(200, t.bufferMs)
        assertEquals(280, t.update(0, 10, 3_100, true))
    }

    @Test
    fun the_speaker_running_dry_with_nothing_playing_on_the_pc_does_not_grow_it() {
        val t = tuner()
        assertEquals(null, t.update(1, 0, 1_000, lastChunkLoud = false)) // the laptop stopped sending: silence by design
        assertEquals(null, t.update(2, 0, 2_000, lastChunkLoud = false))
        assertEquals(120, t.bufferMs)
        assertEquals(200, t.update(3, 0, 3_000, lastChunkLoud = true))   // a later real cut still counts
    }

    @Test
    fun it_shrinks_back_after_a_minute_without_problems_but_never_below_the_base() {
        val t = tuner()
        t.update(1, 0, 1_000, true); t.update(2, 0, 4_000, true) // 280
        assertEquals(null, t.update(2, 0, 30_000, true))        // not yet
        assertEquals(240, t.update(2, 0, 64_001, true))
        assertEquals(null, t.update(2, 0, 100_000, true))
        assertEquals(200, t.update(2, 0, 124_002, true))
        t.update(2, 0, 190_000, true); t.update(2, 0, 260_000, true); t.update(2, 0, 330_000, true)
        assertEquals(120, t.bufferMs)
        assertEquals(null, t.update(2, 0, 400_000, true))
    }

    @Test
    fun a_problem_just_after_shrinking_grows_it_again() {
        val t = tuner()
        t.update(1, 0, 1_000, true)                    // 200
        assertEquals(160, t.update(1, 0, 61_001, true))
        assertEquals(240, t.update(2, 0, 62_000, true))
    }
}
