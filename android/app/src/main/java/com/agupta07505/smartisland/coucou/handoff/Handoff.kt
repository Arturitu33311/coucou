/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * Music handoff, the phone's half. The laptop offers what plays there (a Spotify track from Sonora, a
 * YouTube video) once the person has been on this phone for a while; the offer is a card on the island.
 * Tapping it is the only thing this phone can say (`handoff_accept`); the laptop then pauses and lets go
 * of the Bluetooth headphones, and this phone opens the content once the headphones are here.
 *
 * What it will not do: open anything the laptop names that is not an https page of a service listed in
 * [HandoffRules.HOSTS], or start playing out loud before the headphones are on this phone (it waits for
 * the Bluetooth audio route; if it never comes it opens Bluetooth settings instead).
 */

package com.agupta07505.smartisland.coucou.handoff

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import com.agupta07505.smartisland.R
import com.agupta07505.smartisland.coucou.CoucouRuntime
import com.agupta07505.smartisland.coucou.agents.RemoteMsg
import com.agupta07505.smartisland.coucou.agents.RemoteProtocol
import com.agupta07505.smartisland.di.SmartIslandRepositories
import com.agupta07505.smartisland.model.IslandMode
import com.agupta07505.smartisland.model.IslandNotification
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.util.concurrent.ConcurrentHashMap

/** What the laptop offers: the song or video that plays there, and where the person was in it. */
data class HandoffOffer(
    val id: String,
    /** track, video, search (see handoff.rs). */
    val kind: String,
    val uri: String,
    val title: String,
    val artist: String,
    val player: String,
    val posMs: Long,
    val lengthMs: Long,
    /** The headphones that will be let go. */
    val headset: String,
    /** The laptop lets go of them by itself (the person chose "auto"): no accept is sent. */
    val auto: Boolean,
    val ttlS: Long,
)

object HandoffRules {
    /** The only places an offer may send this phone. */
    val HOSTS = setOf("open.spotify.com", "www.youtube.com", "youtube.com", "m.youtube.com", "music.youtube.com", "youtu.be")

    /** The address to open, or null if it is not a plain https page of one of [HOSTS]. */
    fun safe(uri: String): String? {
        val u = runCatching { java.net.URI(uri) }.getOrNull() ?: return null
        if (u.scheme != "https" || u.userInfo != null || u.port != -1) return null
        if (u.host?.lowercase() !in HOSTS) return null
        return u.toString()
    }

    /** 151000 -> "2:31". */
    fun clock(ms: Long): String {
        val s = (ms.coerceAtLeast(0) / 1000)
        return if (s >= 3600) "%d:%02d:%02d".format(s / 3600, s % 3600 / 60, s % 60) else "%d:%02d".format(s / 60, s % 60)
    }
}

object HandoffFlow {
    private const val KEY = "coucou_handoff"
    private const val DONE_WAIT_MS = 12_000L
    private const val AUDIO_WAIT_MS = 25_000L

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    @Volatile private var current: HandoffOffer? = null
    @Volatile private var started = false
    private val results = ConcurrentHashMap<String, RemoteMsg.HandoffDone>()

    /** The laptop put an offer out: show it on the island. */
    fun offered(app: Context, o: HandoffOffer) {
        current = o
        started = false
        results.remove(o.id)
        post(app, o, app.getString(R.string.coucou_handoff_title), detail(o), expand = true)
        scope.launch {
            delay(o.ttlS.coerceIn(10, 300) * 1000)
            if (current?.id == o.id && !started) {
                current = null
                SmartIslandRepositories.notificationRepository(app).removeNotification(KEY)
            }
        }
    }

    /** How it went on the laptop (it pauses, lets the headphones go, and says so). */
    fun done(msg: RemoteMsg.HandoffDone) {
        results[msg.id] = msg
    }

    /** The person tapped the card (HandoffActivity). */
    fun tap(app: Context, id: String) {
        val o = current?.takeIf { it.id == id } ?: return
        if (started) return
        started = true
        scope.launch { run(app, o) }
    }

    private fun detail(o: HandoffOffer): String =
        listOf(o.title, o.artist.takeIf { it.isNotEmpty() }, HandoffRules.clock(o.posMs)).filterNotNull().joinToString(" · ")

    private suspend fun run(app: Context, o: HandoffOffer) {
        status(app, o, R.string.coucou_handoff_working)
        if (!o.auto && !CoucouRuntime.sendToLaptop(RemoteProtocol.handoffAccept(o.id))) {
            return end(app, o, R.string.coucou_handoff_nolink)
        }
        val done = withTimeoutOrNull(DONE_WAIT_MS) {
            while (results[o.id] == null) delay(200)
            results[o.id]
        }
        if (done == null) return end(app, o, R.string.coucou_handoff_noanswer)
        if (!done.ok) return end(app, o, R.string.coucou_handoff_failed, done.why)

        // The headphones have been let go on the laptop. Nothing plays out loud until they are here.
        val route = withTimeoutOrNull(AUDIO_WAIT_MS) {
            while (!bluetoothAudio(app)) delay(500)
            true
        }
        if (route == null) {
            open(app, Intent(Settings.ACTION_BLUETOOTH_SETTINGS))
            return end(app, o, R.string.coucou_handoff_connect, o.headset.ifEmpty { "Bluetooth" })
        }
        val uri = HandoffRules.safe(o.uri)
        if (uri == null) return end(app, o, R.string.coucou_handoff_ready)
        open(app, Intent(Intent.ACTION_VIEW, Uri.parse(uri)))
        end(app, o, R.string.coucou_handoff_ready)
    }

    private fun bluetoothAudio(app: Context): Boolean {
        val am = app.getSystemService(Context.AUDIO_SERVICE) as AudioManager
        return am.getDevices(AudioManager.GET_DEVICES_OUTPUTS).any {
            it.type == AudioDeviceInfo.TYPE_BLUETOOTH_A2DP ||
                (Build.VERSION.SDK_INT >= 31 && it.type == AudioDeviceInfo.TYPE_BLE_HEADSET)
        }
    }

    /** Started from the background on purpose: this app holds the overlay permission, which allows it. */
    private fun open(app: Context, intent: Intent) {
        runCatching { app.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }

    private fun status(app: Context, o: HandoffOffer, res: Int, vararg args: Any) {
        post(app, o, app.getString(R.string.coucou_handoff_title), app.getString(res, *args), expand = false)
    }

    private fun end(app: Context, o: HandoffOffer, res: Int, vararg args: Any) {
        status(app, o, res, *args)
        scope.launch {
            delay(8_000)
            if (current?.id == o.id) current = null
            SmartIslandRepositories.notificationRepository(app).removeNotification(KEY)
        }
    }

    private fun post(app: Context, o: HandoffOffer, title: String, text: String, expand: Boolean) {
        val tap = PendingIntent.getActivity(
            app, 7, Intent(app, HandoffActivity::class.java).putExtra(HandoffActivity.EXTRA_ID, o.id).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        SmartIslandRepositories.notificationRepository(app).postNotification(
            IslandNotification(
                key = KEY,
                packageName = app.packageName,
                appName = "Coucou",
                title = title,
                text = text,
                timeMillis = System.currentTimeMillis(),
                mode = IslandMode.Notification,
                contentIntent = tap,
            ),
            autoExpand = expand,
        )
    }
}
