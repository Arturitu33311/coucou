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

/** When more than this much is waiting, silent chunks are skipped (the delay shrinks without a click in the speech). */
const val CATCH_UP_MS = 150

/** The most sound kept waiting for a speaker that is slow to start; older sound than this is given up. */
const val PENDING_MAX_MS = 3_000

/**
 * Sound that did not fit in the speaker's buffer, kept in order instead of thrown away. When a sound starts the output
 * (Bluetooth above all) takes a second or two to begin consuming; with nothing to hold it the first words, which arrive
 * meanwhile, were dropped (about two seconds lost at every start, measured on the phone). Pure: tested on the JVM.
 */
class PendingPcm(private val bytesPerMs: Int, private val maxMs: Int = PENDING_MAX_MS) {
    private val queue = java.util.ArrayDeque<ByteArray>()
    private var bytes = 0

    /** Chunks given up because more than [maxMs] was waiting. */
    var dropped = 0
        private set

    val isEmpty: Boolean get() = queue.isEmpty()
    val ms: Int get() = bytes / bytesPerMs

    fun add(chunk: ByteArray) {
        if (chunk.isEmpty()) return
        queue.addLast(chunk)
        bytes += chunk.size
        while (ms > maxMs) {
            bytes -= queue.removeFirst().size
            dropped++
        }
    }

    /**
     * Hands the oldest sound to [write] (it returns how many bytes it took, 0 when full, negative on an error) until it
     * takes no more. A chunk that errors is lost and the rest goes on. Returns the number of errors.
     */
    fun drain(write: (ByteArray) -> Int): Int {
        var errors = 0
        while (queue.isNotEmpty()) {
            val head = queue.removeFirst()
            bytes -= head.size
            val n = write(head)
            if (n < 0) {
                errors++
                continue
            }
            if (n < head.size) {
                val rest = head.copyOfRange(n, head.size)
                queue.addFirst(rest)
                bytes += rest.size
                break
            }
        }
        return errors
    }

    fun clear() {
        queue.clear()
        bytes = 0
    }
}
