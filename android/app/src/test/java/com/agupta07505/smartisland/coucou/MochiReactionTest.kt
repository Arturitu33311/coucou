package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.ui.MochiReactions
import com.agupta07505.smartisland.coucou.ui.MochiMood as M
import com.agupta07505.smartisland.model.IslandMode
import com.agupta07505.smartisland.model.IslandNotification
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MochiReactionTest {
    // What surprises him (and what must not), how the surprise latches, and what makes him dance.
    private val now = 1_000_000_000L

    private fun note(
        key: String,
        mode: IslandMode = IslandMode.Notification,
        at: Long = now,
        playing: Boolean = false,
    ) = IslandNotification(
        key = key,
        packageName = "com.example",
        appName = "Example",
        title = "t",
        text = "x",
        timeMillis = at,
        mode = mode,
        mediaIsPlaying = playing,
    )

    @Test
    fun news_surprises_but_his_own_cards_do_not() {
        assertTrue(MochiReactions.freshArrival(listOf(note("w1")), now))
        assertFalse(MochiReactions.freshArrival(listOf(note("coucou_desk", IslandMode.Desk)), now))
        assertFalse(MochiReactions.freshArrival(listOf(note("coucou_approval_1", IslandMode.Approval)), now))
        assertFalse(MochiReactions.freshArrival(listOf(note("coucou_agent", IslandMode.Agent)), now))
    }

    @Test
    fun old_news_is_not_news() {
        assertFalse(MochiReactions.freshArrival(listOf(note("w1", at = now - 60_000)), now))
        assertTrue(MochiReactions.freshArrival(listOf(note("w1", at = now - 60_000)), now, windowMs = 120_000))
    }

    @Test
    fun surprise_latches_and_never_goes_backwards() {
        assertEquals(now + MochiReactions.SURPRISE_MS, MochiReactions.latchSurprise(0L, listOf(note("w1")), now))
        // More news extends it; silence keeps what was latched.
        assertEquals(
            now + MochiReactions.SURPRISE_MS + 1_000,
            MochiReactions.latchSurprise(now + MochiReactions.SURPRISE_MS + 1_000, listOf(note("w2")), now),
        )
        assertEquals(123L, MochiReactions.latchSurprise(123L, emptyList(), now))
    }

    @Test
    fun only_playing_music_makes_him_dance() {
        val playing = note("m", IslandMode.Music, playing = true)
        val paused = note("m", IslandMode.Music, playing = false)
        assertTrue(MochiReactions.isMusicPlaying(listOf(playing)))
        assertFalse(MochiReactions.isMusicPlaying(listOf(paused)))
        assertFalse(MochiReactions.isMusicPlaying(listOf(note("w1"))))
    }

    @Test
    fun scene_combines_everything() {
        val s = MochiReactions.scene(
            listOf(note("m", IslandMode.Music, playing = true)),
            now, now + 1_000, now + 1_000, now + 1_000, now + 1_000,
        )
        assertTrue(s.musicPlaying && s.surprised && s.greeted && s.glancing && s.celebrating)
        assertFalse(s.batteryLow)
        val low = MochiReactions.scene(
            listOf(note("b", IslandMode.Battery, at = now).copy(category = "battery_low", text = "15%")),
            now, 0L, 0L, 0L, 0L,
        )
        assertTrue(low.batteryLow)
        assertFalse(low.musicPlaying || low.surprised || low.greeted || low.glancing || low.celebrating)
        val calm = MochiReactions.scene(emptyList(), now, 0L, 0L, 0L, 0L)
        assertFalse(calm.musicPlaying || calm.surprised || calm.batteryLow || calm.greeted || calm.glancing || calm.celebrating)
    }

    @Test
    fun glance_outlives_surprise() {
        // News opens both; the pop (3 s) closes first, the stare (6 s) stays watching.
        assertEquals(now + MochiReactions.GLANCE_MS, MochiReactions.latchGlance(0L, listOf(note("w1")), now))
        assertEquals(999L, MochiReactions.latchGlance(999L, emptyList(), now))
        val s = MochiReactions.scene(listOf(note("w1")), now + 4_000, now + 3_000, 0L, now + 6_000, 0L)
        assertFalse(s.surprised)
        assertTrue(s.glancing)
    }

    @Test
    fun celebration_opens_only_on_emptying() {
        assertEquals(now + MochiReactions.CELEBRATE_MS, MochiReactions.latchCelebration(0L, 2, 0, now))
        // Finishing one of several is no celebration; idling empty is none either.
        assertEquals(7L, MochiReactions.latchCelebration(7L, 3, 2, now))
        assertEquals(7L, MochiReactions.latchCelebration(7L, 0, 0, now))
    }

    @Test
    fun night_is_midnight_to_six() {
        // Built in the phone's own zone (the one isNight reads), so boundaries hold anywhere.
        fun localAt(day: Int, hour: Int) = java.util.Calendar.getInstance().apply {
            set(2026, java.util.Calendar.OCTOBER, day, hour, 0, 0)
            set(java.util.Calendar.MILLISECOND, 0)
        }.timeInMillis
        assertTrue(MochiReactions.isNight(localAt(9, 0)))
        assertTrue(MochiReactions.isNight(localAt(9, 3)))
        assertTrue(MochiReactions.isNight(localAt(9, 5)))
        assertFalse(MochiReactions.isNight(localAt(9, 6)))
        assertFalse(MochiReactions.isNight(localAt(9, 12)))
        assertFalse(MochiReactions.isNight(localAt(9, 23)))
    }

    @Test
    fun low_battery_is_announced_or_near_empty() {
        // The system's own low-battery card…
        assertTrue(MochiReactions.isBatteryLow(listOf(note("b", IslandMode.Battery).copy(category = "battery_low", text = "15%"))))
        // …or anything at 20% or less, just in case…
        assertTrue(MochiReactions.isBatteryLow(listOf(note("b", IslandMode.Battery).copy(text = "20%"))))
        assertTrue(MochiReactions.isBatteryLow(listOf(note("b", IslandMode.Battery).copy(title = "Low Battery", text = "19%"))))
        // …but a healthy charging battery is not tired.
        assertFalse(MochiReactions.isBatteryLow(listOf(note("b", IslandMode.Battery).copy(category = "battery_charging", text = "80%"))))
        // …and neither is a "low" word outside the battery card.
        assertFalse(MochiReactions.isBatteryLow(listOf(note("w1").copy(text = "low stocks"))))
    }

    @Test
    fun his_face_follows_jinx_when_she_speaks_and_nobody_else() {
        val speaking = note("coucou_jinx", IslandMode.JinxMsg).copy(category = "mochi:Happy:info")
        assertEquals(com.agupta07505.smartisland.coucou.ui.MochiMood.Happy, MochiReactions.jinxMood(listOf(speaking)))
        // Nothing from her: nothing to wear. Claude Code's card (or anything else) is not her.
        assertEquals(null, MochiReactions.jinxMood(emptyList()))
        assertEquals(null, MochiReactions.jinxMood(listOf(note("coucou_agent", IslandMode.Agent))))
        // And the scene he reads no longer carries Claude Code's work unless asked to.
        val scene = MochiReactions.scene(listOf(speaking), now, 0L, 0L, 0L, 0L, jinxMood = MochiReactions.jinxMood(listOf(speaking)))
        assertEquals(com.agupta07505.smartisland.coucou.ui.MochiMood.Happy, scene.jinxMood)
        assertFalse(scene.working)
    }

    @Test
    fun greeting_latches_like_surprise() {
        assertEquals(now + MochiReactions.GREET_MS, MochiReactions.latchGreeting(0L, now))
        assertEquals(now + 9_000, MochiReactions.latchGreeting(now + 9_000, now))
    }

    @Test fun `when all is calm he wears Jinx's background mood, but night and reminders keep theirs`() {
        assertEquals(M.Sad, MochiReactions.calmOrJinx(M.Idle, M.Sad))
        assertEquals(M.Idle, MochiReactions.calmOrJinx(M.Idle, null))
        assertEquals(M.Sleepy, MochiReactions.calmOrJinx(M.Sleepy, M.Happy))
        assertEquals(M.Alert, MochiReactions.calmOrJinx(M.Alert, M.Happy))
    }
}

