/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.media.MediaPlayer
import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import com.agupta07505.smartisland.coucou.phone.PhoneSettings
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.json.JSONObject
import java.io.File
import java.util.Locale
import java.util.concurrent.Executors

/** POST [body] as JSON with a bearer [key]; returns the response bytes or throws. For audio. */
fun httpPostBytes(url: String, key: String, body: String): ByteArray {
    val c = java.net.URL(url).openConnection() as java.net.HttpURLConnection
    try {
        c.requestMethod = "POST"
        c.connectTimeout = 8_000
        c.readTimeout = 40_000
        c.doOutput = true
        c.setRequestProperty("Content-Type", "application/json")
        if (key.isNotEmpty()) c.setRequestProperty("Authorization", "Bearer $key")
        c.outputStream.use { it.write(body.toByteArray()) }
        if (c.responseCode !in 200..299) throw java.io.IOException("HTTP ${c.responseCode}")
        return c.inputStream.use { it.readBytes() }
    } finally {
        c.disconnect()
    }
}

/**
 * Jinx's own voice (ElevenLabs: the one of « OK Coucou » and the headphones) from the server's bridge: the text goes
 * there over https with the bridge's key and an MP3 comes back. The ElevenLabs key never reaches the phone.
 * Null when it is not set up or the server cannot answer (then the system's voice speaks).
 */
class JinxVoice(
    private val secrets: Secrets,
    private val baseUrl: () -> String,
    private val cacheDir: File,
    private val post: (url: String, key: String, body: String) -> ByteArray = ::httpPostBytes,
) {
    fun fetch(text: String): File? = try {
        val key = secrets.get(SecretNames.G2_KEY)?.takeIf { it.isNotBlank() }
        val base = baseUrl().trim().trimEnd('/').takeIf { it.startsWith("https://") }
        if (key == null || base == null) {
            null
        } else {
            val mp3 = post("$base/api/g2/speak", key, JSONObject().put("text", text).toString())
            tidy()
            File(cacheDir.apply { mkdirs() }, "jinx-voice-${System.nanoTime()}.mp3").also { it.writeBytes(mp3) }
        }
    } catch (_: Exception) {
        null
    }

    /** An open stream of Jinx's voice, raw PCM; [close] ends the connection (it also unblocks a read). */
    class PcmStream(val input: java.io.InputStream, private val conn: java.net.HttpURLConnection, val rate: Int) : java.io.Closeable {
        override fun close() { runCatching { input.close() }; conn.disconnect() }
    }

    /**
     * Starts Jinx's voice as a stream (the server passes on what ElevenLabs makes as it makes it, as « OK Coucou » does
     * on the laptop): 16-bit mono PCM, played while it arrives. Null when not set up or the server cannot.
     */
    fun openStream(text: String): PcmStream? = try {
        val key = secrets.get(SecretNames.G2_KEY)?.takeIf { it.isNotBlank() }
        val base = baseUrl().trim().trimEnd('/').takeIf { it.startsWith("https://") }
        if (key == null || base == null) {
            null
        } else {
            val c = java.net.URL("$base/api/g2/speak/stream").openConnection() as java.net.HttpURLConnection
            c.requestMethod = "POST"
            c.connectTimeout = 6_000
            c.readTimeout = 20_000
            c.doOutput = true
            c.setRequestProperty("Content-Type", "application/json")
            c.setRequestProperty("Authorization", "Bearer $key")
            c.outputStream.use { it.write(JSONObject().put("text", text).toString().toByteArray()) }
            if (c.responseCode !in 200..299) {
                c.disconnect()
                null
            } else {
                PcmStream(c.inputStream, c, c.getHeaderField("X-Audio-Rate")?.toIntOrNull() ?: 24_000)
            }
        }
    } catch (_: Exception) {
        null
    }

    /** Spoken files are deleted after playing; this takes the ones a crash left behind. */
    private fun tidy() {
        val old = System.currentTimeMillis() - 10 * 60_000
        cacheDir.listFiles { f -> f.name.startsWith("jinx-voice-") && f.lastModified() < old }?.forEach { it.delete() }
    }
}

/**
 * Jinx speaking on the phone: her own voice when the server gives it, else the system's. Everything runs on the main
 * thread except the download. [onState] says whether she is (about to be) speaking.
 */
class Speaker(
    context: Context,
    private val remote: ((String) -> File?)? = null,
    /** Her voice as a stream (preferred: it starts sooner); [remote] (a whole MP3) is the way back if it fails. */
    private val remoteStream: ((String) -> JinxVoice.PcmStream?)? = null,
    private val onState: (speaking: Boolean) -> Unit,
) {
    private val app = context.applicationContext
    private val main = Handler(Looper.getMainLooper())
    // Two downloads at once: while the first sentence plays, the next is already on its way.
    private val io = Executors.newFixedThreadPool(2)
    private val waiter = Executors.newSingleThreadExecutor()
    private var token = 0
    private var player: MediaPlayer? = null
    @Volatile private var track: AudioTrack? = null
    @Volatile private var openStream: JinxVoice.PcmStream? = null
    private var tts: TextToSpeech? = null
    private var ttsReady = false
    private var ttsPending: String? = null

    /** Reads [text] aloud. [tagged] (with ElevenLabs's audio tags) is what her own voice gets; the system's gets [text]. */
    fun say(text: String, tagged: String = text) {
        val plain = spokenForm(text)
        if (plain.isEmpty()) return
        main.post {
            stopNow()
            val mine = ++token
            onState(true)
            val t0 = android.os.SystemClock.elapsedRealtime()
            val full = remoteForm(tagged)
            waiter.execute {
                // First choice: the stream, which starts to sound when ElevenLabs's first fragment arrives (~1 s).
                val streamed = remoteStream != null && streamPcm(full, mine, t0)
                main.post {
                    if (mine != token) return@post
                    if (streamed) onState(false) else fetchByParts(full, plain, mine, t0)
                }
            }
        }
    }

    /**
     * The way back: whole MP3s, in sentences. A long answer is cut: the first one starts to sound as soon as it is
     * ready instead of after the whole answer has been synthesized.
     */
    private fun fetchByParts(full: String, plain: String, mine: Int, t0: Long) {
        val parts = splitForSpeech(full)
        val fetches = parts.map { part -> io.submit(java.util.concurrent.Callable { runCatching { remote?.invoke(part) }.getOrNull() }) }
        playFrom(fetches, parts, 0, mine, t0)
    }

    /**
     * Plays [text] as it streams in: raw PCM into an AudioTrack, written as it arrives. Runs on the waiter thread.
     * True when it played (to the end, or until Alan cut it off); false when there was no audio at all, so the caller
     * can go on with the MP3s.
     */
    private fun streamPcm(text: String, mine: Int, t0: Long): Boolean {
        val s = runCatching { remoteStream?.invoke(text) }.getOrNull() ?: return false
        openStream = s
        var at: AudioTrack? = null
        var started = false
        try {
            val rate = s.rate
            val min = AudioTrack.getMinBufferSize(rate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
            at = AudioTrack.Builder()
                .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANT).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
                .setAudioFormat(AudioFormat.Builder().setSampleRate(rate).setEncoding(AudioFormat.ENCODING_PCM_16BIT).setChannelMask(AudioFormat.CHANNEL_OUT_MONO).build())
                .setBufferSizeInBytes(maxOf(min, rate / 2 * 2))
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build()
            track = at
            val buf = ByteArray(8192)
            var carry = -1
            var frames = 0L
            while (mine == token) {
                val n = s.input.read(buf)
                if (n < 0) break
                if (n == 0) continue
                if (!started) {
                    started = true
                    at.play()
                    android.util.Log.i("JinxLatency", "voice stream first audio in ${android.os.SystemClock.elapsedRealtime() - t0} ms")
                }
                // PCM samples are two bytes: a read can end in the middle of one, keep the odd byte for the next.
                var off = 0
                var len = n
                if (carry >= 0) {
                    val joined = byteArrayOf(carry.toByte(), buf[0])
                    at.write(joined, 0, 2)
                    frames++
                    carry = -1
                    off = 1
                    len = n - 1
                }
                if (len % 2 == 1) { carry = buf[off + len - 1].toInt() and 0xFF; len -= 1 }
                if (len > 0) { at.write(buf, off, len); frames += len / 2 }
            }
            if (started && mine == token) {
                // The end of the stream: let what is written finish sounding, then stop.
                at.stop()
                val deadline = android.os.SystemClock.elapsedRealtime() + 8_000
                while (mine == token && at.playbackHeadPosition < frames && android.os.SystemClock.elapsedRealtime() < deadline) Thread.sleep(40)
            }
            return started
        } catch (_: Exception) {
            return started
        } finally {
            track = null
            openStream = null
            runCatching { at?.release() }
            s.close()
        }
    }

    /** Waits for part [i], plays it, and goes on with the next; if one cannot be had, the system's voice reads the rest. */
    private fun playFrom(fetches: List<java.util.concurrent.Future<File?>>, parts: List<String>, i: Int, mine: Int, t0: Long) {
        waiter.execute {
            val file = runCatching { fetches[i].get() }.getOrNull()
            main.post {
                if (mine != token) { file?.delete(); fetches.drop(i + 1).forEach { it.cancel(true) }; return@post }
                if (i == 0) android.util.Log.i("JinxLatency", "voice part 1/${parts.size} ${if (file != null) "ready" else "unavailable"} in ${android.os.SystemClock.elapsedRealtime() - t0} ms")
                if (file == null) {
                    fetches.drop(i + 1).forEach { it.cancel(true) }
                    systemVoice(spokenForm(parts.drop(i).joinToString(" ")), mine)
                } else {
                    play(file, mine, spokenForm(parts.drop(i).joinToString(" "))) {
                        if (i + 1 < parts.size) playFrom(fetches, parts, i + 1, mine, t0) else onState(false)
                    }
                }
            }
        }
    }

    fun stop() { main.post { stopNow() } }

    private fun stopNow() {
        token++
        player?.run { runCatching { stop() }; release() }
        player = null
        track?.let { t -> runCatching { t.pause(); t.flush() } }
        openStream?.let { s -> io.execute { s.close() } }
        tts?.stop()
        ttsPending = null
        onState(false)
    }

    private fun play(file: File, mine: Int, fallback: String, onDone: () -> Unit) {
        try {
            val p = MediaPlayer()
            player = p
            p.setAudioAttributes(
                AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANT).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build(),
            )
            p.setDataSource(file.path)
            p.setOnCompletionListener { mp ->
                file.delete()
                if (mine == token) { player = null; mp.release(); onDone() }
            }
            p.setOnErrorListener { mp, _, _ ->
                mp.release(); player = null; file.delete()
                if (mine == token) systemVoice(fallback, mine)
                true
            }
            p.prepare()
            p.start()
            android.util.Log.i("JinxLatency", "voice playing")
        } catch (_: Exception) {
            file.delete()
            systemVoice(fallback, mine)
        }
    }

    private fun systemVoice(plain: String, mine: Int) {
        val engine = tts ?: TextToSpeech(app) { status ->
            if (status == TextToSpeech.SUCCESS) {
                tts?.let { e ->
                    listOf(Locale("es", "MX"), Locale("es", "US"), Locale("es", "ES"))
                        .firstOrNull { e.isLanguageAvailable(it) >= TextToSpeech.LANG_AVAILABLE }?.let { e.language = it }
                }
                ttsReady = true
                ttsPending?.let { p ->
                    ttsPending = null
                    if (mine == token) tts?.speak(p, TextToSpeech.QUEUE_FLUSH, null, "jinx")
                }
            } else {
                onState(false)
            }
        }.also { e ->
            tts = e
            e.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                override fun onStart(utteranceId: String?) = Unit
                override fun onDone(utteranceId: String?) { main.post { onState(false) } }
                @Deprecated("Deprecated in Java") override fun onError(utteranceId: String?) { main.post { onState(false) } }
            })
        }
        if (ttsReady) engine.speak(plain, TextToSpeech.QUEUE_FLUSH, null, "jinx") else ttsPending = plain
    }

    companion object {
        private val TAG = Regex("\\[[a-z][a-z ,'\\-]{1,40}\\]")
        private val LINK = Regex("https?://\\S+")
        private val MARKS = Regex("[*_`#>~]+")
        private val SPACE = Regex("\\s+")
        private val SENTENCE_END = Regex("(?<=[.!?…])\\s+")
        private const val FIRST_PART = 160

        /**
         * The text as parts to synthesize one after another: a short answer stays whole; a long one starts with its
         * first sentence(s), up to about [FIRST_PART] characters, and the rest follows as a second part.
         */
        fun splitForSpeech(text: String): List<String> {
            val t = text.trim()
            if (t.length <= FIRST_PART) return listOf(t)
            var head = ""
            for (s in t.split(SENTENCE_END)) {
                if (head.isNotEmpty() && head.length + 1 + s.length > FIRST_PART) break
                head = if (head.isEmpty()) s else "$head $s"
            }
            if (head.isEmpty() || head.length >= t.length) return listOf(t)
            val rest = t.removePrefix(head).trim()
            return if (rest.isEmpty()) listOf(t) else listOf(head, rest)
        }

        /** A reply as the system's voice should read it: no markdown, no links, no audio tags, bounded. */
        fun spokenForm(text: String): String =
            text.replace(TAG, "").replace(LINK, "").replace(MARKS, "").replace(SPACE, " ").trim().take(600)

        /** The text for her own voice: markdown and links out, the audio tags kept. */
        fun remoteForm(text: String): String =
            text.replace(LINK, "").replace(MARKS, "").replace(SPACE, " ").trim().take(600)
    }
}

/** One Speaker for the whole app: the panel, the chat and the read-aloud notices share it, so two never talk at once. */
object JinxSpeech {
    private val state = MutableStateFlow(false)
    val speaking: StateFlow<Boolean> get() = state

    @Volatile private var speaker: Speaker? = null

    private fun of(context: Context): Speaker = speaker ?: synchronized(this) {
        speaker ?: JinxVoice(KeystoreSecrets(context.applicationContext), { PhoneSettings.g2Url(context) }, File(context.applicationContext.cacheDir, "voice")).let { voice ->
            Speaker(context, remote = voice::fetch, remoteStream = voice::openStream) { state.value = it }.also { speaker = it }
        }
    }

    fun say(context: Context, text: String, tagged: String = text) = of(context).say(text, tagged)
    fun stop() { speaker?.stop() }
}
