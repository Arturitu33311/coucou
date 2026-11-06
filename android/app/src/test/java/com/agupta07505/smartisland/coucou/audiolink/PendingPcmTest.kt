package com.agupta07505.smartisland.coucou.audiolink

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PendingPcmTest {
    private val bytesPerMs = 192 // 48 kHz, 16 bit, stereo
    private fun chunk(tag: Int, ms: Int = 20) = ByteArray(ms * bytesPerMs) { tag.toByte() }

    @Test
    fun `sound that finds the speaker full waits and is played in order when it has room`() {
        val q = PendingPcm(bytesPerMs)
        q.add(chunk(1)); q.add(chunk(2)); q.add(chunk(3))
        assertEquals(60, q.ms)
        assertEquals(0, q.drain { 0 }) // the output has not started consuming yet: nothing is lost
        assertEquals(60, q.ms)
        val played = mutableListOf<Int>()
        q.drain { played += it[0].toInt(); it.size }
        assertEquals(listOf(1, 2, 3), played)
        assertTrue(q.isEmpty)
    }

    @Test
    fun `a partial write keeps the rest of that chunk at the front`() {
        val q = PendingPcm(bytesPerMs)
        q.add(chunk(1)); q.add(chunk(2))
        val seen = mutableListOf<ByteArray>()
        q.drain { c -> seen += c; c.size / 2 } // takes half and stops: the output is full
        assertEquals(1, seen.size)
        // The next drain starts with the unplayed half of chunk 1, then chunk 2.
        val next = mutableListOf<ByteArray>()
        q.drain { next += it; it.size }
        assertEquals(2, next.size)
        assertEquals(chunk(1).size / 2, next[0].size)
        assertArrayEquals(chunk(2), next[1])
    }

    @Test
    fun `more than the limit gives up the oldest sound, never the newest`() {
        val q = PendingPcm(bytesPerMs, maxMs = 100)
        for (i in 1..8) q.add(chunk(i)) // 160 ms offered
        assertEquals(100, q.ms)
        assertEquals(3, q.dropped)
        val played = mutableListOf<Int>()
        q.drain { played += it[0].toInt(); it.size }
        assertEquals(listOf(4, 5, 6, 7, 8), played)
    }

    @Test
    fun `a chunk that errors is lost and the rest goes on`() {
        val q = PendingPcm(bytesPerMs)
        q.add(chunk(1)); q.add(chunk(2)); q.add(chunk(3))
        val played = mutableListOf<Int>()
        val errors = q.drain { if (it[0].toInt() == 2) -1 else { played += it[0].toInt(); it.size } }
        assertEquals(1, errors)
        assertEquals(listOf(1, 3), played)
    }

    @Test
    fun `empty chunks are ignored and clear empties it`() {
        val q = PendingPcm(bytesPerMs)
        q.add(ByteArray(0)); assertTrue(q.isEmpty)
        q.add(chunk(1)); q.clear()
        assertTrue(q.isEmpty); assertEquals(0, q.ms)
    }
}
