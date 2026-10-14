/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.sin

/** How long the padlock stays on the island after an unlock. */
const val LOCK_BADGE_MS = 2_200L

/**
 * The classic padlock opening on the island when the phone is unlocked: the shackle springs open
 * in the first third (with a little "click" squash), a ring ripples out, and the whole badge
 * fades away. One straight 2.2 s run per unlock — no loops, so it never outstays its welcome.
 */
@Composable
fun LockOpeningBadge(size: Dp = 44.dp) {
    val context = LocalContext.current
    val progress = remember { Animatable(0f) }
    LaunchedEffect(Unit) { progress.animateTo(1f, tween(durationMillis = LOCK_BADGE_MS.toInt(), easing = LinearEasing)) }
    // The click you feel: one short buzz exactly when the shackle lets go (~35% in), so the
    // animation is heard by the hand as well as the eye.
    LaunchedEffect(Unit) {
        kotlinx.coroutines.delay((LOCK_BADGE_MS * 0.35).toLong())
        Haptics.click(context)
    }
    val p = progress.value
    // The shackle opens during the first third; the "click" squash lands right as it lets go.
    val open = (p / 0.35f).coerceIn(0f, 1f).let { it * it * (3 - 2 * it) }
    val click = if (p in 0.30f..0.50f) 1f - 0.08f * sin((p - 0.30f) / 0.20f * PI).toFloat() else 1f
    // Hold, then fade out over the last quarter.
    val alpha = if (p < 0.72f) 1f else 1f - (p - 0.72f) / 0.28f
    // The ripple: grows and dissolves while the shackle opens.
    val ripple = (p / 0.8f).coerceIn(0f, 1f)
    Canvas(
        Modifier
            .size(size)
            .graphicsLayer {
                this.alpha = alpha.coerceIn(0f, 1f)
                scaleX = click
                scaleY = click
            },
    ) {
        val w = this.size.width
        val cx = w / 2f
        val lock = Color.White
        // A dark coin behind it: the pill underneath may be bright (album artwork), and a white
        // lock needs a dark stage.
        drawCircle(Color.Black.copy(alpha = 0.45f), radius = w * 0.48f, center = Offset(cx, w * 0.52f))
        if (ripple in 0.02f..0.99f) {
            drawCircle(
                lock.copy(alpha = 0.45f * (1f - ripple)),
                radius = w * (0.42f + ripple * 0.30f),
                center = Offset(cx, w * 0.54f),
                style = Stroke(width = w * 0.045f),
            )
        }
        val bodyW = w * 0.52f
        val bodyH = w * 0.42f
        val bodyTop = w * 0.44f
        val left = cx - bodyW / 2f
        drawRoundRect(
            color = lock,
            topLeft = Offset(left, bodyTop),
            size = Size(bodyW, bodyH),
            cornerRadius = CornerRadius(w * 0.09f, w * 0.09f),
        )
        // The keyhole: a dot with a slot under it, punched in the island's dark ink.
        val holeC = Offset(cx, bodyTop + bodyH * 0.36f)
        drawCircle(Color(0xFF1B1B20), radius = w * 0.055f, center = holeC)
        drawRoundRect(
            color = Color(0xFF1B1B20),
            topLeft = Offset(holeC.x - w * 0.028f, holeC.y),
            size = Size(w * 0.056f, bodyH * 0.26f),
            cornerRadius = CornerRadius(w * 0.028f, w * 0.028f),
        )
        // The shackle: a stroked inverted-U. Its right leg swings up around the left base as it
        // opens (drawn in a rotated frame so the left leg stays planted in the body).
        val shackleW = w * 0.055f
        val legTop = bodyTop - w * 0.02f
        val archR = bodyW * 0.32f
        val archCY = legTop - w * 0.10f
        // Left leg (planted): from the body up to the arch.
        drawLine(
            lock,
            Offset(cx - archR, bodyTop + w * 0.02f),
            Offset(cx - archR, archCY),
            strokeWidth = shackleW,
            cap = StrokeCap.Round,
        )
        rotate(degrees = -38f * open, pivot = Offset(cx - archR, archCY)) {
            val arm = Path().apply {
                // Over the arch…
                arcTo(
                    Rect(cx - archR, archCY - archR, cx + archR, archCY + archR),
                    startAngleDegrees = 180f,
                    sweepAngleDegrees = 180f,
                    forceMoveTo = false,
                )
                // …and down the right leg into the body.
                lineTo(cx + archR, bodyTop + w * 0.10f)
            }
            drawPath(arm, lock, style = Stroke(width = shackleW, cap = StrokeCap.Round))
        }
    }
}
