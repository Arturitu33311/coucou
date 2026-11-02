/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.audiolink

enum class PlaybackAction { Play, Wait, Drop }

/**
 * What to do with the next chunk of sound, from how much is already queued for the speaker (in ms).
 * Before the first write it waits until [PREBUFFER_MS] has built up, so a tiny network hiccup does not
 * become a stutter; once playing it writes at once; and if more than [MAX_QUEUE_MS] piled up (the network
 * stalled and then caught up) the newest chunk is dropped, so the delay does not grow for good.
 *
 * The margins are wide on purpose: a phone's Wi-Fi that dozes between sounds hands the packets over in bursts
 * of a few hundred ms, and with a 120 ms buffer and a 200 ms limit every burst was dropped (speech that cuts and
 * skips ahead). Half a second of limit costs a little delay, which a music relay does not mind.
 */
fun playbackPlan(queuedMs: Int, started: Boolean, maxQueueMs: Int = MAX_QUEUE_MS): PlaybackAction = when {
    queuedMs > maxQueueMs -> PlaybackAction.Drop
    !started && queuedMs < PREBUFFER_MS -> PlaybackAction.Wait
    else -> PlaybackAction.Play
}

const val PREBUFFER_MS = 120
const val MAX_QUEUE_MS = 500

/** How much sound the speaker's buffer holds (and so about the delay it adds) unless the system needs more. */
const val BUFFER_MS = 200

/** The buffer to ask the system for, given the smallest it accepts (in ms). */
fun trackBufferMs(minBufferMs: Int): Int = maxOf(minBufferMs, BUFFER_MS)

/** How much may be queued before chunks are dropped, given the buffer: always above it, so the buffer can fill. */
fun queueLimitMs(bufferMs: Int): Int = maxOf(MAX_QUEUE_MS, bufferMs + 40)
