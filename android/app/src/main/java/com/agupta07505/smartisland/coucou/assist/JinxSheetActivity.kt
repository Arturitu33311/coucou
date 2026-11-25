/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Keyboard
import androidx.compose.material.icons.rounded.Mic
import androidx.compose.material.icons.rounded.OpenInFull
import androidx.compose.material.icons.rounded.Stop
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import com.agupta07505.smartisland.coucou.ui.Chip
import com.agupta07505.smartisland.coucou.ui.CoucouColors
import com.agupta07505.smartisland.coucou.ui.CoucouTheme
import com.agupta07505.smartisland.coucou.ui.MochiEmerald
import com.agupta07505.smartisland.coucou.ui.MochiGlyph
import com.agupta07505.smartisland.coucou.ui.MochiMood

/**
 * Jinx over the app Alan was using (the assistant gesture), covering as little as it can: the same capsule as
 * « OK Coucou » on the laptop (Mochi and the bars of the voice) with a keyboard and a microphone, and her short
 * answer above it. The rest of the screen stays visible and a tap on it closes
 * Jinx. "Abrir completa" opens the full app (the whole conversation). The conversation is the same one ([AssistModel]).
 */
class JinxSheetActivity : ComponentActivity() {
    /** The system's permission dialog pauses this screen: that is not leaving it, the screen Jinx sees stays. */
    @Volatile internal var askingPermission = false

    /** Counts invocations while this screen lives (it is single-task: the gesture again arrives as a new intent). */
    internal var invocations by androidx.compose.runtime.mutableIntStateOf(0)

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        invocations++
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            CoucouTheme {
                JinxDock(
                    onClose = { finish() },
                    onOpenFull = {
                        startActivity(Intent(this, JinxAssistActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                        finish()
                    },
                )
            }
        }
    }

    override fun onStop() {
        super.onStop()
        // The screen it read belongs to this visit.
        if (!askingPermission && !isChangingConfigurations) { ScreenHolder.clear(); BackdropHolder.clear() }
    }
}

private fun animationsOn(context: android.content.Context): Boolean =
    Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) > 0f

@Composable
internal fun JinxDock(onClose: () -> Unit, onOpenFull: () -> Unit) {
    val context = LocalContext.current
    val activity = context as? JinxSheetActivity
    val model = viewModel<AssistModel>()
    val ui by model.ui.collectAsState()
    val backdrop by BackdropHolder.flow.collectAsState()
    var rootSize by remember { mutableStateOf(androidx.compose.ui.unit.IntSize.Zero) }

    LaunchedEffect(activity?.invocations) { model.clearReply() }

    LifecycleResumeEffect(model) {
        model.onOpen()
        // Not forgetting the screen here: leaving is decided by the activity (onStop), not by a permission dialog.
        model.setShown(true)
        onPauseOrDispose { model.setShown(false, forgetScreen = false) }
    }

    var draft by remember { mutableStateOf("") }
    var listening by remember { mutableStateOf(false) }
    val speaking by JinxSpeech.speaking.collectAsState()
    var level by remember { mutableFloatStateOf(0f) }
    var heard by remember { mutableStateOf("") }
    var byVoice by remember { mutableStateOf(false) }
    var voiceNote by remember { mutableStateOf<String?>(null) }
    var typing by remember { mutableStateOf(false) }
    val focus = remember { androidx.compose.ui.focus.FocusRequester() }
    val animate = remember { animationsOn(context) }

    val voice = remember {
        VoiceInput(
            context,
            onLevel = { level = it },
            onPartial = { heard = it },
            onFinal = { text ->
                heard = ""
                byVoice = true
                model.send(text)
            },
            onEnd = { error ->
                listening = false
                level = 0f
                if (error != null && error != android.speech.SpeechRecognizer.ERROR_NO_MATCH && error != android.speech.SpeechRecognizer.ERROR_SPEECH_TIMEOUT) {
                    voiceNote = "No te pude escuchar (error $error)."
                } else if (error != null) {
                    voiceNote = "No te oí. Intenta otra vez."
                }
                heard = ""
            },
        )
    }
    DisposableEffect(Unit) { onDispose { voice.stop(); JinxSpeech.stop() } }

    // A new answer: read it aloud if the question was spoken.
    LaunchedEffect(ui.replySeq) {
        if (ui.replySeq > 0 && byVoice) ui.reply?.let { JinxSpeech.say(context, it) }
    }

    fun startListening() {
        JinxSpeech.stop()
        voiceNote = null
        if (!voice.available()) { voiceNote = "Este teléfono no tiene reconocimiento de voz."; return }
        listening = true
        heard = ""
        voice.start()
    }

    val askMic = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        activity?.askingPermission = false
        if (ok) startListening() else voiceNote = "Sin permiso del micrófono no puedo escucharte; puedes escribir."
    }
    fun onMic() {
        if (listening) { voice.finish(); return }
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
            startListening()
        } else {
            activity?.askingPermission = true
            askMic.launch(Manifest.permission.RECORD_AUDIO)
        }
    }
    fun sendTyped() {
        if (draft.isBlank() || ui.busy) return
        byVoice = false
        voiceNote = null
        model.send(draft)
        draft = ""
        typing = false
    }

    val phase = when {
        listening -> VoicePhase.Listening
        ui.busy -> VoicePhase.Thinking
        speaking -> VoicePhase.Speaking
        else -> VoicePhase.Idle
    }
    // The laptop's bubble: what it heard (quieter) and her answer, above the capsule.
    val heardLine: String? = when {
        listening && heard.isNotBlank() -> heard
        listening -> "Te escucho…"
        else -> null
    }
    val answerLine: String? = when {
        listening || ui.busy -> null
        voiceNote != null -> voiceNote
        ui.reply != null -> ui.reply
        ui.notice.isNotEmpty() -> ui.notice
        else -> null
    }
    val mood = when (phase) {
        VoicePhase.Listening -> MochiMood.Listening
        VoicePhase.Thinking -> MochiMood.Thinking
        VoicePhase.Speaking, VoicePhase.Idle -> (if (phase == VoicePhase.Speaking || ui.reply != null) JinxMood.moodOfText(ui.reply) else null) ?: MochiMood.Idle
    }
    val phaseLabel = when (phase) {
        VoicePhase.Idle -> "Jinx"
        VoicePhase.Listening -> "Jinx te está escuchando"
        VoicePhase.Thinking -> "Jinx está pensando"
        VoicePhase.Speaking -> "Jinx está hablando"
    }

    // The whole window is transparent: a tap anywhere outside the dock closes Jinx.
    Box(
        Modifier.fillMaxSize().onSizeChanged { rootSize = it }.clickable(
            interactionSource = remember { MutableInteractionSource() },
            indication = null,
            role = Role.Button,
            onClickLabel = "Cerrar Jinx",
            onClick = onClose,
        ),
    ) {
        // One pane of frosted glass holds the bubble, the chips and the capsule; the app stays visible around it.
        GlassCard(
            backdrop = backdrop,
            rootSize = rootSize,
            modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth()
                .imePadding()
                .navigationBarsPadding()
                .padding(start = 10.dp, end = 10.dp, bottom = 8.dp)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = {}),
        ) {
            AnimatedVisibility(visible = heardLine != null || answerLine != null) {
                Column(
                    Modifier.fillMaxWidth()
                        .padding(horizontal = 18.dp, vertical = 14.dp)
                        .semantics { liveRegion = LiveRegionMode.Polite },
                ) {
                    heardLine?.let { Text(it, color = CoucouColors.TextSecondary, fontSize = 15.sp, lineHeight = 21.sp, maxLines = 3, overflow = TextOverflow.Ellipsis) }
                    answerLine?.let { Text(it, color = CoucouColors.Text, fontSize = 16.sp, lineHeight = 22.sp, maxLines = 8, overflow = TextOverflow.Ellipsis) }
                }
            }

            ui.screenApp?.let { app ->
                Row(
                    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 10.dp, vertical = 2.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Chip("Viendo: $app · quitar", dot = CoucouColors.Accent, onClick = { model.dropScreen() })
                    for (q in listOf("¿Qué hay aquí?", "Resume", "Traduce")) {
                        Chip(q, tint = CoucouColors.Text, onClick = { byVoice = false; model.send(q) })
                    }
                }
            }

            // The capsule, as on the laptop: Mochi and the bars of the voice; the keyboard and the microphone at its end.
            Row(
                Modifier.fillMaxWidth().padding(6.dp).heightIn(min = 56.dp).clip(RoundedCornerShape(26.dp))
                    .background(Color.White.copy(alpha = 0.08f)).padding(start = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                MochiGlyph(size = 32.dp, tint = MochiEmerald, mood = mood, modifier = Modifier.semantics { contentDescription = phaseLabel })
                Spacer(Modifier.size(10.dp))
                if (typing && !listening) {
                    TextField(
                        value = draft,
                        onValueChange = { draft = it },
                        modifier = Modifier.weight(1f).focusRequester(focus),
                        placeholder = { Text("Escríbele a Jinx", color = CoucouColors.TextTertiary) },
                        enabled = !ui.busy,
                        maxLines = 3,
                        colors = TextFieldDefaults.colors(
                            focusedContainerColor = Color.Transparent,
                            unfocusedContainerColor = Color.Transparent,
                            disabledContainerColor = Color.Transparent,
                            focusedIndicatorColor = Color.Transparent,
                            unfocusedIndicatorColor = Color.Transparent,
                            disabledIndicatorColor = Color.Transparent,
                            cursorColor = CoucouColors.Accent,
                            focusedTextColor = CoucouColors.Text,
                            unfocusedTextColor = CoucouColors.Text,
                        ),
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                        keyboardActions = KeyboardActions(onSend = { sendTyped() }),
                    )
                    LaunchedEffect(Unit) { focus.requestFocus() }
                    IconButton(onClick = { if (draft.isBlank()) typing = false else sendTyped() }, enabled = !ui.busy) {
                        Icon(
                            if (draft.isBlank()) Icons.Rounded.Close else Icons.AutoMirrored.Rounded.Send,
                            contentDescription = if (draft.isBlank()) "Cerrar el teclado" else "Enviar",
                            tint = CoucouColors.Text,
                        )
                    }
                } else {
                    Box(
                        Modifier.weight(1f).heightIn(min = 48.dp).clickable(role = Role.Button, onClickLabel = if (speaking) "Callar a Jinx" else "Hablar con Jinx") {
                            if (speaking) JinxSpeech.stop() else onMic()
                        },
                        contentAlignment = Alignment.CenterStart,
                    ) {
                        if (phase == VoicePhase.Idle) {
                            Text("Pregúntale a Jinx", color = CoucouColors.TextTertiary, fontSize = 15.sp)
                        } else {
                            VoiceBars(phase, level, color = if (listening) CoucouColors.Accent else CoucouColors.TextSecondary, animate = animate)
                        }
                    }
                    IconButton(onClick = { JinxSpeech.stop(); typing = true }, enabled = !listening && !ui.busy) {
                        Icon(Icons.Rounded.Keyboard, "Escribir", tint = CoucouColors.TextSecondary)
                    }
                    Box(
                        Modifier.padding(end = 4.dp).size(48.dp).clip(CircleShape)
                            .background(if (listening) CoucouColors.Accent else CoucouColors.Surface)
                            .clickable(role = Role.Button, enabled = !ui.busy, onClickLabel = if (listening) "Terminar de hablar" else "Hablar con Jinx") { onMic() },
                        contentAlignment = Alignment.Center,
                    ) {
                        Icon(
                            if (listening) Icons.Rounded.Stop else Icons.Rounded.Mic,
                            contentDescription = null,
                            tint = if (listening) Color(0xFF04140D) else CoucouColors.Text,
                        )
                    }
                }
                IconButton(
                    onClick = onOpenFull,
                    modifier = Modifier.semantics {
                        customActions = listOf(CustomAccessibilityAction("Cerrar Jinx") { onClose(); true })
                    },
                ) { Icon(Icons.Rounded.OpenInFull, "Abrir la conversación completa", tint = CoucouColors.TextSecondary) }
            }
        }
    }
}
