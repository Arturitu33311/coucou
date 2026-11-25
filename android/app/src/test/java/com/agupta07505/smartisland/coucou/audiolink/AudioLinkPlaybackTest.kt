package com.agupta07505.smartisland.coucou.audiolink

import com.agupta07505.smartisland.coucou.phone.PhoneKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class AudioLinkPlaybackTest {
    @Test
    fun playback_plan_thresholds() {
        assertEquals(PlaybackAction.Wait, playbackPlan(PREBUFFER_MS - 1, started = false))
        assertEquals(PlaybackAction.Play, playbackPlan(PREBUFFER_MS, started = false))
        assertEquals(PlaybackAction.Play, playbackPlan(5, started = true))
        assertEquals(PlaybackAction.Play, playbackPlan(MAX_QUEUE_MS, started = true))
        assertEquals(PlaybackAction.Drop, playbackPlan(MAX_QUEUE_MS + 1, started = true))
        assertEquals(PlaybackAction.Drop, playbackPlan(MAX_QUEUE_MS + 1, started = false))
    }

    @Test
    fun a_burst_of_a_few_hundred_ms_is_played_not_dropped() {
        // A dozing Wi-Fi hands over 300 ms at once: with the old 200 ms limit that was dropped (cuts and skips).
        assertEquals(PlaybackAction.Play, playbackPlan(300, started = true))
        assertEquals(PlaybackAction.Play, playbackPlan(trackBufferMs(5), started = true))
    }

    /**
     * AudioFlinger only starts mixing a streaming track once its buffer is completely full, and gives up on it after a
     * timeout ("BUFFER TIMEOUT: remove track from active list due to underrun"). A buffer larger than what we let queue
     * is therefore never full: the position stays at 0 and not a sound comes out. This is what happened with a 300 ms
     * buffer and a 200 ms queue limit.
     */
    @Test
    fun the_track_buffer_can_always_fill_before_the_queue_limit_drops_sound() {
        for (minBufferMs in listOf(5, 20, 80, 120, 180, 240, 400)) {
            val buffer = trackBufferMs(minBufferMs)
            val limit = queueLimitMs(buffer)
            assertTrue("buffer $buffer ms must be within the limit $limit ms (min $minBufferMs)", buffer < limit)
            assertTrue("a full buffer must not already count as too much", playbackPlan(buffer, started = true, maxQueueMs = limit) != PlaybackAction.Drop)
        }
        assertTrue("prebuffer must be below the buffer", PREBUFFER_MS < trackBufferMs(5))
    }

    @Test
    fun the_track_buffer_is_short_unless_the_system_needs_more() {
        assertEquals(BUFFER_MS, trackBufferMs(20))
        assertEquals(240, trackBufferMs(240))
    }

    @Test
    fun the_audio_key_lives_in_other_files_than_the_phone_key() {
        // The Claude Code key keeps its existing file names, so nothing already paired is lost.
        assertEquals("coucou-phone-key.enc" to "coucou-phone-key.pub", PhoneKey.fileNames("phone"))
        val audio = PhoneKey.fileNames(AudioLinkSetup.KEY)
        assertNotEquals(PhoneKey.fileNames("phone").first, audio.first)
        assertNotEquals(PhoneKey.fileNames("phone").second, audio.second)
    }

    @Test
    fun an_unknown_key_name_is_refused() {
        assertThrows(IllegalArgumentException::class.java) { PhoneKey.fileNames("../etc/passwd") }
    }

    @Test
    fun the_audio_line_runs_only_the_audio_command() {
        assertEquals(
            "restrict,command=\"/home/alan/.local/share/coucou/bin/coucou-hook --audio\" ssh-rsa AAAA coucou-android-audio",
            AudioLinkSetup.authorizedLine("alan", "ssh-rsa AAAA coucou-android-audio"),
        )
    }
}
