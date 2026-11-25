/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.audiolink

enum class PlaybackAction { Play, Wait, Drop }

/**
 * What to do with the next chunk of sound, from how much is already queued for the speaker (in ms).
 * Before the first write it waits until [PREBUFFER_MS] has built up, so a tiny network hiccup does not
 * become a stutter; once playing it writes at once; and if more than [MAX_QUEUE_MS] piled up (the network
 * stalled and then caught up) the oldest chunk is dropped, so the delay does not grow for good.
 */
fun playbackPlan(queuedMs: Int, started: Boolean, maxQueueMs: Int = MAX_QUEUE_MS): PlaybackAction = when {
    queuedMs > maxQueueMs -> PlaybackAction.Drop
    !started && queuedMs < PREBUFFER_MS -> PlaybackAction.Wait
    else -> PlaybackAction.Play
}

const val PREBUFFER_MS = 60
const val MAX_QUEUE_MS = 200

/** How much sound the speaker's buffer holds (and so about the delay it adds) unless the system needs more. */
const val BUFFER_MS = 120

/** The buffer to ask the system for, given the smallest it accepts (in ms). */
fun trackBufferMs(minBufferMs: Int): Int = maxOf(minBufferMs, BUFFER_MS)

/** How much may be queued before chunks are dropped, given the buffer: always above it, so the buffer can fill. */
fun queueLimitMs(bufferMs: Int): Int = maxOf(MAX_QUEUE_MS, bufferMs + 40)

/** How much the speaker buffer grows after a real cut, and how long without cuts before it shrinks again. */
const val TUNER_STEP_MS = 80
const val TUNER_DECAY_AFTER_MS = 60_000L

/** The buffer grows at most this often, so one burst of drops cannot send it to the maximum at once. */
const val TUNER_MIN_GAP_MS = 2_000L

/** The most the speaker buffer may grow to (the track is created with this capacity). Past it the delay would be too long. */
const val BUFFER_CAPACITY_MS = 600

/**
 * An adaptive speaker buffer. A bad network (Wi-Fi, a relayed Tailscale path) delivers sound in bursts with gaps; a small
 * buffer then runs dry and the system waits to refill it, which is heard as the audio cutting out. This starts small (low
 * delay) and grows by [stepMs] each time the speaker really ran dry, up to [capacityMs], then shrinks by half a step after
 * [decayAfterMs] without cuts, so a good network keeps the low delay.
 *
 * Only a cut that happened while sound was arriving counts ([lastChunkLoud]): the laptop does not send digital silence, so
 * the speaker also runs dry when nothing is playing on the PC, and that must not make the buffer grow.
 * Pure, so it can be tested without a device.
 */
class JitterTuner(
    private val baseMs: Int,
    private val capacityMs: Int = BUFFER_CAPACITY_MS,
    private val stepMs: Int = TUNER_STEP_MS,
    private val decayAfterMs: Long = TUNER_DECAY_AFTER_MS,
    private val minGapMs: Long = TUNER_MIN_GAP_MS,
) {
    var bufferMs: Int = baseMs
        private set
    private var seenUnderruns = 0
    private var seenOverflows = 0
    private var lastGrowAt = Long.MIN_VALUE / 2
    private var lastProblemAt = 0L

    /**
     * The two signs that the buffer is too small for this network: the speaker ran dry (`underruns`, the track's running
     * count; only when sound was arriving) and a burst did not fit (`overflows`, a running count of chunks the buffer
     * refused). Measured on a phone: 43 to 55 chunks a second arriving where 50 are expected, and drops in a quarter of
     * the seconds, so a small buffer does both. It grows at most once per [minGapMs], so one burst of drops does not
     * send it to the maximum. Returns the new buffer size in ms when it changed, else null.
     */
    fun update(underruns: Int, overflows: Int, nowMs: Long, lastChunkLoud: Boolean): Int? {
        val cut = underruns > seenUnderruns && lastChunkLoud
        val burst = overflows > seenOverflows
        seenUnderruns = maxOf(seenUnderruns, underruns)
        seenOverflows = maxOf(seenOverflows, overflows)
        if (cut || burst) {
            lastProblemAt = nowMs
            if (nowMs - lastGrowAt < minGapMs) return null
            val grown = minOf(capacityMs, bufferMs + stepMs)
            if (grown == bufferMs) return null
            lastGrowAt = nowMs
            bufferMs = grown
            return grown
        }
        if (bufferMs > baseMs && nowMs - lastProblemAt >= decayAfterMs) {
            lastProblemAt = nowMs
            bufferMs = maxOf(baseMs, bufferMs - stepMs / 2)
            return bufferMs
        }
        return null
    }
}
