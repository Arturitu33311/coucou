package com.agupta07505.smartisland.coucou.audiolink

import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.media.session.MediaController
import android.media.session.PlaybackState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class WatchersLogicTest {
    @Test
    fun a2dp_present_is_phone() {
        val types = listOf(AudioDeviceInfo.TYPE_BUILTIN_SPEAKER, AudioDeviceInfo.TYPE_BLUETOOTH_A2DP)
        assertEquals(Where.Phone, WatchersLogic.headphonesWhere(types))
    }

    @Test
    fun no_a2dp_is_none() {
        assertEquals(Where.None, WatchersLogic.headphonesWhere(listOf(AudioDeviceInfo.TYPE_BUILTIN_SPEAKER)))
        assertEquals(Where.None, WatchersLogic.headphonesWhere(emptyList()))
    }

    @Test
    fun wired_only_is_none() {
        assertEquals(Where.None, WatchersLogic.headphonesWhere(listOf(AudioDeviceInfo.TYPE_WIRED_HEADSET)))
    }

    @Test
    fun sco_and_le_headsets_count_too() {
        // During a call the headset is listed as SCO: that must not look like the headphones left.
        assertEquals(Where.Phone, WatchersLogic.headphonesWhere(listOf(AudioDeviceInfo.TYPE_BLUETOOTH_SCO)))
        assertEquals(Where.Phone, WatchersLogic.headphonesWhere(listOf(AudioDeviceInfo.TYPE_BLE_HEADSET)))
    }

    @Test
    fun call_modes() {
        assertTrue(WatchersLogic.isCall(AudioManager.MODE_IN_CALL))
        assertTrue(WatchersLogic.isCall(AudioManager.MODE_IN_COMMUNICATION))
        assertTrue(WatchersLogic.isCall(AudioManager.MODE_RINGTONE))
        assertFalse(WatchersLogic.isCall(AudioManager.MODE_NORMAL))
    }

    @Test
    fun the_laptops_mirror_is_not_music_playing_on_the_phone() {
        // KDE Connect shows what plays on the laptop as a media session here; it is not sound made on this phone.
        assertTrue(WatchersLogic.isMirror("org.kde.kdeconnect_tp"))
        assertTrue(WatchersLogic.isMirror("com.agupta07505.smartisland"))
        assertFalse(WatchersLogic.isMirror("com.spotify.music"))
        assertFalse(WatchersLogic.isMirror("com.google.android.youtube"))
        assertFalse(WatchersLogic.isMirror(null))
    }

    @Test
    fun spotify_connect_playing_on_the_laptop_is_not_the_phones_music() {
        // Spotify on the phone shows the laptop's Spotify ("playing on ALAN-LAP") as a remote session.
        assertTrue(WatchersLogic.isRemote(MediaController.PlaybackInfo.PLAYBACK_TYPE_REMOTE))
        assertFalse(WatchersLogic.isRemote(MediaController.PlaybackInfo.PLAYBACK_TYPE_LOCAL))
        assertFalse(WatchersLogic.isRemote(null))
    }

    @Test
    fun media_playing_needs_state_playing() {
        assertTrue(WatchersLogic.mediaPlaying(listOf(PlaybackState.STATE_PAUSED, PlaybackState.STATE_PLAYING)))
        assertFalse(WatchersLogic.mediaPlaying(listOf(PlaybackState.STATE_PAUSED, PlaybackState.STATE_BUFFERING)))
        assertFalse(WatchersLogic.mediaPlaying(listOf(PlaybackState.STATE_STOPPED, PlaybackState.STATE_NONE)))
        assertFalse(WatchersLogic.mediaPlaying(emptyList()))
    }
}
