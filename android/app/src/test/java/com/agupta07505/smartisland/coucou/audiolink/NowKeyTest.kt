package com.agupta07505.smartisland.coucou.audiolink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class NowKeyTest {
    private val track = NowPlaying("Seguro Le Dolió", "Banda MS", "Seguro Le Dolió", "Playing", 1000, 180000)

    @Test
    fun `a new cover for the same words is a new key, so it is sent`() {
        // The player sent the title first (still the old cover, id 7) and the new cover a moment later (id 9).
        assertNotEquals(nowKey(track, 7), nowKey(track, 9))
    }

    @Test
    fun `the same words and the same picture are the same key, whatever the position or state`() {
        val later = track.copy(positionMs = 90000, status = "Paused")
        assertEquals(nowKey(track, 7), nowKey(later, 7))
    }

    @Test
    fun `no cover and a cover are different, and another title is another key`() {
        assertNotEquals(nowKey(track, null), nowKey(track, 7))
        assertNotEquals(nowKey(track, 7), nowKey(track.copy(title = "Otra"), 7))
    }
}
