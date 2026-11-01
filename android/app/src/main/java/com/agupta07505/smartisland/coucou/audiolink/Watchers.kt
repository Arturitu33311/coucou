/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.audiolink

import android.content.ComponentName
import android.content.Context
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.os.Build
import android.os.Handler
import android.os.Looper
import com.agupta07505.smartisland.service.SmartIslandNotificationListenerService

/** What the phone tells the audio relay about itself. All of it is reported as it changes. */
interface PhoneSignals {
    fun onHeadphones(at: Where)
    fun onCall(on: Boolean)
    fun onPhoneMedia(playing: Boolean)
}

/** The decisions the watchers make, with no Android objects, so they can be tested on the JVM. */
object WatchersLogic {
    private val BLUETOOTH_OUTPUTS = setOf(
        AudioDeviceInfo.TYPE_BLUETOOTH_A2DP,
        AudioDeviceInfo.TYPE_BLUETOOTH_SCO, // the same headset during a call
        AudioDeviceInfo.TYPE_BLE_HEADSET,
    )

    /** `Phone` when any output device is a Bluetooth headset; the phone never reports `Pc`. */
    fun headphonesWhere(outputTypes: List<Int>): Where =
        if (outputTypes.any { it in BLUETOOTH_OUTPUTS }) Where.Phone else Where.None

    /** A call or a ringing phone, from `AudioManager.getMode()`. */
    fun isCall(audioMode: Int): Boolean =
        audioMode == AudioManager.MODE_IN_CALL ||
            audioMode == AudioManager.MODE_IN_COMMUNICATION ||
            audioMode == AudioManager.MODE_RINGTONE

    /**
     * Sessions that only mirror the laptop's media (KDE Connect shows what the laptop plays as a session here) or are
     * our own: they are not sound made on this phone. Counting them would make the laptop's music look like the phone's
     * and pausing them would pause the laptop.
     */
    fun isMirror(packageName: String?): Boolean = packageName in MIRRORS

    private val MIRRORS = setOf("org.kde.kdeconnect_tp", "com.agupta07505.smartisland")

    /** Media is playing when some session is in `STATE_PLAYING`: not paused, not buffering, not stopped. */
    fun mediaPlaying(sessionStates: List<Int>): Boolean = sessionStates.any { it == PlaybackState.STATE_PLAYING }
}

/**
 * Watches the three things the relay needs from the phone: whether Bluetooth headphones are connected, whether
 * a call is on, and whether music or a video is playing. Media comes from the apps' media sessions, which is
 * why a notification sound or a navigation voice (no session) can never pause the laptop. Reading media
 * sessions needs notification access; without it `onPhoneMedia` simply never fires.
 */
class Watchers(private val context: Context, private val signals: PhoneSignals) {
    private val audio = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private val sessions = context.getSystemService(Context.MEDIA_SESSION_SERVICE) as MediaSessionManager
    private val listener = ComponentName(context, SmartIslandNotificationListenerService::class.java)
    private val main = Handler(Looper.getMainLooper())

    private val controllers = mutableMapOf<MediaController, MediaController.Callback>()
    private var started = false

    /** False when notification access is missing: the phone's media cannot be seen. */
    @Volatile
    var canSeeMedia = true
        private set

    private val devices = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>?) = reportHeadphones()
        override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>?) = reportHeadphones()
    }

    private val modeListener = AudioManager.OnModeChangedListener { signals.onCall(WatchersLogic.isCall(it)) }

    private val sessionsListener = MediaSessionManager.OnActiveSessionsChangedListener { list -> follow(list.orEmpty()) }

    fun start() {
        if (started) return
        started = true
        audio.registerAudioDeviceCallback(devices, main)
        reportHeadphones()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            audio.addOnModeChangedListener(context.mainExecutor, modeListener)
        }
        signals.onCall(WatchersLogic.isCall(audio.mode))
        try {
            sessions.addOnActiveSessionsChangedListener(sessionsListener, listener, main)
            follow(sessions.getActiveSessions(listener))
            canSeeMedia = true
        } catch (_: SecurityException) {
            canSeeMedia = false
        }
    }

    fun stop() {
        if (!started) return
        started = false
        audio.unregisterAudioDeviceCallback(devices)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) audio.removeOnModeChangedListener(modeListener)
        try {
            sessions.removeOnActiveSessionsChangedListener(sessionsListener)
        } catch (_: SecurityException) {
        }
        controllers.forEach { (c, cb) -> c.unregisterCallback(cb) }
        controllers.clear()
    }

    /** Pauses whatever media is playing on the phone (a pause, never a toggle). */
    fun pausePhoneMedia() {
        main.post {
            controllers.keys
                .filter { it.playbackState?.state == PlaybackState.STATE_PLAYING }
                .forEach { it.transportControls.pause() }
        }
    }

    private fun reportHeadphones() {
        val types = audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).map { it.type }
        signals.onHeadphones(WatchersLogic.headphonesWhere(types))
    }

    private fun reportMedia() {
        val states = controllers.keys.mapNotNull { it.playbackState?.state }
        signals.onPhoneMedia(WatchersLogic.mediaPlaying(states))
    }

    /**
     * Follows exactly the sessions that are active now, each for changes in what it is doing. Mirrors of the laptop's
     * media (KDE Connect) and our own sessions are left out: they are neither heard nor paused here.
     */
    private fun follow(all: List<MediaController>) {
        val active = all.filterNot { WatchersLogic.isMirror(it.packageName) }
        val ids = active.map { it.sessionToken }.toSet()
        controllers.keys.filter { it.sessionToken !in ids }.forEach { gone ->
            controllers.remove(gone)?.let { gone.unregisterCallback(it) }
        }
        for (c in active) {
            if (controllers.keys.any { it.sessionToken == c.sessionToken }) continue
            val cb = object : MediaController.Callback() {
                override fun onPlaybackStateChanged(state: PlaybackState?) = reportMedia()
            }
            c.registerCallback(cb, main)
            controllers[c] = cb
        }
        reportMedia()
    }
}
