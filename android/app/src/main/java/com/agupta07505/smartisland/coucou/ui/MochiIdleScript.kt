/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * Mochi's life when nothing is asking for him. Pure logic (no Android): when he naps, which little
 * gesture he does next, how long it lasts and what the desk says under his name.
 */

package com.agupta07505.smartisland.coucou.ui

import com.agupta07505.smartisland.R
import kotlin.random.Random

/** The small things he does on his own while calm. [None] is "between gestures". */
enum class IdleGesture { None, Yawn, Stretch, LookUp, LookAround, Hop, Whistle, Scratch, Wink, Think, Tickle }

object MochiIdleScript {
    /** Without a touch for this long he dozes off (an empty list alone no longer does it). */
    const val SLEEP_AFTER_S = 20 * 60L

    /** A task due within this window keeps him attentive. */
    const val SOON_MS = 15 * 60_000L

    fun idleAsleep(idleSeconds: Long): Boolean = idleSeconds >= SLEEP_AFTER_S

    /** [remindAts] are unix seconds. Due soon means in the next 15 min and not yet overdue. */
    fun isSoon(remindAts: List<Long>, nowMs: Long): Boolean =
        remindAts.any { it * 1000 in nowMs..(nowMs + SOON_MS) }

    /**
     * What he is when no event, reminder or music decides it: attentive for a task about to be due,
     * asleep at night or after 20 quiet minutes, otherwise awake and idling (with [next] gestures).
     */
    fun calmMood(night: Boolean, idleSeconds: Long, soonTask: Boolean): MochiMood = when {
        soonTask -> MochiMood.Listening
        night || idleAsleep(idleSeconds) -> MochiMood.Sleepy
        else -> MochiMood.Idle
    }

    /**
     * Whether he may do a little gesture now: not while the person has system animations off (they
     * asked for stillness) and not while the screen is off (nobody to see it, and it keeps the CPU
     * asleep).
     */
    fun gesturesAllowed(reducedMotion: Boolean, screenOn: Boolean): Boolean = !reducedMotion && screenOn

    /**
     * How far into the quarter hour before the next reminder it is, 0..1; 0 when none is that close.
     * Never below a sliver, so a ring that has just started is still seen. [remindAts] in unix seconds.
     */
    fun soonProgress(remindAts: List<Long>, nowMs: Long): Float {
        val next = remindAts.map { it * 1000 }.filter { it in nowMs..(nowMs + SOON_MS) }.minOrNull() ?: return 0f
        return (1f - (next - nowMs).toFloat() / SOON_MS).coerceIn(0.03f, 1f)
    }

    /** Pause between two gestures: calm, never a nervous tic. */
    fun gapMs(rnd: Random): Long = 6_000L + rnd.nextLong(0L, 9_001L)

    fun durationMs(g: IdleGesture): Long = when (g) {
        IdleGesture.None -> 0L
        IdleGesture.Yawn -> 1_800L
        IdleGesture.Stretch -> 1_600L
        IdleGesture.LookUp -> 1_400L
        IdleGesture.LookAround -> 2_200L
        IdleGesture.Hop -> 900L
        IdleGesture.Whistle -> 2_000L
        IdleGesture.Scratch -> 1_200L
        IdleGesture.Wink -> 700L
        IdleGesture.Think -> 3_500L
        IdleGesture.Tickle -> 1_200L
    }

    /** Gestures he picks from by himself (Tickle only answers a touch). */
    private val REPERTOIRE = listOf(
        IdleGesture.Yawn, IdleGesture.Stretch, IdleGesture.LookUp, IdleGesture.LookAround,
        IdleGesture.Hop, IdleGesture.Whistle, IdleGesture.Scratch, IdleGesture.Wink, IdleGesture.Think,
    )

    /**
     * The next gesture: never [IdleGesture.None], never the same as [prev]. Mornings lean toward
     * yawns and stretches, the afternoon toward daydreaming (the thought bubble).
     */
    fun next(prev: IdleGesture, hour: Int, allowThought: Boolean, rnd: Random): IdleGesture {
        val weighted = REPERTOIRE
            .filter { it != prev && (allowThought || it != IdleGesture.Think) }
            .map { g ->
                val w = when {
                    hour in 5..11 && (g == IdleGesture.Yawn || g == IdleGesture.Stretch) -> 4
                    hour in 14..17 && g == IdleGesture.Think -> 3
                    else -> 1
                }
                g to w
            }
        var pick = rnd.nextInt(weighted.sumOf { it.second })
        for ((g, w) in weighted) {
            if (pick < w) return g
            pick -= w
        }
        return weighted.last().first
    }

    /** What floats in his thought bubble. */
    fun thoughtFor(hour: Int, rnd: Random): String {
        val pool = buildList {
            addAll(listOf("💭", "⭐", "🎵", "🍡"))
            when (hour) {
                in 5..11 -> add("☀️")
                in 14..17 -> repeat(3) { add("☕") }
                in 0..4, 22, 23 -> repeat(3) { add("🌙") }
            }
        }
        return pool[rnd.nextInt(pool.size)]
    }

    /** A line Mochi says: a string resource and, for the ones that count something, the number. */
    data class Phrase(@androidx.annotation.StringRes val res: Int, val arg: Int? = null)

    /** What the expanded desk says as its headline when there is no timed task to show. */
    fun phrases(open: Int, overdue: Int, hour: Int): List<Phrase> {

        if (overdue > 0) return listOf(Phrase(R.string.coucou_phrase_late_1), Phrase(R.string.coucou_phrase_late_2), Phrase(R.string.coucou_phrase_late_3))
        val base = if (open == 0) {
            listOf(
                Phrase(R.string.coucou_phrase_clear_1), Phrase(R.string.coucou_phrase_clear_2), Phrase(R.string.coucou_phrase_clear_3),
                Phrase(R.string.coucou_phrase_clear_4), Phrase(R.string.coucou_phrase_clear_5),
            )
        } else {
            listOf(
                Phrase(R.string.coucou_phrase_busy_1), Phrase(R.string.coucou_phrase_busy_2), Phrase(R.string.coucou_phrase_busy_3, open),
                Phrase(R.string.coucou_phrase_busy_4), Phrase(R.string.coucou_phrase_busy_5),
            )
        }
        val extra = when (hour) {
            in 5..11 -> listOf(Phrase(R.string.coucou_phrase_morning))
            in 14..17 -> listOf(Phrase(R.string.coucou_phrase_afternoon))
            in 20..23 -> listOf(Phrase(R.string.coucou_phrase_evening))
            else -> emptyList()
        }
        return extra + base
    }

    /** The small grey line under the headline says one of these: how many things are open. */
    enum class StatusKind { Overdue, Clear, Todo }

    fun statusKind(open: Int, overdue: Int): StatusKind = when {
        overdue > 0 -> StatusKind.Overdue
        open == 0 -> StatusKind.Clear
        else -> StatusKind.Todo
    }

    /** The phrase for a step of the rotation; the step wraps. Null for an empty list. */
    fun phraseAt(list: List<Phrase>, step: Int): Phrase? =
        if (list.isEmpty()) null else list[Math.floorMod(step, list.size)]
}
