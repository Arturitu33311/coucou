/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 * The logic is a port of Coucou's tasks.rs (MIT), for the same behaviour on the phone and the PC.
 */

package com.agupta07505.smartisland.coucou

/**
 * Reads a time out of a sentence: "tomar agua en 20 min" → ("tomar agua", now + 20 min);
 * "reunión mañana a las 9" → ("reunión", tomorrow 09:00). A time of day that has already
 * passed today means tomorrow. Spanish and English.
 */
object TaskParser {
    data class Parsed(val title: String, val whenSec: Long?)

    /**
     * A day picked on the calendar (epoch seconds of its local midnight) plus a time: the time
     * of day the sentence carried ("viernes a las 17:30" → Friday 17:30), or [defaultHour] when
     * it carried none. `offset` is this phone's zone offset in seconds east of UTC at a time.
     */
    fun withDay(dayStartSec: Long, parsedWhenSec: Long?, defaultHour: Int, offset: (Long) -> Long): Long {
        val atMin = if (parsedWhenSec != null) {
            Math.floorMod(parsedWhenSec + offset(parsedWhenSec), 86_400L) / 60
        } else {
            defaultHour * 60L
        }
        // Two passes so a daylight-saving jump inside the day still lands on the right wall time.
        var t = dayStartSec + atMin * 60 - offset(dayStartSec + 12 * 3600)
        t = dayStartSec + atMin * 60 - offset(t)
        return t
    }

    private val SPACES = Regex("\\s+")

    /** "14" / "10min" → its number and the letters after it. */
    private fun numberAndUnit(tok: String): Pair<Long, String>? {
        val digits = tok.takeWhile { it in '0'..'9' }.length
        if (digits == 0 || digits > 4) return null
        return tok.substring(0, digits).toLong() to tok.substring(digits)
    }

    private fun unitSeconds(unit: String): Long? = when (unit) {
        "m", "min", "mins", "minuto", "minutos", "minute", "minutes" -> 60L
        "h", "hr", "hrs", "hora", "horas", "hour", "hours" -> 3600L
        "d", "dia", "día", "dias", "días", "day", "days" -> 86_400L
        else -> null
    }

    private fun isAsciiAlpha(c: Char) = c in 'a'..'z' || c in 'A'..'Z'

    /** "17:30", "9:05pm", "5" followed by "pm" → (minutes since midnight, words used). */
    private fun clock(tok: String, next: String?): Pair<Int, Int>? {
        val idx = tok.indexOfFirst { isAsciiAlpha(it) }
        val hm = if (idx >= 0) tok.substring(0, idx) else tok
        var suffix = if (idx >= 0) tok.substring(idx) else ""
        val h: Long
        val m: Long
        if (':' in hm) {
            val (a, b) = hm.split(':', limit = 2)
            h = a.toLongOrNull() ?: return null
            m = b.toLongOrNull() ?: return null
        } else {
            h = hm.toLongOrNull() ?: return null
            m = 0
        }
        var used = 1
        if (suffix.isEmpty() && (next == "am" || next == "pm")) {
            suffix = next
            used = 2
        }
        val hour = when {
            suffix.isEmpty() -> h
            suffix == "am" && h in 1..12 -> h % 12
            suffix == "pm" && h in 1..12 -> h % 12 + 12
            else -> return null
        }
        if (hour !in 0..23 || m !in 0..59) return null
        return (hour * 60 + m).toInt() to used
    }

    /** `offset` gives this phone's time-zone offset in seconds east of UTC at a given time. */
    fun parseWhen(text: String, now: Long, offset: (Long) -> Long): Parsed {
        val words = text.trim().split(SPACES).filter { it.isNotEmpty() }
        val low = text.lowercase().trim().split(SPACES).filter { it.isNotEmpty() }
        val used = BooleanArray(words.size)
        var relative: Long? = null
        var dayAdd: Long? = null
        var minutes: Int? = null

        var i = 0
        while (i < low.size) {
            val w = low[i]
            val next = low.getOrNull(i + 1)
            // "en 20 min", "in 2 hours", "en 20min"
            if ((w == "en" || w == "in") && relative == null) {
                val nu = next?.let { numberAndUnit(it) }
                if (nu != null) {
                    var unit = nu.second
                    var span = 2
                    if (unit.isEmpty()) {
                        unit = low.getOrNull(i + 2) ?: ""
                        span = 3
                    }
                    val seconds = unitSeconds(unit)
                    if (seconds != null) {
                        relative = nu.first * seconds
                        for (k in i until minOf(i + span, low.size)) used[k] = true
                        i += span
                        continue
                    }
                }
            }
            if ((w == "mañana" || w == "tomorrow") && dayAdd == null) {
                dayAdd = 1
                used[i] = true
            } else if ((w == "hoy" || w == "today") && dayAdd == null) {
                dayAdd = 0
                used[i] = true
            } else if (minutes == null) {
                // "a las 17:30", "a la 1", "at 5pm", or a bare "17:30"
                val (lead, at) = when {
                    w == "a" && (next == "las" || next == "la") -> 2 to i + 2
                    w == "at" || (w == "a" && next != null) -> 1 to i + 1
                    else -> 0 to i
                }
                val candidate = low.getOrNull(at) ?: ""
                // "a las 9" is a time by itself; otherwise it needs a colon or am/pm ("9:00", "5pm"),
                // so that "tema 3" or "look at 5 things" stay words.
                val marked = ':' in candidate ||
                    (candidate.any { isAsciiAlpha(it) } && candidate.firstOrNull()?.let { it in '0'..'9' } == true) ||
                    (candidate.all { it in '0'..'9' } && low.getOrNull(at + 1).let { it == "am" || it == "pm" })
                if (lead == 2 || marked) {
                    val c = clock(candidate, low.getOrNull(at + 1))
                    if (c != null) {
                        minutes = c.first
                        for (k in i until minOf(at + c.second, low.size)) used[k] = true
                        i = at + c.second
                        continue
                    }
                }
            }
            i += 1
        }

        val whenSec: Long? = when {
            relative != null -> now + relative
            minutes != null || dayAdd != null -> {
                val off = offset(now)
                val today = Math.floorDiv(now + off, 86_400L)
                val at = (minutes ?: (9 * 60)).toLong()
                fun ts(day: Long) = day * 86_400L + at * 60 - off
                var t = ts(today + (dayAdd ?: 0))
                // "a las 8" at 10:00 is tomorrow's 8, unless the day was said.
                if (t <= now && dayAdd == null) t = ts(today + 1)
                t
            }
            else -> null
        }

        val title = words.filterIndexed { idx, _ -> !used[idx] }.joinToString(" ")
        return Parsed(if (title.isBlank()) text.trim() else title, whenSec)
    }
}
