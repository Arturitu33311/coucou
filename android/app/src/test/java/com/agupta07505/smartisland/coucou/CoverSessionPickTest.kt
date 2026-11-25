package com.agupta07505.smartisland.coucou

import org.junit.Assert.assertEquals
import org.junit.Test

class CoverSessionPickTest {
    private val spotify = "com.spotify.music"
    private val kde = "org.kde.kdeconnect_tp"

    private fun s(pkg: String, playing: Boolean, track: Boolean = true) = SessionView(pkg, track, playing)

    @Test
    fun `what plays on the phone wins over the laptop's mirror, whatever the order`() {
        assertEquals(1, pickSession(listOf(s(kde, true), s(spotify, true))))
        assertEquals(0, pickSession(listOf(s(spotify, true), s(kde, true))))
    }

    @Test
    fun `a mirror is never followed, even when it says it is playing and nothing else is`() {
        assertEquals(-1, pickSession(listOf(s(kde, true))))
        assertEquals(0, pickSession(listOf(s(spotify, false), s(kde, true))))
    }

    @Test
    fun `nothing sounds, the real player's last track is kept`() {
        assertEquals(1, pickSession(listOf(s(kde, false), s(spotify, false))))
    }

    @Test
    fun `our own quiet notification session counts as a mirror`() {
        assertEquals(1, pickSession(listOf(s("com.agupta07505.smartisland", true), s(spotify, true))))
    }

    @Test
    fun `a session with no track is never picked and an empty list gives nothing`() {
        assertEquals(1, pickSession(listOf(s(spotify, true, track = false), s("org.videolan.vlc", true))))
        assertEquals(-1, pickSession(listOf(s(spotify, true, track = false))))
        assertEquals(-1, pickSession(emptyList()))
    }
}
