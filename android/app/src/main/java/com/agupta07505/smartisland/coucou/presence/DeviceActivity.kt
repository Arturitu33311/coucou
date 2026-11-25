/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * Is this the device Mochi and Coucou's cards belong on? Fed by three things that can each be
 * missing: the laptop's beats over the ssh link, the presence server's answer, and this phone's own
 * screen and touches. Nothing depends on the others being there: with nobody to hear and no server,
 * the answer is "yes, here". Only rendering follows [active]; timeouts, answers and sync do not.
 */

package com.agupta07505.smartisland.coucou.presence

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.PowerManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

object DeviceActivity {
    private val _active = MutableStateFlow(true)

    /** True while this phone is the one that shows Mochi and Coucou. Starts true: nobody has said otherwise. */
    val active: StateFlow<Boolean> = _active.asStateFlow()

    private val _peerMusic = MutableStateFlow(false)

    /** The laptop is playing something: Mochi dances here too when he is the one on screen. */
    val peerMusic: StateFlow<Boolean> = _peerMusic.asStateFlow()

    private val _peerNews = MutableStateFlow(false)

    /** Something just arrived on the laptop: a moment of surprise for Mochi here. */
    val peerNews: StateFlow<Boolean> = _peerNews.asStateFlow()

    /** How long the server's answer counts: the heartbeat is every 30 s, with one missed. */
    private const val SERVER_TTL_MS = 75_000L
    private const val NEWS_MS = 3_000L

    private val _peerLimit = MutableStateFlow(false)

    /** Claude Code is rate-limited on the laptop: Mochi is tired here as well. */
    val peerLimit: StateFlow<Boolean> = _peerLimit.asStateFlow()

    private data class PeerState(val beat: Beat, val music: Boolean, val news: Boolean, val limit: Boolean, val at: Long)

    /**
     * Every other device that has been heard, by id: the laptop (over the link) and the other phone
     * (its beats are relayed by the laptop, the only place the phones meet).
     */
    private val peers = java.util.concurrent.ConcurrentHashMap<String, PeerState>()

    /** Which phone this is (s21, s22…). Set from the settings when the runtime starts. */
    @Volatile var myId: String = Arbiter.idForModel(android.os.Build.MODEL)
        private set
    @Volatile private var serverActive: String? = null
    @Volatile private var serverAt = 0L
    @Volatile private var previous: String? = null
    @Volatile private var screenOnTs = 0L
    @Volatile private var lastTouchTs = 0L

    private var receiver: BroadcastReceiver? = null
    private var ticker: Job? = null

    @Synchronized
    fun start(app: Context, scope: CoroutineScope) {
        appContext = app.applicationContext
        myId = com.agupta07505.smartisland.coucou.phone.PhoneSettings.deviceId(app)
        val pm =app.getSystemService(Context.POWER_SERVICE) as PowerManager
        if (screenOnTs == 0L && pm.isInteractive) screenOnTs = System.currentTimeMillis()
        if (receiver == null) {
            val r = object : BroadcastReceiver() {
                override fun onReceive(c: Context, intent: Intent) {
                    if (intent.action == Intent.ACTION_SCREEN_ON) screenOnTs = System.currentTimeMillis()
                    recompute(c)
                }
            }
            val filter = IntentFilter().apply {
                addAction(Intent.ACTION_SCREEN_ON)
                addAction(Intent.ACTION_SCREEN_OFF)
            }
            runCatching {
                if (Build.VERSION.SDK_INT >= 33) app.registerReceiver(r, filter, Context.RECEIVER_NOT_EXPORTED)
                else {
                    @Suppress("UnspecifiedRegisterReceiverFlag")
                    app.registerReceiver(r, filter)
                }
                receiver = r
            }
        }
        if (ticker?.isActive != true) {
            // Expiry runs on a clock, not on a connection closing: an ssh link to a phone that left
            // the wifi can look alive for minutes.
            ticker = scope.launch {
                while (isActive) {
                    recompute(app)
                    delay(2_000)
                }
            }
        }
    }

    @Volatile private var appContext: Context? = null

    /** The overlay and the accessibility service call this when the person uses the phone. */
    fun noteInteraction() {
        // The first touch after a pause settles who has Mochi right now: no waiting for the clock.
        val wasIdle = idleSeconds() >= 1
        lastTouchTs = System.currentTimeMillis()
        if (wasIdle) appContext?.let { recompute(it) }
    }

    fun idleSeconds(): Long {
        val ref = maxOf(screenOnTs, lastTouchTs)
        if (ref <= 0L) return 0L
        return maxOf(0L, (System.currentTimeMillis() - ref) / 1000)
    }

    fun screenOn(app: Context): Boolean =
        (app.getSystemService(Context.POWER_SERVICE) as PowerManager).isInteractive

    /** The presence server answered (or did not) for this phone's heartbeat. */
    fun onServer(app: Context, ok: Boolean, activeDevice: String?) {
        serverActive = if (ok) Arbiter.serverAnswer(activeDevice) else null
        serverAt = if (ok) System.currentTimeMillis() else 0L
        recompute(app)
    }

    /** A beat arrived over the link: the laptop's own, or the other phone's relayed by it. Our own is ignored. */
    fun onPeer(app: Context, beat: Beat, music: Boolean, news: Boolean, limit: Boolean = false) {
        if (beat.device == myId || beat.device !in Arbiter.KNOWN) return
        peers[beat.device] = PeerState(beat, music, news, limit, System.currentTimeMillis())
        recompute(app)
    }

    /** The laptop says another phone left the link: it is not there until it beats again. */
    fun onPeerGone(app: Context, device: String) {
        if (peers.remove(device) != null) recompute(app)
    }

    /** The link to the laptop ended: nobody on it is heard any more (the other phone came through it). */
    fun onLinkDown(app: Context) {
        peers.clear()
        recompute(app)
    }

    /** What this phone says about itself right now. */
    fun myBeat(app: Context): Beat {
        val fresh = serverAt > 0 && System.currentTimeMillis() - serverAt <= SERVER_TTL_MS
        return Beat(
            device = myId,
            idleS = idleSeconds(),
            screenOn = screenOn(app),
            serverOk = fresh,
            serverActive = if (fresh) serverActive else null,
        )
    }

    @Synchronized
    private fun recompute(app: Context) {
        val now = System.currentTimeMillis()
        val seen = peers.values.map { Seen(it.beat, now - it.at) }
        // What the laptop itself hears and sees (the dance, the startle, the tired Mochi) comes from the
        // laptop's own beats, not from the other phone's.
        val p = peers[Arbiter.LAPTOP]
        val me = myBeat(app)
        val winner = Arbiter.decide(me, seen, previous)
        if (winner != previous) {
            // One line per change of who has Mochi: what each side said, so a migration that does not
            // happen can be read off the log (adb logcat -s Presence).
            android.util.Log.d(
                "Presence",
                "winner=$winner (was $previous) me[idle=${me.idleS} screen=${me.screenOn} srv=${me.serverOk}/${me.serverActive}] " +
                    (peers.values.joinToString(" ") {
                        "${it.beat.device}[idle=${it.beat.idleS} age=${now - it.at}ms screen=${it.beat.screenOn} srv=${it.beat.serverOk}/${it.beat.serverActive}]"
                    }.ifEmpty { "peers[none]" }),
            )
        }
        previous = winner
        _active.value = winner == myId
        val alive = p != null && now - p.at <= Arbiter.FRESH_MS
        _peerMusic.value = alive && p != null && p.music
        _peerLimit.value = alive && p != null && p.limit
        // News is an edge, not a state: only the beat that carries it counts, not the ones after.
        _peerNews.value = alive && p != null && p.news && now - p.at <= NEWS_MS
    }
}
