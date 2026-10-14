package com.agupta07505.smartisland.coucou

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The same cases as Coucou's tasks.rs, so the phone and the PC read a sentence the same way. */
class TaskParserTest {
    private val day = 86_400L
    private val now = 100 * day + 10 * 3600 // 10:00 on day 100, UTC

    private fun p(s: String, off: Long = 0) = TaskParser.parseWhen(s, now) { off }

    @Test
    fun a_relative_time_is_taken_out_of_the_title() {
        assertEquals(TaskParser.Parsed("tomar agua", now + 1200), p("tomar agua en 20 min"))
        assertEquals(TaskParser.Parsed("stretch", now + 7200), p("stretch in 2 hours"))
        assertEquals(TaskParser.Parsed("llamar a mamá", now + 300), p("llamar en 5min a mamá"))
        assertEquals(TaskParser.Parsed("revisar", now + day), p("revisar en 1 día"))
    }

    @Test
    fun a_time_of_day_is_today_if_ahead_and_tomorrow_if_passed() {
        assertEquals(TaskParser.Parsed("llamar a mamá", 100 * day + 17 * 3600 + 1800), p("llamar a mamá a las 17:30"))
        assertEquals(TaskParser.Parsed("standup", 101 * day + 9 * 3600), p("standup 9:00"))
        assertEquals(TaskParser.Parsed("call", 100 * day + 17 * 3600), p("call at 5pm"))
        assertEquals(TaskParser.Parsed("call", 100 * day + 17 * 3600), p("call at 5 pm"))
        assertEquals(TaskParser.Parsed("cena", 100 * day + 21 * 3600), p("cena a las 21"))
    }

    @Test
    fun a_day_word_moves_the_day() {
        assertEquals(TaskParser.Parsed("reunión", 101 * day + 9 * 3600), p("reunión mañana a las 9"))
        assertEquals(TaskParser.Parsed("dentista", 101 * day + 9 * 3600), p("dentista tomorrow"))
        assertEquals(TaskParser.Parsed("gym", 100 * day + 8 * 3600), p("gym hoy 8:00"))
    }

    @Test
    fun ordinary_words_are_not_a_time() {
        assertEquals(TaskParser.Parsed("comprar pan", null), p("comprar pan"))
        assertEquals(TaskParser.Parsed("ir a la tienda", null), p("ir a la tienda"))
        assertEquals(TaskParser.Parsed("pagar en efectivo", null), p("pagar en efectivo"))
        assertEquals(TaskParser.Parsed("estudiar tema 3", null), p("estudiar tema 3"))
        assertEquals(TaskParser.Parsed("a las 25:00 algo", null), p("a las 25:00 algo"))
        // A title is never left empty.
        assertEquals(TaskParser.Parsed("en 20 min", now + 1200), p("en 20 min"))
    }

    @Test
    fun the_time_zone_is_the_local_one() {
        // UTC+2: at 10:00 UTC it is 12:00 there; 17:30 local is 15:30 UTC.
        assertEquals(100 * day + 15 * 3600 + 1800, p("llamar a las 17:30", 2 * 3600).whenSec)
    }

    @Test
    fun no_time_gives_no_reminder() {
        assertNull(p("sin hora").whenSec)
    }
}
