/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.audiolink

import java.util.concurrent.LinkedBlockingQueue

/**
 * Lines for the laptop, written by one thread of their own. The watchers report changes on the main thread,
 * where a network write is forbidden (and, worse, a refused write would leave the ssh stream's counters out of
 * step); so a change is only queued here and this thread does the writing.
 */
class Outbox(private val write: (String) -> Unit) {
    private val queue = LinkedBlockingQueue<String>(QUEUE)
    @Volatile private var open = true
    private val thread = Thread({
        try {
            while (open) {
                val line = queue.take()
                if (line === STOP) break
                try {
                    write(line)
                } catch (_: Exception) {
                    // The link broke: the session notices on its own. Keep draining so post() never blocks.
                }
            }
        } catch (_: InterruptedException) {
        }
    }, "coucou-audiolink-out").apply { isDaemon = true; start() }

    /** Queues a line. Never blocks, never throws; when the queue is full the oldest line goes (the newest state wins). */
    fun post(line: String) {
        if (!open) return
        while (!queue.offer(line)) queue.poll()
    }

    fun close() {
        open = false
        queue.clear()
        queue.offer(STOP)
    }

    private companion object {
        const val QUEUE = 64
        val STOP = String()
    }
}

/** Seconds the headphones may be missing from the phone before the session is given up (the laptop gives its speakers back after 5). */
const val LEAVE_AFTER_MS = 7_000L

/** True when the headphones have been gone from the phone for long enough that holding the link open is pointless. */
fun shouldLeave(goneSinceMs: Long?, nowMs: Long): Boolean = goneSinceMs != null && nowMs - goneSinceMs >= LEAVE_AFTER_MS

enum class GateStep {
    /** Start the speaker over (stop, flush) and ask for focus. */
    Reset,

    /** Ask for audio focus; play only once it is granted. */
    AskFocus,

    /** Play this chunk. */
    Play,
}

/**
 * Decides, chunk by chunk, whether the phone may play: the laptop sends sound only while it has some, so audio focus
 * is taken when sound starts (not when the link opens, which would pause the phone's own music for nothing), and
 * everything starts over after a pause in the sound.
 */
class PlayerGate(private val gapMs: Long = 400) {
    private var hasFocus = false
    private var last = -1L

    fun next(nowMs: Long): GateStep {
        val gap = last >= 0 && nowMs - last > gapMs
        last = nowMs
        return when {
            gap -> {
                hasFocus = false
                GateStep.Reset
            }
            !hasFocus -> GateStep.AskFocus
            else -> GateStep.Play
        }
    }

    fun focusGranted() {
        hasFocus = true
    }

    fun focusLost() {
        hasFocus = false
    }
}
