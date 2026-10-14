/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * The arithmetic that keeps Mochi, his outfit and what floats above his head inside the box he is
 * drawn in at island size: the pill and the bubble clip whatever falls outside it. Coordinates are in
 * box heights, 0 at the top of the box.
 */

package com.agupta07505.smartisland.coucou.ui

object GlyphFit {
    /** His body spans this much of the box, centred: its top is at 0.11 and its bottom at 0.89. */
    const val BODY = 0.78f
    const val BODY_TOP = 0.11f
    const val BODY_BOTTOM = 0.89f

    /** A little air above the tallest thing, for the bob and the hop. */
    const val MARGIN = 0.06f

    /** [room] is how far above his body something floats (0: nothing does); returns it with the margin. */
    fun need(room: Float): Float = if (room > 0f) room + MARGIN else 0f

    /** The scale of the whole picture: his outfit's own fit, and no more than the room needs. */
    fun target(rawFit: Float, need: Float): Float =
        if (need > 0f) minOf(rawFit, 1f / (BODY + need)) else rawFit

    /** How far the picture slides down (box heights) so its top, not its middle, meets the box. */
    fun drop(target: Float, need: Float): Float =
        if (need > 0f) (-0.5f + target * ((0.5f - BODY_TOP) + need)).coerceAtLeast(0f) else 0f

    /** Where the highest floating thing ends up (0 = the top of the box). */
    fun top(target: Float, drop: Float, need: Float): Float = 0.5f + target * ((BODY_TOP - need) - 0.5f) + drop

    /** Where the bottom of his body ends up (1 = the bottom of the box). */
    fun bottom(target: Float, drop: Float): Float = 0.5f + target * (BODY_BOTTOM - 0.5f) + drop
}
