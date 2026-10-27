/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.graphics.Bitmap
import java.util.concurrent.atomic.AtomicReference

/** The cover of what is playing right now, handed from the notification listener to the animated wallpaper (same process). */
object CoverBus {
    /** [at] is when [playing] or [key] last changed: how long it has been paused is `now - at`. */
    data class Now(val cover: Bitmap?, val playing: Boolean, val key: String, val at: Long = System.currentTimeMillis())

    private val ref = AtomicReference(Now(null, false, ""))

    fun publish(cover: Bitmap?, playing: Boolean, key: String) {
        val prev = ref.get()
        val changed = prev.playing != playing || prev.key != key
        // keep the last cover while paused so the wallpaper does not flash empty
        ref.set(Now(cover ?: prev.cover.takeIf { key == prev.key }, playing, key, if (changed) System.currentTimeMillis() else prev.at))
    }

    fun current(): Now = ref.get()

    /**
     * A cheap fingerprint of a cover: players send a placeholder first and the real art later under the same title, so
     * the title alone cannot tell "already shown" from "better picture arrived".
     */
    fun signature(b: Bitmap?): Int {
        if (b == null || b.isRecycled || b.width < 4 || b.height < 4) return 0
        var h = b.width * 31 + b.height
        for (i in 1..4) for (j in 1..4) {
            h = h * 31 + b.getPixel(b.width * i / 5, b.height * j / 5)
        }
        return h
    }
}
