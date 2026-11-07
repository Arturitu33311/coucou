/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer

/** The recognizer to use: the system's own (Google's), never Jinx's placeholder, which always fails. */
internal fun pickRecognizer(context: Context): ComponentName? {
    val services = context.packageManager.queryIntentServices(Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).setPackage(null), 0)
        .ifEmpty { context.packageManager.queryIntentServices(Intent("android.speech.RecognitionService"), PackageManager.GET_META_DATA) }
        .mapNotNull { it.serviceInfo }
        .filter { it.packageName != context.packageName }
    val names = services.map { ComponentName(it.packageName, it.name) }
    return PREFERRED.firstNotNullOfOrNull { p -> names.firstOrNull { it.packageName == p } } ?: names.firstOrNull()
}

private val PREFERRED = listOf("com.google.android.tts", "com.google.android.googlequicksearchbox", "com.google.android.as")

/** What the microphone heard, live. Main thread only (it is a SpeechRecognizer). */
class VoiceInput(
    private val context: Context,
    private val onLevel: (Float) -> Unit,
    private val onPartial: (String) -> Unit,
    private val onFinal: (String) -> Unit,
    private val onEnd: (error: Int?) -> Unit,
) {
    private var recognizer: SpeechRecognizer? = null
    private var delivered = false

    fun available(): Boolean = pickRecognizer(context) != null || SpeechRecognizer.isRecognitionAvailable(context)

    fun start() {
        stop()
        delivered = false
        val comp = pickRecognizer(context)
        val r = if (comp != null) SpeechRecognizer.createSpeechRecognizer(context, comp) else SpeechRecognizer.createSpeechRecognizer(context)
        r.setRecognitionListener(object : RecognitionListener {
            override fun onReadyForSpeech(params: Bundle?) = Unit
            override fun onBeginningOfSpeech() = Unit
            override fun onBufferReceived(buffer: ByteArray?) = Unit
            override fun onEvent(eventType: Int, params: Bundle?) = Unit
            override fun onEndOfSpeech() = Unit
            override fun onRmsChanged(rmsdB: Float) = onLevel(levelOf(rmsdB))
            override fun onPartialResults(partialResults: Bundle?) {
                partialResults?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.let(onPartial)
            }
            override fun onResults(results: Bundle?) {
                val text = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull().orEmpty()
                delivered = true
                if (text.isNotBlank()) onFinal(text) else onEnd(SpeechRecognizer.ERROR_NO_MATCH)
                onEnd(null)
            }
            override fun onError(error: Int) {
                if (!delivered) onEnd(error)
            }
        })
        recognizer = r
        r.startListening(
            Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE, "es-MX")
                .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
                // The recognizer waits this long in silence before deciding Alan has finished: its default is ~1.5-2 s of
                // dead air before every answer, so ask for less (some recognizers ignore it).
                .putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_COMPLETE_SILENCE_LENGTH_MILLIS, 900L)
                .putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_POSSIBLY_COMPLETE_SILENCE_LENGTH_MILLIS, 700L)
                .putExtra(RecognizerIntent.EXTRA_CALLING_PACKAGE, context.packageName),
        )
    }

    /** Stops listening and takes what was heard so far. */
    fun finish() { recognizer?.stopListening() }

    fun stop() {
        recognizer?.run { cancel(); destroy() }
        recognizer = null
    }

    companion object {
        /** The recognizer's level (about -2…10 dB) as 0…1 for the orb. */
        fun levelOf(rmsdB: Float): Float = ((rmsdB + 2f) / 12f).coerceIn(0f, 1f)
    }
}
