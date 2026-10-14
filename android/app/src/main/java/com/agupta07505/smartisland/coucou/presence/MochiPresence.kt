/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * Jinx's messages on this phone, through the presence server (no voice here:
 * text only). Heartbeat says "this phone is alive and looked at"; the inbox
 * carries Jinx's texts; the island shows them while this phone holds the
 * presence, and acks them so the VGA panel does not show them too.
 * Only while the screen is on: with the screen off there is nobody to show
 * anything to, and Doze would eat a tighter schedule anyway.
 */

package com.agupta07505.smartisland.coucou.presence

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.PowerManager
import android.util.Log
import com.agupta07505.smartisland.coucou.phone.PhoneSettings
import com.agupta07505.smartisland.di.SmartIslandRepositories
import com.agupta07505.smartisland.model.IslandMode
import com.agupta07505.smartisland.model.IslandNotification
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/** One text of Jinx's waiting in the presence inbox. */
data class PresenceMsg(
    val id: Long,
    val text: String,
    val mood: String?,
    val kind: String,
    val source: String,
    val ts: Long,
)

/** Moods the server may send. Anything else falls back to Listening. */
fun validMood(name: String?): String? {
    if (name == null) return null
    return name.takeIf {
        it in setOf("Idle", "Happy", "Sleepy", "Alert", "Listening",
            "Celebrating", "Dancing", "Tired", "Surprised", "Glance") ||
            com.agupta07505.smartisland.coucou.ui.MochiMood.entries.any { m -> m.emote != null && m.name == it }
    }
}

/** Tiny HTTP client. No new dependency: HttpURLConnection is enough for this. */
object PresenceNet {
    fun get(base: String, path: String, key: String, timeoutMs: Int = 8000): String? =
        request(base, path, key, null, timeoutMs)

    fun post(base: String, path: String, key: String, body: String, timeoutMs: Int = 8000): String? =
        request(base, path, key, body, timeoutMs)

    /**
     * The key rides in a header, so it only goes where it cannot be read on the way: https anywhere, or
     * plain http inside a private network (the tailnet's 100.64.0.0/10, a LAN, loopback, the emulator's
     * host). Plain http to a public name or address would show it to everyone on the path.
     */
    fun mayCarryKey(base: String): Boolean {
        val m = Regex("""^(https?)://([^/:]+)""", RegexOption.IGNORE_CASE).find(base.trim()) ?: return false
        if (m.groupValues[1].equals("https", ignoreCase = true)) return true
        val host = m.groupValues[2].lowercase()
        if (host == "localhost" || host.endsWith(".ts.net") || host.endsWith(".local")) return true
        val o = host.split('.').map { it.toIntOrNull() ?: return false }
        if (o.size != 4 || o.any { it !in 0..255 }) return false
        return o[0] == 10 || o[0] == 127 ||
            (o[0] == 192 && o[1] == 168) ||
            (o[0] == 172 && o[1] in 16..31) ||
            (o[0] == 100 && o[1] in 64..127)
    }

    private fun request(base: String, path: String, key: String, body: String?, timeoutMs: Int): String? {
        if (!mayCarryKey(base)) {
            Log.w("MochiPresence", "not sending the key over plain http to a public address: use https or the tailnet")
            return null
        }
        var conn: HttpURLConnection? = null
        return try {
            conn = (URL(base.trimEnd('/') + path).openConnection() as HttpURLConnection).apply {
                connectTimeout = timeoutMs
                readTimeout = timeoutMs
                setRequestProperty("X-Mochi-Key", key)
                if (body != null) {
                    requestMethod = "POST"
                    setRequestProperty("Content-Type", "application/json")
                    doOutput = true
                    outputStream.use { it.write(body.toByteArray()) }
                }
            }
            if (conn.responseCode !in 200..299) return null
            conn.inputStream.bufferedReader().use { it.readText() }
        } catch (e: Throwable) {
            // Throwable on purpose: the platform's HTTP stack signals some failures with an Error
            // (AssertionError), and nothing here may take the process down.
            Log.w("MochiPresence", "request failed: ${e::class.java.simpleName}: ${e.message}")
            null
        } finally {
            conn?.disconnect()
        }
    }
}

/** What the island shows of Jinx's inbox: one card with the latest text. */
object IslandJinx {
    const val KEY = "coucou_jinx"

    fun publish(context: Context, msgs: List<PresenceMsg>, autoExpandAlert: Boolean): Set<Long> {
        val repo = SmartIslandRepositories.notificationRepository(context)
        if (msgs.isEmpty()) {
            repo.removeNotification(KEY)
            return emptySet()
        }
        val last = msgs.last()
        // The sender's mood when it gave one, else what her text reads as (the PC's rules, JinxMood).
        val mood = validMood(last.mood) ?: com.agupta07505.smartisland.coucou.assist.JinxMood.moodOfText(last.text)?.name
        repo.postNotification(
            IslandNotification(
                key = KEY,
                packageName = context.packageName,
                appName = "Jinx",
                title = if (msgs.size > 1) "Jinx · +${msgs.size - 1}" else "Jinx",
                text = last.text,
                timeMillis = last.ts * 1000,
                mode = IslandMode.JinxMsg,
                category = "mochi:${mood ?: "Listening"}:${last.kind}",
            ),
            autoExpand = autoExpandAlert && last.kind == "alert",
        )
        return msgs.map { it.id }.toSet()
    }

    fun clear(context: Context) {
        SmartIslandRepositories.notificationRepository(context).removeNotification(KEY)
    }
}

/**
 * Heartbeat + inbox while the screen is on. Started by CoucouRuntime when the
 * followJinx switch is on; safe to start twice, safe to stop when idle.
 */
object MochiPresence {
    private const val TAG = "MochiPresence"
    private const val TICK_MS = 30_000L

    private val _status = MutableStateFlow("off")
    val status: StateFlow<String> = _status.asStateFlow()

    private var job: Job? = null
    private var screenReceiver: BroadcastReceiver? = null

    /** Fire-and-forget acks (the Cerrar button runs on the UI thread). */
    private val ioScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** Ids currently on the island: what Cerrar acks. */
    @Volatile private var shownIds: Set<Long> = emptySet()

    @Volatile private var screenOnTs = 0L

    /** The overlay calls this when Alan touches the island: best idle signal we have without UsageStats. */
    fun noteInteraction() {
        DeviceActivity.noteInteraction()
    }

    /** The idle clock lives in [DeviceActivity], so the arbitration works with this switch off. */
    fun idleSeconds(): Long = DeviceActivity.idleSeconds()

    @Synchronized
    fun start(app: Context, scope: CoroutineScope) {
        if (job?.isActive == true) return
        val pm = app.getSystemService(Context.POWER_SERVICE) as PowerManager
        if (pm.isInteractive) screenOnTs = System.currentTimeMillis()
        if (screenReceiver == null) {
            val r = object : BroadcastReceiver() {
                override fun onReceive(c: Context, intent: Intent) {
                    when (intent.action) {
                        Intent.ACTION_SCREEN_ON -> {
                            screenOnTs = System.currentTimeMillis()
                            _status.value = "awake"
                        }
                        Intent.ACTION_SCREEN_OFF -> _status.value = "screen off"
                    }
                }
            }
            val filter = IntentFilter().apply {
                addAction(Intent.ACTION_SCREEN_ON)
                addAction(Intent.ACTION_SCREEN_OFF)
            }
            try {
                if (Build.VERSION.SDK_INT >= 33) {
                    app.registerReceiver(r, filter, Context.RECEIVER_NOT_EXPORTED)
                } else {
                    @Suppress("UnspecifiedRegisterReceiverFlag")
                    app.registerReceiver(r, filter)
                }
                screenReceiver = r
            } catch (e: Exception) {
                _status.value = "no screen events: ${e.message}"
            }
        }
        job = scope.launch(Dispatchers.IO) { loop(app) }
        Log.d(TAG, "start: loop lanzado")
    }

    @Synchronized
    fun stop(app: Context) {
        job?.cancel()
        job = null
        try {
            screenReceiver?.let { app.unregisterReceiver(it) }
        } catch (e: Exception) {
            // Already gone; nothing to do.
        }
        screenReceiver = null
        IslandJinx.clear(app)
        _status.value = "off"
    }

    /** Acks what is on the island (the Cerrar button). Returns what was acked. */
    fun ackShown(app: Context): Set<Long> {
        val ids = shownIds
        shownIds = emptySet()
        IslandJinx.clear(app)
        ack(app, ids)
        return ids
    }

    fun ack(app: Context, ids: Set<Long>) {
        if (ids.isEmpty()) return
        val base = PhoneSettings.presenceUrl(app)
        val key = PhoneSettings.mochiKey(app)
        if (key.isEmpty()) return
        ioScope.launch {
            for (id in ids) {
                PresenceNet.post(base, "/api/mochi/ack", key,
                    JSONObject().put("id", id).put("by", DeviceActivity.myId).toString())
            }
        }
    }

    private suspend fun loop(app: Context) {
        while (currentCoroutineContext().isActive) {
            try {
                tick(app)
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Throwable) {
                _status.value = "error: ${e.message}"
                Log.w(TAG, "tick failed", e)
            }
            delay(TICK_MS)
        }
    }

    private fun tick(app: Context) {
        val base = PhoneSettings.presenceUrl(app)
        val key = PhoneSettings.mochiKey(app)
        Log.d(TAG, "tick: base=$base keyLen=${key.length}")
        if (key.isEmpty()) {
            DeviceActivity.onServer(app, false, null)
            _status.value = "falta la clave en ajustes"
            Log.d(TAG, "sin clave en prefs")
            return
        }
        val pm = app.getSystemService(Context.POWER_SERVICE) as PowerManager
        if (!pm.isInteractive) {
            _status.value = "screen off"
            Log.d(TAG, "pantalla apagada: no late")
            return
        }
        if (screenOnTs <= 0L) screenOnTs = System.currentTimeMillis()
        val beat = JSONObject()
            .put("device", DeviceActivity.myId)
            .put("idle_seconds", idleSeconds())
            .put("screen_on", true)
            .toString()
        val answer = PresenceNet.post(base, "/api/presence/heartbeat", key, beat)
            ?.let { runCatching { JSONObject(it) }.getOrNull() }
        // The server calls the laptop "pc"; our ids say "laptop". Its "vga" (nobody in use) is an answer too.
        val active = answer?.let { Arbiter.fromServer(it.optString("active_device", "?")) }
        if (answer != null) JinxBackground.set(readJinxMood(answer))
        Log.d(TAG, "heartbeat -> ${active ?: "SIN RESPUESTA"}")
        // What the server says goes to the arbitration, which also hears the laptop and may overrule
        // it when the two of us do not both reach the server (see Arbiter).
        DeviceActivity.onServer(app, active != null, active)
        if (active == null) {
            _status.value = "sin enlace"
            return
        }
        _status.value = if (DeviceActivity.active.value) "presencia aquí" else "presencia en otro dispositivo"
        val inboxRaw = PresenceNet.get(base, "/api/mochi/inbox?since_id=0", key)
        val inbox = inboxRaw?.let { runCatching { parseInbox(it) }.getOrNull() }
        Log.d(TAG, "inbox -> ${inbox?.size ?: "SIN RESPUESTA"} mensajes (active=$active)")
        if (inbox == null) return
        // Only the device Mochi is on shows Jinx's texts, and it does not ack them while another one is.
        if (!DeviceActivity.active.value) return
        val nowS = System.currentTimeMillis() / 1000
        val (stale, fresh) = inbox.partition { nowS - it.ts > 3600 }
        ack(app, stale.map { it.id }.toSet())
        shownIds = IslandJinx.publish(app, fresh, autoExpandAlert = true)
    }

    /** Jinx's background mood in a heartbeat answer (`jinx_mood`, one of Mochi's emotes), else null. Pure. */
    fun readJinxMood(answer: JSONObject): String? =
        com.agupta07505.smartisland.coucou.assist.JinxMood.validEmote(
            if (answer.isNull("jinx_mood")) null else answer.optString("jinx_mood"),
        )

    /** Pure parsing, so it can be unit-tested without Android. */
    fun parseInbox(body: String): List<PresenceMsg> {
        val arr = JSONObject(body).optJSONArray("messages") ?: return emptyList()
        val out = mutableListOf<PresenceMsg>()
        for (i in 0 until arr.length()) {
            val o = arr.optJSONObject(i) ?: continue
            val text = o.optString("text", "").trim()
            if (text.isEmpty()) continue
            out += PresenceMsg(
                id = o.optLong("id", 0L),
                text = text.take(500),
                mood = o.optString("mood", null).takeIf { !it.isNullOrEmpty() && it != "null" },
                kind = o.optString("kind", "info").takeIf { it in setOf("info", "warn", "alert") } ?: "info",
                source = o.optString("source", ""),
                ts = o.optLong("ts", System.currentTimeMillis() / 1000),
            )
        }
        return out
    }
}
