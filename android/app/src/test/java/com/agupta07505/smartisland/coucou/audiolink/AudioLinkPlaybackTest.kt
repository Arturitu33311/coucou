package com.agupta07505.smartisland.coucou.audiolink

import com.agupta07505.smartisland.coucou.phone.PhoneKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class AudioLinkPlaybackTest {
    @Test
    fun playback_plan_thresholds() {
        assertEquals(PlaybackAction.Wait, playbackPlan(59, started = false))
        assertEquals(PlaybackAction.Play, playbackPlan(60, started = false))
        assertEquals(PlaybackAction.Play, playbackPlan(5, started = true))
        assertEquals(PlaybackAction.Play, playbackPlan(200, started = true))
        assertEquals(PlaybackAction.Drop, playbackPlan(201, started = true))
        assertEquals(PlaybackAction.Drop, playbackPlan(201, started = false))
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
