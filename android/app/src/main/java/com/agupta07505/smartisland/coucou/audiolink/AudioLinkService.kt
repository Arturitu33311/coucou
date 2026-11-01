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
import android.os.SystemClock
import android.util.Log
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
import kotlinx.coroutines.flow.combine
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

    /** The current session's writer. Changes are only ever queued on it: they are reported on the main thread. */
    @Volatile private var outbox: Outbox? = null

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
        runCatching { hidePcNow() }
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
        outbox?.post(line)
    }

    private fun sendState() = sendLine(AudioLinkWire.stateLine(headphones.value, call, media))

    // ── What the phone is playing, for the laptop's island and wallpaper ──────

    private val mediaSession = MutableStateFlow(false)

    /** True once the laptop's hello says it reads `now`/`art` lines. */
    @Volatile private var laptopReadsNow = false
    private var lastNow: NowPlaying? = null
    private var lastArt: android.graphics.Bitmap? = null
    private var sentArtKey: String? = null
    private var sentArtHash: String? = null
    private val nowWorker = java.util.concurrent.Executors.newSingleThreadExecutor { Thread(it, "coucou-audiolink-now").apply { isDaemon = true } }

    override fun onMediaSession(present: Boolean) {
        mediaSession.value = present
    }

    override fun onNowPlaying(now: NowPlaying, art: android.graphics.Bitmap?) {
        lastNow = now
        lastArt = art
        pushNow()
    }

    // ── What the laptop plays, shown here ──────────────────────────────────────

    /** The laptop's cover for the track in `showPcNow`. Written by the reader thread, which is also the one that reads it. */
    private var pcCover: android.graphics.Bitmap? = null

    /** A quiet, ongoing notification with the laptop's track and cover; gone when nothing plays there or the link ends. */
    private fun showPcNow(json: ByteArray) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val o = try {
            org.json.JSONObject(String(json, Charsets.UTF_8))
        } catch (_: Exception) {
            return
        }
        val title = o.optString("title")
        if (title.isEmpty() || o.optString("status") == "Stopped") {
            nm.cancel(PC_NOTIFICATION_ID)
            return
        }
        nm.createNotificationChannel(
            NotificationChannel(PC_CHANNEL, getString(R.string.audiolink_pc_channel), NotificationManager.IMPORTANCE_LOW),
        )
        val sub = listOf(o.optString("artist"), o.optString("album")).filter { it.isNotEmpty() }.joinToString(" · ")
        val b = NotificationCompat.Builder(this, PC_CHANNEL)
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle(title)
            .setContentText(sub)
            .setSubText(getString(R.string.audiolink_pc_channel))
            .setLargeIcon(pcCover)
            .setOngoing(o.optString("status") == "Playing")
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_TRANSPORT)
        try {
            nm.notify(PC_NOTIFICATION_ID, b.build())
        } catch (_: SecurityException) {
            // Notifications are not allowed on this phone: the relay itself does not need them.
        }
    }

    private fun hidePcNow() {
        (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).cancel(PC_NOTIFICATION_ID)
        pcCover = null
    }

    /** A new link starts from nothing sent: the laptop has forgotten the last cover. */
    private fun resetNowSent() {
        sentArtKey = null
        sentArtHash = null
    }

    /** Sends the cover when it changed and then the track, off the main thread (compressing a cover takes a moment). */
    private fun pushNow() {
        if (!laptopReadsNow) return
        val now = lastNow ?: return
        val art = lastArt
        nowWorker.execute {
            try {
                val key = "${now.title}|${now.artist}|${now.album}|${art != null}"
                if (key != sentArtKey) {
                    val jpeg = if (now.title.isEmpty() || art == null) null else NowArt.encode(art)
                    val hash = if (jpeg == null) "none" else NowArt.hash(jpeg)
                    if (hash != sentArtHash) sendLine(NowWire.artLine(hash, jpeg ?: ByteArray(0)))
                    sentArtKey = key
                    sentArtHash = hash
                }
                sendLine(NowWire.nowLine(now))
            } catch (e: Exception) {
                Log.d(TAG, "could not send what is playing: $e")
            }
        }
    }

    // ── The link ───────────────────────────────────────────────────────────────

    /** The link is worth holding while the headphones are on this phone or an app has media (the laptop shows it). */
    private fun wanted() = headphones.value == Where.Phone || mediaSession.value

    private suspend fun loop() {
        var wait = 2_000L
        while (scope.isActive) {
            if (!wanted()) {
                _status.value = Status.Idle
                // Nothing to hear and nothing to show: sleep until the headphones are on this phone or an app has media.
                combine(headphones, mediaSession) { h, s -> h == Where.Phone || s }.first { it }
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
            outbox = null
            laptopReadsNow = false
            hidePcNow()
            _status.value = if (wanted()) Status.Reconnecting else Status.Idle
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
                                    FrameKind.SwapPrompt -> swapAskOf(d.frame.payload)?.let { notifySwap(it) }
                                    FrameKind.Hello -> {
                                        laptopReadsNow = NowWire.helloAllowsNow(d.frame.payload)
                                        resetNowSent()
                                        // A laptop that reads our lines also sends its own music: say we can show it.
                                        if (laptopReadsNow) sendLine(AudioLinkWire.capsLine())
                                        pushNow()
                                    }
                                    FrameKind.PcArt -> pcCover = d.frame.payload.takeIf { it.isNotEmpty() }
                                        ?.let { android.graphics.BitmapFactory.decodeByteArray(it, 0, it.size) }
                                    FrameKind.PcNow -> showPcNow(d.frame.payload)
                                    FrameKind.Heartbeat -> {}
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
        // Everything written to the laptop goes through this one thread (see Outbox).
        val writer = Outbox { line ->
            stream.output.write((line + "\n").toByteArray())
            stream.output.flush()
        }
        outbox = writer
        sendState()
        _status.value = Status.Streaming
        reader.start()
        var goneSince: Long? = null
        try {
            while (scope.isActive && reader.isAlive) {
                writer.post(AudioLinkWire.heartbeatLine())
                val now = System.currentTimeMillis()
                if (now - lastHeard.get() > SILENCE_MS) break
                // A short Bluetooth blip keeps the session (a new one would take seconds to open, longer than the
                // laptop waits before giving its speakers back); a long absence ends it.
                goneSince = if (wanted()) null else goneSince ?: now
                if (shouldLeave(goneSince, now)) break
                player.idle(SystemClock.elapsedRealtime())
                delay(1_000)
            }
        } finally {
            // In this order: stop writing, close the link, wait for the reader, and only then free the speaker.
            outbox = null
            writer.close()
            runCatching { stream.close() }
            runCatching { reader.join(2_000) }
            player.release()
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
        private val bufferMs: Int
        private val maxQueueMs: Int
        private val gate = PlayerGate()
        private var holdsFocus = false
        private var lastFeedAt = 0L

        init {
            val minBytes = AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT)
            // The system mixes a streaming track only once its buffer is completely full, so the buffer must be one the
            // queue limit allows to fill (see trackBufferMs); a short one also keeps the delay low.
            val minMs = (minBytes.toLong() * 1000 / (RATE * BYTES_PER_FRAME)).toInt()
            bufferMs = trackBufferMs(minMs)
            maxQueueMs = queueLimitMs(bufferMs)
            val bufferBytes = max(minBytes, RATE * bufferMs / 1000 * BYTES_PER_FRAME)
            track = AudioTrack.Builder()
                .setAudioAttributes(attrs)
                .setAudioFormat(format)
                .setTransferMode(AudioTrack.MODE_STREAM)
                // The default path, not the low-latency one: that one skips the mixer's effects and uses a simpler resampler,
                // and over Bluetooth the delay is dominated by the codec anyway.
                .setBufferSizeInBytes(bufferBytes)
                .build()
            Log.d(TAG, "track buffer ${bufferBytes / BYTES_PER_FRAME * 1000 / RATE} ms (system minimum $minMs ms), queue limit $maxQueueMs ms")
            focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(attrs)
                .setOnAudioFocusChangeListener { change ->
                    // A call or another app took the speaker: stop, and do not ask for it back (see PlayerGate). If
                    // Android gives it back (after a call or a notification), play on.
                    when (change) {
                        AudioManager.AUDIOFOCUS_LOSS, AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> synchronized(this) {
                            holdsFocus = false
                            gate.focusLost()
                            restart()
                        }
                        AudioManager.AUDIOFOCUS_GAIN -> synchronized(this) {
                            holdsFocus = true
                            gate.focusGranted()
                        }
                    }
                }
                .build()
            // No focus yet: it is taken when sound arrives, so opening the link never pauses the phone's own music.
        }

        private fun takeFocus(): Boolean {
            val granted = audio.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
            if (granted) {
                holdsFocus = true
                gate.focusGranted()
            }
            return granted
        }

        private fun giveFocusBack() {
            audio.abandonAudioFocusRequest(focus)
            holdsFocus = false
            gate.focusReleased()
        }

        /** Called about once a second: after a couple of seconds without sound, stop holding the speaker. */
        @Synchronized
        fun idle(nowMs: Long) {
            if (holdsFocus && lastFeedAt > 0 && nowMs - lastFeedAt > IDLE_MS) {
                restart()
                giveFocusBack()
            }
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

        // What happened to the chunks since the last log line, so a silent relay can be told from a starved one.
        private var nFed = 0
        private var nDrop = 0
        private var nWait = 0
        private var nPlay = 0
        private var nNoFocus = 0
        private var nWriteErr = 0
        private var lastWrite = 0
        private var lastLogAt = 0L

        private fun logStats(now: Long) {
            if (now - lastLogAt < 1000) return
            lastLogAt = now
            Log.d(
                TAG,
                "fed=$nFed play=$nPlay wait=$nWait drop=$nDrop noFocus=$nNoFocus writeErr=$nWriteErr lastWrite=$lastWrite " +
                    "queuedMs=${queuedMs()} head=${head()} base=$baseHead written=$written started=$started focus=$holdsFocus " +
                    "trackState=${track.playState}",
            )
            nFed = 0; nDrop = 0; nWait = 0; nPlay = 0; nNoFocus = 0; nWriteErr = 0
        }

        @Synchronized
        fun feed(pcm: ByteArray) {
            val now = SystemClock.elapsedRealtime()
            lastFeedAt = now
            nFed++
            try {
                feedInner(pcm, now)
            } finally {
                logStats(now)
            }
        }

        private fun feedInner(pcm: ByteArray, now: Long) {
            when (gate.next(now, loud = !isSilentPcm(pcm))) {
                GateStep.Reset -> {
                    // The sound stopped for a while and is back: start the speaker over. No focus yet: a short sound
                    // (a ping) must not pause the phone's music; it is asked for only once the sound goes on.
                    restart()
                    giveFocusBack()
                }
                GateStep.AskFocus -> if (!takeFocus()) { nNoFocus++; return } // refused (a call, say): the chunk is dropped
                GateStep.Play -> {}
                GateStep.Silent -> { nNoFocus++; return } // someone else holds the focus: dropped, not fought over
            }
            when (playbackPlan(queuedMs(), started, maxQueueMs)) {
                PlaybackAction.Drop -> { nDrop++; return }
                PlaybackAction.Wait -> nWait++
                PlaybackAction.Play -> {
                    nPlay++
                    if (!started) {
                        track.play()
                        started = true
                    }
                }
            }
            // Never block the reader thread: what does not fit in the buffer is dropped, which also keeps the delay bounded.
            val n = track.write(pcm, 0, pcm.size, AudioTrack.WRITE_NON_BLOCKING)
            lastWrite = n
            if (n > 0) written += n / BYTES_PER_FRAME
            if (n < pcm.size) {
                if (n < 0) nWriteErr++ else nDrop++
            }
        }

        @Synchronized
        fun release() {
            runCatching { track.stop() }
            track.release()
            giveFocusBack()
        }
    }

    // ── Moving the headphones by hand ──────────────────────────────────────────

    /**
     * Android does not let an app disconnect or connect Bluetooth audio by itself, so the laptop's request to
     * move the headphones becomes a notification that opens Bluetooth settings: the person does the one tap.
     */
    private fun notifySwap(ask: SwapAsk) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.createNotificationChannel(
            NotificationChannel(SWAP_CHANNEL, getString(R.string.audiolink_swap_channel), NotificationManager.IMPORTANCE_HIGH),
        )
        val open = android.app.PendingIntent.getActivity(
            this, 0, Intent(android.provider.Settings.ACTION_BLUETOOTH_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            android.app.PendingIntent.FLAG_IMMUTABLE or android.app.PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val (title, text) = when (ask) {
            SwapAsk.Release -> R.string.audiolink_release_title to R.string.audiolink_release_text
            SwapAsk.Connect -> R.string.audiolink_connect_title to R.string.audiolink_connect_text
        }
        nm.notify(
            SWAP_NOTIFICATION_ID,
            NotificationCompat.Builder(this, SWAP_CHANNEL)
                .setSmallIcon(android.R.drawable.stat_sys_headset)
                .setContentTitle(getString(title))
                .setContentText(getString(text))
                .setContentIntent(open)
                .setAutoCancel(true)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .build(),
        )
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
        private const val PC_CHANNEL = "audiolink-pc-music"
        private const val PC_NOTIFICATION_ID = 4109
        private const val SWAP_CHANNEL = "audiolink-swap"
        private const val SWAP_NOTIFICATION_ID = 4108
        private const val RATE = 48_000
        private const val BYTES_PER_FRAME = 4
        private const val SILENCE_MS = 3_000L
        private const val TAG = "AudioLink"
        private const val IDLE_MS = 2_000L

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
