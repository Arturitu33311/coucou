package com.agupta07505.smartisland.util

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CameraCutoutFreeSpaceTest {
    // A punch hole 8 mm wide reads as ~28 dp; the detector adds 24 dp of padding to it.
    private val centred = DetectedCutoutInfo(xOffsetDp = 0f, yOffsetDp = 8f, widthDp = 52f, heightDp = 34f, hasHardwareCutout = true)

    @Test
    fun no_hole_means_no_limit() {
        val none = DetectedCutoutInfo(0f, 12f, 112f, 34f, false)
        assertNull(CameraCutoutDetector.freeRightDp(110f, 0f, none, 12f))
    }

    @Test
    fun a_centred_hole_leaves_the_right_half_minus_the_hole_the_padding_and_a_gap() {
        // pill 110: half 55; hole half 14; padding 12; gap 6 -> 23
        assertEquals(23f, CameraCutoutDetector.freeRightDp(110f, 0f, centred, 12f)!!, 0.001f)
    }

    @Test
    fun a_pill_shifted_to_the_right_of_the_hole_has_less_room() {
        // The pill's centre is 10 dp to the right of the hole's, so the hole sits left of the centre
        // and more of the right half is free (55 - 4 - 12 - 6 = 33).
        val moved = CameraCutoutDetector.freeRightDp(110f, 10f, centred, 12f)!!
        val base = CameraCutoutDetector.freeRightDp(110f, 0f, centred, 12f)!!
        assertTrue(moved > base)
    }

    @Test
    fun never_narrower_than_an_icon() {
        assertEquals(CameraCutoutDetector.MIN_SLOT_DP, CameraCutoutDetector.freeRightDp(40f, 0f, centred, 12f)!!, 0.001f)
    }
}
