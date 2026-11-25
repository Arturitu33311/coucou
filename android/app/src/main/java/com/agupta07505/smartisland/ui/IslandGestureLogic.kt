package com.agupta07505.smartisland.ui

import kotlin.math.hypot
import kotlin.math.max

/**
 * The island's touch rules, kept pure so they can be tested.
 *
 * Why this exists: the island's own gesture handler used to consume ANY movement above 0.5 px. Compose's tap
 * detectors (`detectTapGestures`, `clickable`) cancel the tap when an ancestor consumed the movement, so the
 * buttons inside the open island (tasks' check, media controls) only worked with a perfectly still finger. The
 * island now treats a press as a drag only after the system's touch slop, and a still press-and-hold as a tap.
 */
internal object IslandGestureLogic {
    /** Never shorter than the island's own 300 ms; follows the system "touch & hold delay" (accessibility) when longer. */
    fun holdThresholdMs(systemLongPressMs: Long, floorMs: Long = 300L): Long = max(systemLongPressMs, floorMs)

    /** True once the finger has travelled farther than `slopPx` from where it went down. */
    fun crossedSlop(totalDx: Float, totalDy: Float, slopPx: Float): Boolean = hypot(totalDx, totalDy) > slopPx

    /**
     * Whether a release counts as a tap. A hold only cancels the tap when it was also a drag (a hold-swipe has its
     * own action); a still hold is just a slow tap. A release already handled by a child button never also taps.
     */
    fun releaseIsTap(holdRegistered: Boolean, dragging: Boolean, handledInside: Boolean): Boolean =
        !handledInside && !(holdRegistered && dragging)

    /** The collapsed pill's touch region height: the pill plus a margin, but never under `minDp` (a fingertip). */
    fun collapsedTouchHeightDp(pillHeightDp: Float, marginDp: Float = 16f, minDp: Float = 44f): Float =
        max(pillHeightDp + marginDp, minDp)
}
