package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.ui.GlyphFit
import com.agupta07505.smartisland.coucou.ui.Outfit
import com.agupta07505.smartisland.coucou.ui.Wardrobe
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class GlyphFitTest {
    // How far above his body the things that float over his head reach (box heights), as MochiGlyph uses.
    private val rooms = listOf(0.34f, 0.21f, 0.22f)

    @Test
    fun nothing_floating_means_the_outfits_own_fit_and_no_slide() {
        for (o in Outfit.entries) {
            val raw = if (o == Outfit.None) 1f else Wardrobe.fit(o)
            assertEquals(raw, GlyphFit.target(raw, GlyphFit.need(0f)), 0f)
            assertEquals(0f, GlyphFit.drop(raw, GlyphFit.need(0f)), 0f)
        }
    }

    @Test
    fun the_whole_picture_stays_inside_the_box_for_every_outfit_and_every_floating_thing() {
        for (o in Outfit.entries) {
            val raw = if (o == Outfit.None) 1f else Wardrobe.fit(o)
            for (room in rooms) {
                val need = GlyphFit.need(room)
                val fit = GlyphFit.target(raw, need)
                val drop = GlyphFit.drop(fit, need)
                val top = GlyphFit.top(fit, drop, need)
                val bottom = GlyphFit.bottom(fit, drop)
                assertTrue("$o room=$room: top=$top", top >= -0.001f)
                assertTrue("$o room=$room: bottom=$bottom", bottom <= 1.001f)
                assertTrue("$o room=$room: fit=$fit", fit in 0.5f..1f)
            }
        }
    }

    @Test
    fun a_taller_thing_asks_for_a_smaller_picture() {
        val small = GlyphFit.target(1f, GlyphFit.need(0.2f))
        val tall = GlyphFit.target(1f, GlyphFit.need(0.4f))
        assertTrue(tall < small)
    }
}
