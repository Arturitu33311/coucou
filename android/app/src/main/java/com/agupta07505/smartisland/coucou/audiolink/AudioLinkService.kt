/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.audiolink

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import com.agupta07505.smartisland.R
import com.agupta07505.smartisland.coucou.phone.PhoneKey
import com.agupta07505.smartisland.coucou.phone.PhoneSettings
import com.agupta07505.smartisland.coucou.phone.Ssh
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.io.OutputStream
import kotlin.math.max

/**
 * Hears the laptop through the headphones that stay on this phone. While the setting is on and Bluetooth
 * headphones are connected to the phone, it dials the laptop over ssh with the audio-only key
 * (`coucou-hook --audio`, a pipe to one socket and nothing else), plays the PCM the laptop sends, and tells
 * the laptop where the headphones are, whether a call is on and whether media is playing here. The laptop
 * decides everything else (see `AudioLinkLogic`); the only thing it can ask of the phone is to pause its media.
 */
class AudioLinkService : Service(), PhoneSignals {
    enum class Status { Off, Idle, NeedsSetup, Connecting, Streaming, Reconnecting }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var loopJob: Job? = null
    private lateinit var watchers: Watchers

    private val headphones = MutableStateFlow(Where.None)
    @Volatile private var call = false
    @Volatile private var media = false
    private val outLock = Any()
    private var out: OutputStream? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startInForeground()
        if (loopJob == null) {
            watchers = Watchers(this, this)
            watchers.start()
            loopJob = scope.launch { loop() }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        runCatching { watchers.stop() }
        scope.cancel()
        _status.value = Status.Off
        super.onDestroy()
    }

    // ── What the phone says about itself ───────────────────────────────────────

    override fun onHeadphones(at: Where) {
        headphones.value = at
        sendState()
    }

    override fun onCall(on: Boolean) {
        call = on
        sendState()
    }

    override fun onPhoneMedia(playing: Boolean) {
        media = playing
        sendState()
    }

    private fun sendLine(line: String) {
        synchronized(outLock) {
            val o = out ?: return
            runCatching {
                o.write((line + "\n").toByteArray())
                o.flush()
            }
        }
    }

    private fun sendState() = sendLine(AudioLinkWire.stateLine(headphones.value, call, media))

    // ── The link ───────────────────────────────────────────────────────────────

    private suspend fun loop() {
        var wait = 2_000L
        while (scope.isActive) {
            if (headphones.value != Where.Phone) {
                _status.value = Status.Idle
                // Nothing to hear without the headphones on this phone: sleep until they are.
                headphones.first { it == Where.Phone }
                continue
            }
            val profile = PhoneSettings.profile(this, PhoneSettings.LAPTOP)
            if (!profile.paired || !PhoneKey.exists(this, AudioLinkSetup.KEY)) {
                _status.value = Status.NeedsSetup
                delay(5_000)
                continue
            }
            _status.value = Status.Connecting
            try {
                Ssh.open(this, profile, keyName = AudioLinkSetup.KEY).use { conn ->
                    conn.stream("coucou-audio").use { stream ->
                        wait = 2_000L
                        session(stream)
                    }
                }
            } catch (_: Exception) {
                // Unreachable laptop, refused key, hung-up pipe: all the same to the phone, try again.
            }
            synchronized(outLock) { out = null }
            _status.value = if (headphones.value == Where.Phone) Status.Reconnecting else Status.Idle
            delay(wait)
            wait = (wait * 2).coerceAtMost(30_000L)
        }
    }

    /** One connected session. Returns when the link ends, goes quiet or the headphones leave. */
    private suspend fun session(stream: Ssh.Stream) {
        val player = RelayPlayer(getSystemService(Context.AUDIO_SERVICE) as AudioManager)
        val lastHeard = java.util.concurrent.atomic.AtomicLong(System.currentTimeMillis())
        val reader = Thread {
            val buf = ByteArray(32 * 1024)
            var have = 0
            try {
                while (true) {
                    val n = stream.input.read(buf, have, buf.size - have)
                    if (n < 0) break
                    have += n
                    lastHeard.set(System.currentTimeMillis())
                    var at = 0
                    while (true) {
                        when (val d = AudioLinkWire.decode(buf.copyOfRange(at, have), have - at)) {
                            is Decoded.Got -> {
                                when (d.frame.kind) {
                                    FrameKind.Audio -> player.feed(d.frame.payload)
                                    FrameKind.PausePhone -> watchers.pausePhoneMedia()
                                    FrameKind.Hello, FrameKind.Heartbeat -> {}
                                }
                                at += d.consumed
                            }
                            Decoded.NeedMore -> break
                            is Decoded.Bad -> return@Thread // a frame this phone does not understand ends the link
                        }
                    }
                    System.arraycopy(buf, at, buf, 0, have - at)
                    have -= at
                    if (have >= buf.size) break // cannot happen with frames this small; never grow without bound
                }
            } catch (_: Exception) {
            }
        }
        synchronized(outLock) { out = stream.output }
        sendState()
        _status.value = Status.Streaming
        reader.start()
        try {
            while (scope.isActive && reader.isAlive) {
                sendLine(AudioLinkWire.heartbeatLine())
                if (System.currentTimeMillis() - lastHeard.get() > SILENCE_MS) break
                if (headphones.value != Where.Phone) {
                    sendState() // tell the laptop they are gone before leaving
                    break
                }
                delay(1_000)
            }
        } finally {
            player.release()
            runCatching { stream.close() }
        }
    }

    // ── Playing it ─────────────────────────────────────────────────────────────

    /** The speaker side: an `AudioTrack` with a small, bounded queue and audio focus while it plays. */
    private class RelayPlayer(private val audio: AudioManager) {
        private val attrs = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
            .build()
        private val format = AudioFormat.Builder()
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
            .setSampleRate(RATE)
            .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
            .build()
        private val track: AudioTrack
        private var started = false
        private var written = 0L
        private var baseHead = 0L
        private val focus: AudioFocusRequest

        init {
            val min = AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT)
            track = AudioTrack.Builder()
                .setAudioAttributes(attrs)
                .setAudioFormat(format)
                .setTransferMode(AudioTrack.MODE_STREAM)
                .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                .setBufferSizeInBytes(max(min, RATE * BYTES_PER_FRAME * (MAX_QUEUE_MS + 100) / 1000))
                .build()
            focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(attrs)
                .setOnAudioFocusChangeListener { change ->
                    // A call or another app took the speaker: stop and start over cleanly when it is ours again.
                    if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
                        synchronized(this) { restart() }
                    }
                }
                .build()
            audio.requestAudioFocus(focus)
        }

        private fun head(): Long = track.playbackHeadPosition.toLong() and 0xffffffffL

        private fun queuedMs(): Int = (((written - (head() - baseHead)) * 1000) / RATE).toInt().coerceAtLeast(0)

        private fun restart() {
            runCatching {
                track.pause()
                track.flush()
            }
            started = false
            written = 0
            baseHead = head()
        }

        @Synchronized
        fun feed(pcm: ByteArray) {
            when (playbackPlan(queuedMs(), started)) {
                PlaybackAction.Drop -> return
                PlaybackAction.Wait -> {}
                PlaybackAction.Play -> if (!started) {
                    track.play()
                    started = true
                }
            }
            val n = track.write(pcm, 0, pcm.size)
            if (n > 0) written += n / BYTES_PER_FRAME
        }

        @Synchronized
        fun release() {
            runCatching { track.stop() }
            track.release()
            audio.abandonAudioFocusRequest(focus)
        }
    }

    // ── Foreground notification ────────────────────────────────────────────────

    private fun startInForeground() {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.audiolink_channel), NotificationManager.IMPORTANCE_LOW),
        )
        val notification: Notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_headset)
            .setContentTitle(getString(R.string.audiolink_notification_title))
            .setContentText(getString(R.string.audiolink_notification_text))
            .setOngoing(true)
            .setSilent(true)
            .build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    companion object {
        private const val CHANNEL = "audiolink"
        private const val NOTIFICATION_ID = 4107
        private const val RATE = 48_000
        private const val BYTES_PER_FRAME = 4
        private const val SILENCE_MS = 3_000L

        private val _status = MutableStateFlow(Status.Off)
        val status: StateFlow<Status> = _status.asStateFlow()

        /** Starts the service (idempotent). Never throws: from the background Android may refuse. */
        fun start(context: Context) {
            runCatching { ContextCompat.startForegroundService(context, Intent(context, AudioLinkService::class.java)) }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, AudioLinkService::class.java))
        }
    }
}
