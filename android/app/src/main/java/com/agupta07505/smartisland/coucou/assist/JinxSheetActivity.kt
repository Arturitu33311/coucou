/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.agupta07505.smartisland.coucou.ui.CoucouColors
import com.agupta07505.smartisland.coucou.ui.CoucouTheme
import kotlin.math.roundToInt

/**
 * Jinx as a bottom sheet over the app Alan was using (the assistant gesture): the app stays visible behind a
 * scrim, so "what is this?" is asked looking at it. Tap the scrim, drag the handle down or press back to close;
 * "Abrir completa" opens the full app on Jinx's tab. The conversation is the same one ([AssistModel]).
 */
class JinxSheetActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            CoucouTheme {
                JinxSheet(
                    onClose = { finish() },
                    onOpenFull = {
                        startActivity(Intent(this, JinxAssistActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                        finish()
                    },
                )
            }
        }
    }
}

/** How far the handle must be dragged down to close the sheet. */
internal const val SHEET_CLOSE_DRAG_PX = 220f

@Composable
internal fun JinxSheet(onClose: () -> Unit, onOpenFull: () -> Unit) {
    val model = viewModel<AssistModel>()
    var drag by remember { mutableFloatStateOf(0f) }
    Box(
        Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.45f))
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                role = Role.Button,
                onClickLabel = "Cerrar Jinx",
                onClick = onClose,
            ),
    ) {
        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth()
                .imePadding()
                .fillMaxHeight(0.72f)
                .offset { IntOffset(0, drag.roundToInt()) }
                .clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp))
                .background(CoucouColors.Background)
                // Taps inside the sheet are not the scrim's.
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = {})
                .navigationBarsPadding(),
        ) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    Modifier.weight(1f).height(48.dp)
                        .pointerInput(Unit) {
                            detectVerticalDragGestures(
                                onDragEnd = { if (drag > SHEET_CLOSE_DRAG_PX) onClose() else drag = 0f },
                                onDragCancel = { drag = 0f },
                                onVerticalDrag = { _, dy -> drag = (drag + dy).coerceAtLeast(0f) },
                            )
                        }
                        .semantics {
                            contentDescription = "Panel de Jinx. Desliza hacia abajo para cerrar"
                            customActions = listOf(
                                CustomAccessibilityAction("Cerrar") { onClose(); true },
                                CustomAccessibilityAction("Abrir completa") { onOpenFull(); true },
                            )
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Box(Modifier.width(40.dp).height(4.dp).clip(RoundedCornerShape(2.dp)).background(Color.White.copy(alpha = 0.35f)))
                }
                TextButton(onClick = onOpenFull, modifier = Modifier.height(48.dp)) {
                    Text("Abrir completa", color = CoucouColors.Accent, fontSize = 13.sp)
                }
            }
            AssistScreen(model, Modifier.weight(1f))
        }
    }
}

