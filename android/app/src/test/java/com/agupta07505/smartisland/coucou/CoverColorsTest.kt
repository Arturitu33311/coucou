package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.ui.CoverColors
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CoverColorsTest {
    // The dominant colour: vivid pixels win over dull ones, and dull covers fall back to settings.
    private fun solid(argb: Int, n: Int = 64) = IntArray(n) { argb }

    @Test
    fun vivid_red_wins() {
        val c = CoverColors.dominant(solid(0xFFFF2211.toInt()))
        assertNotNull(c)
        val r = (c!! shr 16) and 0xFF
        val g = (c shr 8) and 0xFF
        val b = c and 0xFF
        assertTrue("expected red, got $r,$g,$b", r > 200 && g < 100 && b < 100)
    }

    @Test
    fun vivid_minority_beats_dull_majority() {
        val px = IntArray(64) { 0xFF888888.toInt() }
        repeat(10) { px[it] = 0xFF2244CC.toInt() }
        val c = CoverColors.dominant(px)
        assertNotNull(c)
        val b = c!! and 0xFF
        val r = (c shr 16) and 0xFF
        assertTrue("expected blue, got $r,?, $b", b > r + 40)
    }

    @Test
    fun grey_black_white_and_transparent_have_no_colour() {
        assertNull(CoverColors.dominant(solid(0xFF888888.toInt())))
        assertNull(CoverColors.dominant(solid(0xFF000000.toInt())))
        assertNull(CoverColors.dominant(solid(0xFFFFFFFF.toInt())))
        assertNull(CoverColors.dominant(solid(0x00000000)))
        assertNull(CoverColors.dominant(intArrayOf()))
    }

    @Test
    fun result_is_opaque_argb() {
        val c = CoverColors.dominant(solid(0xFF11AA55.toInt()))!!
        assertEquals(0xFF, (c ushr 24))
    }
}
