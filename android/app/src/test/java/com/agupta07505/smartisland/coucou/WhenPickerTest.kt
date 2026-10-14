package com.agupta07505.smartisland.coucou

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.util.Calendar
import java.util.TimeZone

class WhenPickerTest {
    // A picked day keeps the sentence's time of day; without one it takes the picked hour (UTC pin).
    private val utc = TimeZone.getTimeZone("UTC")
    private val offset: (Long) -> Long = { 0L }

    private fun utcSec(year: Int, month: Int, day: Int, hour: Int, min: Int = 0): Long =
        Calendar.getInstance(utc).apply {
            set(year, month - 1, day, hour, min, 0)
            set(Calendar.MILLISECOND, 0)
        }.timeInMillis / 1000

    @Test
    fun picked_day_keeps_the_sentence_time() {
        // Thursday 2026-10-08 10:00 UTC; "a las 17:30" reads 17:30.
        val now = utcSec(2026, 10, 8, 10)
        val parsed = TaskParser.parseWhen("llamar a las 17:30", now, offset)
        val friday = utcSec(2026, 10, 9, 0)
        assertEquals(utcSec(2026, 10, 9, 17, 30), TaskParser.withDay(friday, parsed.whenSec, 9, offset))
        // The time words are still eaten from the title.
        assertEquals("llamar", parsed.title)
    }

    @Test
    fun picked_day_without_sentence_time_uses_the_picked_hour() {
        val parsed = TaskParser.parseWhen("comprar pan", utcSec(2026, 10, 8, 10), offset)
        assertNull(parsed.whenSec)
        val friday = utcSec(2026, 10, 9, 0)
        assertEquals(utcSec(2026, 10, 9, 18), TaskParser.withDay(friday, null, 18, offset))
    }

    @Test
    fun picked_morning_time_stays_that_morning() {
        // "a las 8" at 10:00 already means tomorrow 08:00; on a picked day it is that day 08:00.
        val now = utcSec(2026, 10, 8, 10)
        val parsed = TaskParser.parseWhen("riego a las 8", now, offset)
        val monday = utcSec(2026, 10, 12, 0)
        assertEquals(utcSec(2026, 10, 12, 8), TaskParser.withDay(monday, parsed.whenSec, 9, offset))
    }
}
