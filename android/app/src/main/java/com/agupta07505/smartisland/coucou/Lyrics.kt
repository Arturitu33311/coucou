/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.content.Context
import android.os.SystemClock
import android.util.Log
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.lyricsOn
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import kotlin.concurrent.thread
import org.json.JSONArray
import org.json.JSONObject

/**
 * Time-synced lyrics of the song that is playing, for the car wallpaper. Opt-in (Settings, off by default) because the
 * artist, title and album are sent to LRCLIB (lrclib.net, a free community database, no account, no key). Lyrics are
 * cached in `filesDir/lyrics`, so each song is looked up once.
 */
object Lyrics {
    class Line(val ms: Long, val text: String)

    private const val TAG = "Lyrics"
    private const val API = "https://lrclib.net/api"
    private const val AGENT = "Coucou-Android (https://github.com/Arturitu33311/coucou)"

    /** Lines of the current track, sorted by time; empty while loading, when the song has none, or when the switch is off. */
    @Volatile var lines: List<Line> = emptyList()
        private set

    @Volatile private var trackKey = ""
    @Volatile private var failedAt = 0L
    @Volatile private var basePos = 0L
    @Volatile private var baseStamp = 0L
    @Volatile private var speed = 0f

    /** The player's position when it last told us, advanced by the elapsed time while it plays. */
    fun positionMs(): Long = basePos + if (speed > 0f) ((SystemClock.elapsedRealtime() - baseStamp) * speed).toLong() else 0L

    /** [stamp] is `PlaybackState.getLastPositionUpdateTime()` (elapsedRealtime). */
    fun onState(positionMs: Long, playbackSpeed: Float, playing: Boolean, stamp: Long) {
        basePos = positionMs
        baseStamp = if (stamp > 0L) stamp else SystemClock.elapsedRealtime()
        speed = if (playing) playbackSpeed.coerceAtLeast(0f).let { if (it == 0f) 1f else it } else 0f
    }

    fun onTrack(context: Context, key: String, artist: String, title: String, album: String, durationMs: Long) {
        val retry = failedAt != 0L && SystemClock.elapsedRealtime() - failedAt > 60_000L
        if (key == trackKey && !retry) return
        trackKey = key
        failedAt = 0L
        lines = emptyList()
        val app = context.applicationContext
        if (!app.lyricsOn || title.isBlank()) return
        thread(name = "lyrics") {
            val found = runCatching { load(app, artist, title, album, durationMs / 1000) }
                .onFailure {
                    Log.i(TAG, "lyrics for $title not loaded: ${it.message}")
                    if (trackKey == key) failedAt = SystemClock.elapsedRealtime()
                }
                .getOrNull()
            if (found != null) Log.i(TAG, "lyrics for $title: ${found.size} lines")
            if (trackKey == key && found != null) lines = found
        }
    }

    /** The switch was turned off: forget what is loaded. */
    fun clear() {
        trackKey = ""
        lines = emptyList()
    }

    /** Index of the line being sung at [posMs], or -1 before the first one. */
    fun indexAt(list: List<Line>, posMs: Long): Int {
        var lo = 0
        var hi = list.size - 1
        var ans = -1
        while (lo <= hi) {
            val mid = (lo + hi) ushr 1
            if (list[mid].ms <= posMs) { ans = mid; lo = mid + 1 } else hi = mid - 1
        }
        return ans
    }

    /** "[01:23.45] text" (several time tags per line allowed); tags like [ar:Artist] are ignored. */
    fun parse(lrc: String): List<Line> {
        val tag = Regex("""\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?]""")
        val out = ArrayList<Line>()
        for (raw in lrc.lineSequence()) {
            val tags = tag.findAll(raw).toList()
            if (tags.isEmpty()) continue
            val text = raw.substring(tags.last().range.last + 1).trim()
            for (m in tags) {
                val frac = m.groupValues[3]
                val fracMs = if (frac.isEmpty()) 0L else (frac.padEnd(3, '0').take(3)).toLong()
                out += Line(m.groupValues[1].toLong() * 60_000L + m.groupValues[2].toLong() * 1000L + fracMs, text)
            }
        }
        return out.sortedBy { it.ms }
    }

    private fun load(app: Context, artist: String, title: String, album: String, durationSec: Long): List<Line> {
        val dir = File(app.filesDir, "lyrics").apply { mkdirs() }
        val f = File(dir, Integer.toHexString("$artist|$title|$album|$durationSec".hashCode()) + ".lrc")
        // an empty file means "looked up, none": believed for a week; a server error (thrown by [get]) is never cached
        if (f.exists() && (f.length() > 0L || System.currentTimeMillis() - f.lastModified() < 7 * 86_400_000L)) return parse(f.readText())
        val lrc = fetch(artist, title, album, durationSec)
        f.writeText(lrc.orEmpty())
        return parse(lrc.orEmpty())
    }

    private fun fetch(artist: String, title: String, album: String, durationSec: Long): String? {
        val q = StringBuilder("artist_name=${enc(artist)}&track_name=${enc(title)}")
        if (album.isNotBlank()) q.append("&album_name=${enc(album)}")
        if (durationSec > 0) q.append("&duration=$durationSec")
        (get("$API/get?$q") as? JSONObject)?.optString("syncedLyrics")?.takeIf { it.isNotBlank() && it != "null" }?.let { return it }
        // the exact lookup is strict (an extra "- Remastered" breaks it): fall back to a search with the cleaned title
        val clean = title.replace(Regex("""\s*[(\[].*?[)\]]"""), "").replace(Regex("""\s+-\s+.*$"""), "").trim()
        // players join several artists ("A, B, C"): the database lists the first
        val first = artist.split(",", " & ", " feat.", " ft.").first().trim()
        val hits = get("$API/search?track_name=${enc(clean)}&artist_name=${enc(first)}") as? JSONArray ?: return null
        for (i in 0 until hits.length()) {
            val o = hits.optJSONObject(i) ?: continue
            val synced = o.optString("syncedLyrics")
            val close = durationSec <= 0 || kotlin.math.abs(o.optDouble("duration", 0.0) - durationSec) <= 4
            if (synced.isNotBlank() && synced != "null" && close) return synced
        }
        return null
    }

    /** null = the database has no such song (404); any other failure, e.g. "server busy" (503), throws after a few tries. */
    private fun get(url: String): Any? {
        var last: Exception? = null
        repeat(3) { attempt ->
            if (attempt > 0) Thread.sleep(2500L * attempt)
            val c = URL(url).openConnection() as HttpURLConnection
            try {
                c.connectTimeout = 8000
                c.readTimeout = 8000
                c.setRequestProperty("User-Agent", AGENT)
                val code = c.responseCode
                if (code == 404) return null
                if (code != 200) { last = java.io.IOException("HTTP $code"); return@repeat }
                val body = c.inputStream.bufferedReader().use { it.readText() }
                return if (body.trimStart().startsWith("[")) JSONArray(body) else JSONObject(body)
            } catch (e: java.io.IOException) {
                last = e
            } finally {
                c.disconnect()
            }
        }
        throw last ?: java.io.IOException("no answer")
    }

    private fun enc(s: String) = URLEncoder.encode(s, "UTF-8")
}
