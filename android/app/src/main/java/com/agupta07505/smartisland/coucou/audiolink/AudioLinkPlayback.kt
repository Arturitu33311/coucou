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
fun playbackPlan(queuedMs: Int, started: Boolean): PlaybackAction = when {
    queuedMs > MAX_QUEUE_MS -> PlaybackAction.Drop
    !started && queuedMs < PREBUFFER_MS -> PlaybackAction.Wait
    else -> PlaybackAction.Play
}

const val PREBUFFER_MS = 60
const val MAX_QUEUE_MS = 200
