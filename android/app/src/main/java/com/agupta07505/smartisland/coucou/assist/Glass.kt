/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.graphics.Bitmap
import android.os.Build
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlurEffect
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.TileMode
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlin.math.roundToInt

/**
 * The picture of the app under Jinx, taken by the system when the assistant was invoked. It only paints the frosted
 * glass of the panel (the part of it behind the panel, blurred): it is never read, stored or sent. Kept in memory for
 * this visit; never kept for the apps [SensitiveApps] names.
 */
object BackdropHolder {
    private val slot = MutableStateFlow<Bitmap?>(null)
    val flow: StateFlow<Bitmap?> get() = slot

    @Volatile var sensitive = false

    fun put(b: Bitmap?) { slot.value = if (sensitive) null else b }
    fun clear() { slot.value = null; sensitive = false }
}

private val Ink = Color(0xFF0B0B10)

/**
 * A pane of frosted glass: the [backdrop] (the screen behind, blurred), a tint so the text always reads, a soft
 * highlight and a thin edge of light. Without a backdrop (or before Android 12, which cannot blur) it is the same
 * pane with a deeper tint, so it still reads as glass and never as a flat box.
 */
@Composable
fun GlassCard(
    backdrop: Bitmap?,
    rootSize: IntSize,
    modifier: Modifier = Modifier,
    shape: Shape = RoundedCornerShape(30.dp),
    content: @Composable ColumnScope.() -> Unit,
) {
    var at by remember { mutableStateOf(Offset.Zero) }
    val blurred = backdrop != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && rootSize.width > 0
    Box(
        modifier
            .onGloballyPositioned { at = it.positionInRoot() }
            .clip(shape)
            .border(1.dp, Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.50f), Color.White.copy(alpha = 0.06f))), shape),
    ) {
        if (blurred && backdrop != null) {
            val image = remember(backdrop) { backdrop.asImageBitmap() }
            Canvas(Modifier.matchParentSize().graphicsLayer { renderEffect = BlurEffect(44f, 44f, TileMode.Clamp) }) {
                drawImage(image, dstOffset = IntOffset(-at.x.roundToInt(), -at.y.roundToInt()), dstSize = rootSize)
            }
        }
        // Tint: dark enough to read white text over any app; lighter when there is a blurred picture to show through.
        Box(Modifier.matchParentSize().background(Ink.copy(alpha = if (blurred) 0.52f else 0.86f)))
        // Sheen: light falling from the top edge.
        Box(Modifier.matchParentSize().background(Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.13f), Color.Transparent), endY = 220f)))
        Column(content = content)
    }
}
