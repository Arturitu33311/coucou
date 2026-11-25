/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Intent
import android.app.assist.AssistContent
import android.app.assist.AssistStructure
import android.os.Bundle
import android.service.voice.VoiceInteractionService
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.speech.RecognitionService
import android.speech.SpeechRecognizer

/** What Android binds to when Jinx is the chosen assistant. It has nothing to do until a session is shown. */
class JinxVoiceInteractionService : VoiceInteractionService()

class JinxSessionService : VoiceInteractionSessionService() {
    override fun onNewSession(args: Bundle?): VoiceInteractionSession = JinxSession(this)
}

/** The assistant gesture arrives here: Jinx opens as a bottom sheet over the app Alan was in (see [JinxSheetActivity]). */
class JinxSession(private val service: JinxSessionService) : VoiceInteractionSession(service) {
    override fun onShow(args: Bundle?, showFlags: Int) {
        super.onShow(args, showFlags)
        startAssistantActivity(
            Intent(service, JinxSheetActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        finish()
    }

    /** What Alan was looking at when he invoked Jinx: kept in memory for a few minutes, only for this phone's own brains. */
    @Suppress("OVERRIDE_DEPRECATION")
    override fun onHandleAssist(data: Bundle?, structure: AssistStructure?, content: AssistContent?) {
        super.onHandleAssist(data, structure, content)
        // A sensitive app (bank, wallet, passwords) leaves no screen and no picture behind the glass.
        val pkg = structure?.activityComponent?.packageName
        BackdropHolder.sensitive = pkg != null && SensitiveApps.isSensitive(pkg)
        if (BackdropHolder.sensitive) BackdropHolder.put(null)
        ScreenHolder.put(runCatching { ScreenReader.read(service, structure) }.getOrNull())
    }

    /** The system's picture of the app under Jinx: only to frost the glass behind the panel (see [BackdropHolder]). */
    override fun onHandleScreenshot(screenshot: android.graphics.Bitmap?) {
        super.onHandleScreenshot(screenshot)
        BackdropHolder.put(screenshot)
    }
}

/** Android's assistant list expects a recognition service; Jinx has no voice yet (phase 2), so it only says so. */
class JinxRecognitionService : RecognitionService() {
    override fun onStartListening(recognizerIntent: Intent?, listener: Callback?) {
        listener?.error(SpeechRecognizer.ERROR_CLIENT)
    }

    override fun onCancel(listener: Callback?) = Unit
    override fun onStopListening(listener: Callback?) = Unit
}
