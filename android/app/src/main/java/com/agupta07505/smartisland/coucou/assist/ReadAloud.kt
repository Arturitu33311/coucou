/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Context
import com.agupta07505.smartisland.coucou.phone.PhoneSettings
import com.agupta07505.smartisland.coucou.presence.JinxChatScreen
import com.agupta07505.smartisland.coucou.presence.PresenceMsg
import java.util.Calendar

/** What the read-aloud has already spent: the day (days since 1970), how many today, and when the last one was. */
data class ReadAloudState(val day: Long = -1, val count: Int = 0, val lastAtMs: Long = 0)

/**
 * When a notice of Jinx is read aloud on the phone: the laptop's `jinx-voice-alert` rules, the same ones. Not in the
 * quiet hours (23–07), at most [MAX_PER_DAY] a day, [COOLDOWN_MS] between two (an alert skips that wait), nothing that
 * is already old, and nothing while her chat is on screen (he is reading it).
 */
object ReadAloudPolicy {
    const val MAX_PER_DAY = 8
    const val COOLDOWN_MS = 20 * 60_000L
    const val MAX_AGE_S = 3600L

    fun quiet(hour: Int): Boolean = hour >= 23 || hour < 7

    fun allow(enabled: Boolean, m: PresenceMsg, nowMs: Long, hour: Int, day: Long, st: ReadAloudState, chatVisible: Boolean): Boolean {
        if (!enabled || chatVisible || quiet(hour)) return false
        if (nowMs / 1000 - m.ts > MAX_AGE_S) return false
        val today = if (st.day == day) st.count else 0
        if (today >= MAX_PER_DAY) return false
        return m.kind == "alert" || nowMs - st.lastAtMs >= COOLDOWN_MS
    }

    fun spent(st: ReadAloudState, nowMs: Long, day: Long): ReadAloudState =
        ReadAloudState(day, (if (st.day == day) st.count else 0) + 1, nowMs)
}

object ReadAloud {
    private fun prefs(c: Context) = c.applicationContext.getSharedPreferences("coucou_read_aloud", Context.MODE_PRIVATE)

    /** Called with the notices that just reached the island; reads the newest one if the rules allow. */
    fun consider(context: Context, fresh: List<PresenceMsg>, nowS: Long) {
        val m = fresh.lastOrNull() ?: return
        if (!PhoneSettings.readAloud(context)) return
        val nowMs = nowS * 1000
        val cal = Calendar.getInstance().apply { timeInMillis = nowMs }
        val day = nowMs / 86_400_000L
        val p = prefs(context)
        val st = ReadAloudState(p.getLong("day", -1), p.getInt("count", 0), p.getLong("last", 0))
        if (!ReadAloudPolicy.allow(true, m, nowMs, cal.get(Calendar.HOUR_OF_DAY), day, st, JinxChatScreen.visible)) return
        val next = ReadAloudPolicy.spent(st, nowMs, day)
        p.edit().putLong("day", next.day).putInt("count", next.count).putLong("last", next.lastAtMs).apply()
        // The written text is what the island shows; her voice gets the version she wrote for it (with the audio tags).
        JinxSpeech.say(context, m.text, tagged = m.speech ?: m.text)
    }
}
