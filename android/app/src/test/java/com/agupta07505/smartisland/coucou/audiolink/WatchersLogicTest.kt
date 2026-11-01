package com.agupta07505.smartisland.coucou.audiolink

import android.media.AudioDeviceInfo
import android.media.AudioManager
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
    fun media_playing_needs_state_playing() {
        assertTrue(WatchersLogic.mediaPlaying(listOf(PlaybackState.STATE_PAUSED, PlaybackState.STATE_PLAYING)))
        assertFalse(WatchersLogic.mediaPlaying(listOf(PlaybackState.STATE_PAUSED, PlaybackState.STATE_BUFFERING)))
        assertFalse(WatchersLogic.mediaPlaying(listOf(PlaybackState.STATE_STOPPED, PlaybackState.STATE_NONE)))
        assertFalse(WatchersLogic.mediaPlaying(emptyList()))
    }
}
