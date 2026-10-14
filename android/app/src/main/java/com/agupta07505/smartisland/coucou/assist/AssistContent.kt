/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.rounded.CloudDownload
import androidx.compose.material.icons.rounded.MoreVert
import androidx.compose.material.icons.rounded.OfflineBolt
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.agupta07505.smartisland.coucou.agents.SessionInfo
import com.agupta07505.smartisland.coucou.agents.SessionState
import com.agupta07505.smartisland.coucou.ui.Chip
import com.agupta07505.smartisland.coucou.ui.CoucouColors
import com.agupta07505.smartisland.coucou.ui.EmptyState
import com.agupta07505.smartisland.coucou.ui.MochiEmerald
import com.agupta07505.smartisland.coucou.ui.MochiGlyph
import com.agupta07505.smartisland.coucou.ui.MochiMood
import com.agupta07505.smartisland.coucou.ui.ScreenHeader
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Jinx's tab: the conversation shared with Telegram, the other phone and the PC, what Claude Code is doing,
 * and one field to write. Stateless (the model is [AssistScreen]'s), so it can be drawn in a test.
 */
@Composable
internal fun AssistContent(
    ui: AssistUi,
    sessions: List<SessionInfo>,
    onSend: (String) -> Unit,
    onSync: () -> Unit,
    onProvision: () -> Unit,
    onDownloadModel: () -> Unit,
    onTestModel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var draft by remember { mutableStateOf("") }
    var menu by remember { mutableStateOf(false) }
    // What Jinx feels between messages (from the server): her Mochi wears it while nothing else moves him.
    val background = com.agupta07505.smartisland.coucou.presence.JinxBackground.emote.collectAsState().value
        .let { JinxMood.moodOf(it) }
    val list = rememberLazyListState()
    LaunchedEffect(ui.lines.size, ui.busy) {
        val last = ui.lines.size + if (ui.busy) 1 else 0
        if (last > 0) list.animateScrollToItem(last - 1)
    }
    fun send() {
        if (draft.isBlank() || ui.busy) return
        onSend(draft)
        draft = ""
    }

    Column(modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        ScreenHeader(
            title = "Jinx",
            subtitle = ui.badge.ifEmpty { "Tu asistente · conversación compartida" },
            leading = {
                Box(
                    Modifier.size(46.dp).clip(CircleShape).background(CoucouColors.AccentSoft),
                    contentAlignment = Alignment.Center,
                ) { MochiGlyph(size = 30.dp, tint = MochiEmerald, mood = JinxMood.headerMood(ui.lines, ui.busy, System.currentTimeMillis() / 1000.0, background)) }
            },
            actions = {
                Box {
                    IconButton(onClick = { menu = true }) {
                        Icon(Icons.Rounded.MoreVert, "Más", tint = CoucouColors.TextSecondary)
                    }
                    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                        DropdownMenuItem(text = { Text("Sincronizar con la laptop") }, onClick = { menu = false; onSync() })
                        DropdownMenuItem(text = { Text("Provisionar (1.ª vez)") }, onClick = { menu = false; onProvision() })
                        DropdownMenuItem(
                            text = { Text(if (ui.model == ModelState.Ready) "Probar el modelo local" else "Descargar el modelo local (2,6 GB)") },
                            onClick = { menu = false; if (ui.model == ModelState.Ready) onTestModel() else onDownloadModel() },
                            enabled = !ui.modelBusy,
                        )
                    }
                }
            },
        )

        if (sessions.isNotEmpty()) {
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(bottom = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                for (s in sessions) {
                    val (dot, what) = when (s.state) {
                        SessionState.Working -> CoucouColors.Info to "trabajando"
                        SessionState.NeedsYou -> CoucouColors.Warning to "te necesita"
                        SessionState.Idle -> CoucouColors.TextTertiary to "en espera"
                    }
                    Chip("${s.project} · $what", dot = dot)
                }
            }
        }

        ModelStrip(ui)

        AnimatedVisibility(visible = ui.notice.isNotEmpty()) {
            Text(
                ui.notice,
                color = CoucouColors.Warning,
                fontSize = 13.sp,
                modifier = Modifier.fillMaxWidth().padding(bottom = 6.dp).clip(RoundedCornerShape(12.dp))
                    .background(CoucouColors.Warning.copy(alpha = 0.10f)).padding(horizontal = 12.dp, vertical = 8.dp),
            )
        }

        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (ui.lines.isEmpty() && !ui.busy) {
                EmptyState(
                    title = "Habla con Jinx",
                    body = "Lo que le digas aquí también lo verá en Telegram y en el PC.",
                    mood = MochiMood.Listening,
                    modifier = Modifier.align(Alignment.Center),
                )
            } else {
                LazyColumn(
                    Modifier.fillMaxSize(),
                    state = list,
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                    contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = 8.dp),
                ) {
                    items(ui.lines, key = { it.id }) { l -> Bubble(l) }
                    if (ui.busy) item(key = "typing") { Typing() }
                }
            }
        }

        Row(
            Modifier.fillMaxWidth().padding(top = 6.dp, bottom = 10.dp),
            verticalAlignment = Alignment.Bottom,
        ) {
            TextField(
                value = draft,
                onValueChange = { draft = it },
                modifier = Modifier.weight(1f).heightIn(min = 52.dp),
                placeholder = { Text("Escríbele a Jinx", color = CoucouColors.TextTertiary) },
                enabled = !ui.busy,
                maxLines = 5,
                shape = RoundedCornerShape(26.dp),
                colors = TextFieldDefaults.colors(
                    focusedContainerColor = CoucouColors.SurfaceHigh,
                    unfocusedContainerColor = CoucouColors.SurfaceHigh,
                    disabledContainerColor = CoucouColors.Surface,
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent,
                    disabledIndicatorColor = Color.Transparent,
                    cursorColor = CoucouColors.Accent,
                    focusedTextColor = CoucouColors.Text,
                    unfocusedTextColor = CoucouColors.Text,
                ),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { send() }),
            )
            Spacer(Modifier.size(8.dp))
            IconButton(
                onClick = { send() },
                enabled = draft.isNotBlank() && !ui.busy,
                modifier = Modifier.size(52.dp),
                colors = IconButtonDefaults.iconButtonColors(
                    containerColor = CoucouColors.Accent,
                    contentColor = Color(0xFF04140D),
                    disabledContainerColor = CoucouColors.SurfaceHigh,
                    disabledContentColor = CoucouColors.TextTertiary,
                ),
            ) { Icon(Icons.AutoMirrored.Rounded.Send, "Enviar") }
        }
    }
}

/** The on-device model, only when there is something to say about it (downloading, half done, ready). */
@Composable
private fun ModelStrip(ui: AssistUi) {
    val text = when {
        ui.modelBusy && ui.modelPercent >= 0 -> "Descargando el modelo local · ${ui.modelPercent} %"
        ui.modelBusy -> "Modelo local ocupado…"
        ui.model == ModelState.Ready -> "Sin conexión también: modelo local listo"
        ui.model is ModelState.Partial -> "Modelo local a medio descargar · menú ⋮ para seguir"
        else -> return
    }
    Row(
        Modifier.fillMaxWidth().padding(bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (ui.model == ModelState.Ready) Icons.Rounded.OfflineBolt else Icons.Rounded.CloudDownload,
            null, tint = CoucouColors.TextTertiary, modifier = Modifier.size(16.dp),
        )
        Spacer(Modifier.size(6.dp))
        Text(text, color = CoucouColors.TextTertiary, fontSize = 12.sp)
    }
}

private val timeFormat = SimpleDateFormat("HH:mm", Locale.getDefault())

@Composable
private fun Bubble(l: ChatLine) {
    val mine = !l.fromJinx
    val shape = if (mine) RoundedCornerShape(20.dp, 20.dp, 6.dp, 20.dp) else RoundedCornerShape(20.dp, 20.dp, 20.dp, 6.dp)
    Column(
        Modifier.fillMaxWidth(),
        horizontalAlignment = if (mine) Alignment.End else Alignment.Start,
    ) {
        Column(
            Modifier
                .widthIn(max = 300.dp)
                .clip(shape)
                .background(if (mine) CoucouColors.Accent.copy(alpha = 0.22f) else CoucouColors.SurfaceHigh)
                .padding(horizontal = 14.dp, vertical = 9.dp),
        ) {
            Text(l.text, color = CoucouColors.Text, fontSize = 15.sp, lineHeight = 21.sp)
        }
        val meta = listOfNotNull(
            l.sourceLabel,
            l.ts.takeIf { it > 0 }?.let { timeFormat.format(Date((it * 1000).toLong())) },
        ).joinToString(" · ")
        if (meta.isNotEmpty()) {
            Text(meta, color = CoucouColors.TextTertiary, fontSize = 11.sp, modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp))
        }
    }
}

/** Jinx is answering. */
@Composable
private fun Typing() {
    Row(
        Modifier.clip(RoundedCornerShape(20.dp, 20.dp, 20.dp, 6.dp)).background(CoucouColors.SurfaceHigh)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        repeat(3) { i -> Box(Modifier.size(7.dp).clip(CircleShape).background(CoucouColors.TextSecondary.copy(alpha = 0.4f + i * 0.2f))) }
    }
}
