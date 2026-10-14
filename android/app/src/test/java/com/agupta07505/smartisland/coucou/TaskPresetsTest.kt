package com.agupta07505.smartisland.coucou

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TaskPresetsTest {
    private val utc: (Long) -> Long = { 0L }
    private val minus3: (Long) -> Long = { -3 * 3600L }

    // 2026-10-13 10:30:00 UTC
    private val now = 1_791_887_400L

    @Test
    fun tomorrow_morning_is_the_next_days_nine() {
        val d = SnoozePresets.untilTomorrow(now, utc)
        // 10:30 -> tomorrow 09:00 = 22 h 30 min
        assertEquals(22 * 3600L + 30 * 60L, d)
    }

    @Test
    fun tomorrow_morning_follows_the_zone() {
        // 10:30 UTC is 07:30 at -3: tomorrow 09:00 there is 25 h 30 min away.
        assertEquals(25 * 3600L + 30 * 60L, SnoozePresets.untilTomorrow(now, minus3))
    }

    @Test
    fun late_night_still_means_the_next_calendar_day() {
        // 23:50: tomorrow 09:00 is 9 h 10 min away.
        val late = (now / 86_400L) * 86_400L + 23 * 3600L + 50 * 60L
        assertEquals(9 * 3600L + 10 * 60L, SnoozePresets.untilTomorrow(late, utc))
        // 00:10: the same calendar day is already "today", tomorrow 09:00 is 32 h 50 min away.
        val early = (now / 86_400L) * 86_400L + 10 * 60L
        assertEquals(32 * 3600L + 50 * 60L, SnoozePresets.untilTomorrow(early, utc))
    }

    @Test
    fun editing_rewords_and_only_a_new_time_moves_the_reminder() {
        val t = CoucouTask(id = "a", title = "old", remindAt = 5_000, notified = true, created = 1, updatedAt = 10)
        val reworded = TaskLogic.edit(listOf(t), "a", "  new title  ", null, now = 99).single()
        assertEquals("new title", reworded.title)
        assertEquals(5_000L, reworded.remindAt)
        assertTrue(reworded.notified)
        assertEquals(99L, reworded.updatedAt)
        val moved = TaskLogic.edit(listOf(t), "a", "new title", 9_000, now = 100).single()
        assertEquals(9_000L, moved.remindAt)
        assertEquals(false, moved.notified)
    }

    @Test
    fun a_task_cannot_be_edited_to_nothing() {
        val t = CoucouTask(id = "a", title = "old", created = 1)
        val failed = runCatching { TaskLogic.edit(listOf(t), "a", "   ", null) }.isFailure
        assertTrue(failed)
    }

    @Test
    fun a_parsed_time_is_told_back_by_day() {
        assertNull(TaskPreview.label(null, now, utc))
        val today = TaskPreview.label(now + 3600, now, utc)!!
        assertEquals(WhenDay.Today, today.day)
        assertEquals(11, today.hour)
        assertEquals(30, today.minute)
        assertEquals(WhenDay.Tomorrow, TaskPreview.label(now + 86_400, now, utc)!!.day)
        assertEquals(WhenDay.Other, TaskPreview.label(now + 3 * 86_400, now, utc)!!.day)
        assertTrue(TaskPreview.label(now - 3600, now, utc)!!.day == WhenDay.Today)
    }
}
