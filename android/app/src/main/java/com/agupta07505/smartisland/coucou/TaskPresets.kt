/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * Small pure helpers of the desk and the reminder card: where a snoozed reminder goes, and how a time
 * the parser understood is told back to the person before they confirm it.
 */

package com.agupta07505.smartisland.coucou

/** Where a snoozed reminder goes. */
object SnoozePresets {
    const val SHORT_S = 10 * 60L
    const val HOUR_S = 60 * 60L

    /**
     * Seconds from [nowSec] until [hour]:00 tomorrow, local time. [offset] gives the zone's offset in
     * seconds at a moment (so a change of clocks overnight is not off by an hour).
     */
    fun untilTomorrow(nowSec: Long, offset: (Long) -> Long, hour: Int = 9): Long {
        val localNow = nowSec + offset(nowSec)
        val dayStart = Math.floorDiv(localNow, 86_400L) * 86_400L
        val targetLocal = dayStart + 86_400L + hour * 3600L
        val targetUtc = targetLocal - offset(targetLocal - offset(nowSec))
        return (targetUtc - nowSec).coerceAtLeast(60L)
    }
}

enum class WhenDay { Today, Tomorrow, Other }

/** A time as the person will read it back: which day (today, tomorrow, another) and the clock time. */
data class WhenLabel(val day: WhenDay, val hour: Int, val minute: Int, val atSec: Long)

object TaskPreview {
    /** Null when the sentence carries no time (the task will have no reminder). */
    fun label(whenSec: Long?, nowSec: Long, offset: (Long) -> Long): WhenLabel? {
        val at = whenSec ?: return null
        val local = at + offset(at)
        val nowLocal = nowSec + offset(nowSec)
        val diff = Math.floorDiv(local, 86_400L) - Math.floorDiv(nowLocal, 86_400L)
        val secs = Math.floorMod(local, 86_400L)
        val day = when (diff) {
            0L -> WhenDay.Today
            1L -> WhenDay.Tomorrow
            else -> WhenDay.Other
        }
        return WhenLabel(day, (secs / 3600).toInt(), (secs % 3600 / 60).toInt(), at)
    }
}
