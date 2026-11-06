/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Context
import android.media.AudioAttributes
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
    private val onState: (speaking: Boolean) -> Unit,
) {
    private val app = context.applicationContext
    private val main = Handler(Looper.getMainLooper())
    private val io = Executors.newSingleThreadExecutor()
    private var token = 0
    private var player: MediaPlayer? = null
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
            io.execute {
                val file = runCatching { remote?.invoke(remoteForm(tagged)) }.getOrNull()
                main.post {
                    if (mine != token) { file?.delete(); return@post }
                    if (file != null) play(file, mine, plain) else systemVoice(plain, mine)
                }
            }
        }
    }

    fun stop() { main.post { stopNow() } }

    private fun stopNow() {
        token++
        player?.run { runCatching { stop() }; release() }
        player = null
        tts?.stop()
        ttsPending = null
        onState(false)
    }

    private fun play(file: File, mine: Int, fallback: String) {
        try {
            val p = MediaPlayer()
            player = p
            p.setAudioAttributes(
                AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANT).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build(),
            )
            p.setDataSource(file.path)
            p.setOnCompletionListener { mp ->
                if (mine == token) { player = null; mp.release(); onState(false) }
                file.delete()
            }
            p.setOnErrorListener { mp, _, _ ->
                mp.release(); player = null; file.delete()
                if (mine == token) systemVoice(fallback, mine)
                true
            }
            p.prepare()
            p.start()
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
        speaker ?: Speaker(
            context,
            remote = JinxVoice(KeystoreSecrets(context.applicationContext), { PhoneSettings.g2Url(context) }, File(context.applicationContext.cacheDir, "voice"))::fetch,
        ) { state.value = it }.also { speaker = it }
    }

    fun say(context: Context, text: String, tagged: String = text) = of(context).say(text, tagged)
    fun stop() { speaker?.stop() }
}
