package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.ui.IdleGesture
import com.agupta07505.smartisland.coucou.ui.MochiIdleScript
import com.agupta07505.smartisland.coucou.ui.MochiMood
import com.agupta07505.smartisland.R
import kotlin.random.Random
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MochiIdleScriptTest {
    private val awake = 60L

    @Test
    fun he_does_not_fall_asleep_just_because_the_list_is_empty() {
        // The phone's normal state (nothing to do) used to be Sleepy all day.
        assertEquals(MochiMood.Idle, MochiIdleScript.calmMood(night = false, idleSeconds = awake, soonTask = false))
    }

    @Test
    fun he_sleeps_after_twenty_minutes_without_a_touch() {
        val limit = MochiIdleScript.SLEEP_AFTER_S
        assertEquals(20 * 60L, limit)
        assertFalse(MochiIdleScript.idleAsleep(limit - 1))
        assertTrue(MochiIdleScript.idleAsleep(limit))
        assertEquals(MochiMood.Sleepy, MochiIdleScript.calmMood(night = false, idleSeconds = limit, soonTask = false))
    }

    @Test
    fun night_still_puts_him_to_sleep() {
        assertEquals(MochiMood.Sleepy, MochiIdleScript.calmMood(night = true, idleSeconds = 0, soonTask = false))
    }

    @Test
    fun a_task_due_soon_keeps_him_listening_even_when_sleepy() {
        assertEquals(MochiMood.Listening, MochiIdleScript.calmMood(night = true, idleSeconds = 99_999, soonTask = true))
    }

    @Test
    fun soon_means_within_fifteen_minutes_and_not_yet_overdue() {
        val now = 1_000_000_000_000L
        fun at(min: Long) = now / 1000 + min * 60
        assertTrue(MochiIdleScript.isSoon(listOf(at(10)), now))
        assertFalse(MochiIdleScript.isSoon(listOf(at(16)), now))
        assertFalse(MochiIdleScript.isSoon(listOf(at(-1)), now))
        assertFalse(MochiIdleScript.isSoon(emptyList(), now))
    }

    @Test
    fun the_next_gesture_is_never_none_and_never_the_same_twice() {
        val rnd = Random(7)
        var prev = IdleGesture.None
        repeat(500) {
            val g = MochiIdleScript.next(prev, hour = 10, allowThought = true, rnd = rnd)
            assertNotEquals(IdleGesture.None, g)
            assertNotEquals(prev, g)
            prev = g
        }
    }

    @Test
    fun the_thought_bubble_only_comes_when_allowed() {
        val rnd = Random(3)
        var prev = IdleGesture.None
        repeat(500) {
            prev = MochiIdleScript.next(prev, hour = 15, allowThought = false, rnd = rnd)
            assertNotEquals(IdleGesture.Think, prev)
        }
        // …and it does come when allowed.
        val seen = (0 until 500).scan(IdleGesture.None) { p, _ ->
            MochiIdleScript.next(p, hour = 15, allowThought = true, rnd = rnd)
        }
        assertTrue(IdleGesture.Think in seen)
    }

    @Test
    fun mornings_favour_yawns_and_stretches() {
        val rnd = Random(11)
        var prev = IdleGesture.None
        var morning = 0
        var evening = 0
        repeat(2000) {
            prev = MochiIdleScript.next(prev, hour = 8, allowThought = false, rnd = rnd)
            if (prev == IdleGesture.Yawn || prev == IdleGesture.Stretch) morning++
        }
        prev = IdleGesture.None
        repeat(2000) {
            prev = MochiIdleScript.next(prev, hour = 21, allowThought = false, rnd = rnd)
            if (prev == IdleGesture.Yawn || prev == IdleGesture.Stretch) evening++
        }
        assertTrue("morning=$morning evening=$evening", morning > evening)
    }

    @Test
    fun gaps_between_gestures_are_calm() {
        val rnd = Random(1)
        repeat(200) {
            val gap = MochiIdleScript.gapMs(rnd)
            assertTrue(gap in 6_000L..15_000L)
        }
    }

    @Test
    fun every_gesture_has_a_duration() {
        for (g in IdleGesture.entries) {
            if (g == IdleGesture.None) continue
            assertTrue(MochiIdleScript.durationMs(g) in 500L..4_000L)
        }
    }

    @Test
    fun phrases_fit_the_situation_and_rotate() {

        val empty = MochiIdleScript.phrases(open = 0, overdue = 0, hour = 12)
        val busy = MochiIdleScript.phrases(open = 3, overdue = 0, hour = 12)
        val late = MochiIdleScript.phrases(open = 3, overdue = 1, hour = 12)
        assertTrue(empty.size >= 3)
        assertTrue(busy.size >= 3)
        assertTrue(late.isNotEmpty())
        assertNotEquals(empty, busy)
        assertTrue(MochiIdleScript.phrases(0, 0, hour = 8).any { it.res == R.string.coucou_phrase_morning })
        // The one that counts carries the number.
        assertEquals(3, busy.first { it.res == R.string.coucou_phrase_busy_3 }.arg)
        // Step wraps instead of throwing.
        assertEquals(empty[0], MochiIdleScript.phraseAt(empty, empty.size))
        assertEquals(empty[1], MochiIdleScript.phraseAt(empty, 1))
        assertEquals(null, MochiIdleScript.phraseAt(emptyList(), 5))
    }

    @Test
    fun gestures_rest_with_reduced_motion_and_with_the_screen_off() {
        assertTrue(MochiIdleScript.gesturesAllowed(reducedMotion = false, screenOn = true))
        assertFalse(MochiIdleScript.gesturesAllowed(reducedMotion = true, screenOn = true))
        assertFalse(MochiIdleScript.gesturesAllowed(reducedMotion = false, screenOn = false))
    }

    @Test
    fun the_ring_closes_as_the_reminder_comes_due() {
        val now = 2_000_000_000_000L
        fun at(sec: Long) = now / 1000 + sec
        assertEquals(0f, MochiIdleScript.soonProgress(emptyList(), now), 0f)
        assertEquals(0f, MochiIdleScript.soonProgress(listOf(at(16 * 60)), now), 0f)
        assertEquals(0f, MochiIdleScript.soonProgress(listOf(at(-30)), now), 0f)
        val early = MochiIdleScript.soonProgress(listOf(at(14 * 60)), now)
        val late = MochiIdleScript.soonProgress(listOf(at(2 * 60)), now)
        assertTrue(early > 0f && late > early)
        assertEquals(1f, MochiIdleScript.soonProgress(listOf(at(0)), now), 0.001f)
        // The nearest one rules.
        assertEquals(late, MochiIdleScript.soonProgress(listOf(at(10 * 60), at(2 * 60)), now), 0.001f)
    }

    @Test
    fun desk_status_says_overdue_first() {
        assertEquals(MochiIdleScript.StatusKind.Clear, MochiIdleScript.statusKind(0, 0))
        assertEquals(MochiIdleScript.StatusKind.Todo, MochiIdleScript.statusKind(3, 0))
        assertEquals(MochiIdleScript.StatusKind.Overdue, MochiIdleScript.statusKind(3, 2))
    }

    @Test
    fun thought_emoji_follows_the_hour() {
        val rnd = Random(5)
        val night = (0 until 100).map { MochiIdleScript.thoughtFor(hour = 2, rnd = rnd) }
        assertTrue(night.any { it == "🌙" })
        val afternoon = (0 until 100).map { MochiIdleScript.thoughtFor(hour = 16, rnd = rnd) }
        assertTrue(afternoon.any { it == "☕" })
        assertTrue(afternoon.all { it.isNotEmpty() })
    }
}
