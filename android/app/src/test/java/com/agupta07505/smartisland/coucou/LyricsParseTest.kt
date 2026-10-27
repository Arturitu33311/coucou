/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import org.junit.Assert.assertEquals
import org.junit.Test

class LyricsParseTest {
    private val lrc = """
        [ar:The Weeknd]
        [ti:Blinding Lights]
        [00:00.00]
        [00:12.34] Yeah
        [01:02.5][02:03.456] Repeated line
        [03:00] No fraction
    """.trimIndent()

    @Test fun parsesTimesTextAndSkipsMetadata() {
        val l = Lyrics.parse(lrc)
        assertEquals(listOf(0L, 12_340L, 62_500L, 123_456L, 180_000L), l.map { it.ms })
        assertEquals(listOf("", "Yeah", "Repeated line", "Repeated line", "No fraction"), l.map { it.text })
    }

    @Test fun findsTheLineBeingSung() {
        val l = Lyrics.parse(lrc)
        assertEquals(0, Lyrics.indexAt(l, 5_000))
        assertEquals(1, Lyrics.indexAt(l, 12_340))
        assertEquals(3, Lyrics.indexAt(l, 150_000))
        assertEquals(4, Lyrics.indexAt(l, 999_999))
        assertEquals(-1, Lyrics.indexAt(Lyrics.parse("[00:10.00] x"), 3_000))
    }
}
