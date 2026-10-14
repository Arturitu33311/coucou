/*
 * Smart Island (2026)
 * © Animesh Gupta — github.com/agupta07505
 * Licensed under the GNU GPL v3 License
 * Do not remove or alter this notice. - Per GPL-3.0 Section 4 & Section 5
 */

package com.agupta07505.smartisland.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.cos
import kotlin.math.sin

@Composable
fun WavyMusicSeekBar(
    progress: Float,
    isPlaying: Boolean,
    onSeek: (Float) -> Unit,
    modifier: Modifier = Modifier,
    waveColor: Color = Color.White,
    trackColor: Color = Color(0xFF667085),
    thumbColor: Color = Color.White,
    waveHeight: Dp = 8.dp,
    trackThickness: Dp = 4.dp,
    thumbRadius: Dp = 8.dp,
    // Mochi rides the thumb while music plays (dancing) and rests when paused: the bar and the
    // mascot stay one thing instead of two decorations side by side.
    showMochi: Boolean = true,
) {
    val infiniteTransition = rememberInfiniteTransition(label = "wavySeekBarPhase")
    // The travelling shimmer is decorative (the bar's fill already tracks the real 33 fps position
    // above), so it runs at half its original 667 ms speed: calmer, less distracting.
    val phase by infiniteTransition.animateFloat(
        initialValue = 0f,
        targetValue = if (isPlaying) 6.2831855f else 0f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 667 * 2, easing = LinearEasing),
            repeatMode = RepeatMode.Restart
        ),
        label = "phase"
    )

    var isDragging by remember { mutableStateOf(false) }
    var dragProgress by remember { mutableStateOf(0f) }

    val activeProgress = if (isDragging) dragProgress else progress

    val density = LocalDensity.current
    // Pixel sizes hoisted out of the canvas so the wave builder below is a plain function.
    val baseThicknessPx = with(density) { trackThickness.toPx() }
    val maxWaveHeightPx = with(density) { waveHeight.toPx() }
    val thumbRadiusPx = with(density) { thumbRadius.toPx() }
    val fadeWidthPx = with(density) { WAVE_FADE_WIDTH.toPx() }

    BoxWithConstraints(
        modifier = modifier
            .fillMaxWidth()
            .height(24.dp)
    ) {
        // Clamped so he never leaves the bar; the upper bound is floored at zero so a
        // not-yet-measured (zero-width) bar cannot crash the clamp.
        val riderX = (maxWidth * activeProgress - RIDER_SIZE / 2).coerceIn(0.dp, (maxWidth - RIDER_SIZE).coerceAtLeast(0.dp))
        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .pointerInput(Unit) {
                    detectTapGestures { offset ->
                        val newProgress = (offset.x / size.width.toFloat()).coerceIn(0f, 1f)
                        dragProgress = newProgress
                        onSeek(newProgress)
                    }
                }
                .pointerInput(Unit) {
                    detectDragGestures(
                        onDragStart = { offset ->
                            isDragging = true
                            dragProgress = (offset.x / size.width.toFloat()).coerceIn(0f, 1f)
                        },
                        onDrag = { change, _ ->
                            change.consume()
                            dragProgress = (change.position.x / size.width.toFloat()).coerceIn(0f, 1f)
                        },
                        onDragEnd = {
                            isDragging = false
                            onSeek(dragProgress)
                        },
                        onDragCancel = {
                            isDragging = false
                        }
                    )
                }
        ) {
            val width = size.width
            val height = size.height
            val progressX = activeProgress * width

            val baselineY = height / 2f

            // One wave fill: an echo (opposite phase, faint, sunk a little) plus the main one.
            // The echo gives depth for one extra pass over the same pixels.
            fun wavePath(ph: Float, yShift: Float): Path {
                val path = Path()
                val startX = 0f
                val endX = progressX

                // Left cap height
                path.arcTo(
                    rect = Rect(
                        left = startX,
                        top = baselineY - baseThicknessPx / 2f,
                        right = startX + baseThicknessPx,
                        bottom = baselineY + baseThicknessPx / 2f
                    ),
                    startAngleDegrees = 90f,
                    sweepAngleDegrees = 180f,
                    forceMoveTo = true
                )

                // The wave should transition smoothly to flat at the left edge and near the thumb
                // transitionWidth: width over which the wave fades to flat (e.g. 24dp)
                // Generate path points along the wavy top edge
                // We start from the end of the left arc (startX + baseThicknessPx / 2f)
                val pathStartX = startX + baseThicknessPx / 2f
                if (endX > pathStartX) {
                    for (xInt in pathStartX.toInt()..endX.toInt()) {
                        val x = xInt.toFloat()

                        // Damp the wave near startX and endX so it smoothly meets the flat baseline
                        val dampLeft = ((x - pathStartX) / fadeWidthPx).coerceIn(0f, 1f)
                        val dampRight = ((endX - x) / fadeWidthPx).coerceIn(0f, 1f)
                        val damp = dampLeft * dampRight

                        // Premium-looking organic wave: combine two sine frequencies
                        // Using phase to shift the wave left/right
                        val waveVal = sin(x * WAVE_FREQ_1 - ph) * 0.6f + cos(x * WAVE_FREQ_2 + ph * 1.3f) * 0.4f
                        val normalizedWave = (waveVal + 1f) / 2f // 0f..1f

                        val y = baselineY + yShift - (baseThicknessPx / 2f) - (maxWaveHeightPx * damp * normalizedWave)
                        path.lineTo(x, y)
                    }
                }

                // Draw down to the bottom baseline at progressX
                path.lineTo(progressX, baselineY + yShift + baseThicknessPx / 2f)

                // Draw flat bottom baseline back to the start of the left arc
                path.lineTo(pathStartX, baselineY + yShift + baseThicknessPx / 2f)

                path.close()
                return path
            }

            // 1. Draw remaining track on the right (thin straight rounded bar)
            val remainingWidth = width - progressX
            if (remainingWidth > 0) {
                drawRoundRect(
                    color = trackColor,
                    topLeft = Offset(progressX, baselineY - baseThicknessPx / 2f),
                    size = Size(remainingWidth, baseThicknessPx),
                    cornerRadius = CornerRadius(baseThicknessPx / 2f, baseThicknessPx / 2f)
                )
            }

            // 2. Draw played wave fill on the left: echo first (behind), then the main wave.
            if (progressX > 0) {
                drawPath(
                    path = wavePath(phase + PI_F, ECHO_DROP_PX),
                    color = waveColor.copy(alpha = 0.22f),
                    style = Fill
                )
                drawPath(
                    path = wavePath(phase, 0f),
                    color = waveColor,
                    style = Fill
                )
            }

            // 3. Draw thumb at progressX: it breathes with the beat and glows while playing.
            val pulse = 1f + (if (isPlaying) 0.14f * sin(phase * 2f) else 0f)
            if (isPlaying) {
                drawCircle(
                    color = waveColor.copy(alpha = 0.22f),
                    radius = thumbRadiusPx * 2.2f,
                    center = Offset(progressX, baselineY)
                )
            }
            drawCircle(
                color = thumbColor,
                radius = thumbRadiusPx * pulse,
                center = Offset(progressX, baselineY)
            )
        }
        // Mochi surfs the thumb: dancing while it moves, resting when paused. He draws on top but
        // takes no gestures, so taps and drags fall through to the canvas below.
        if (showMochi) {
            com.agupta07505.smartisland.coucou.ui.MochiGlyph(
                size = RIDER_SIZE,
                mood = if (isPlaying) {
                    com.agupta07505.smartisland.coucou.ui.MochiMood.Dancing
                } else {
                    com.agupta07505.smartisland.coucou.ui.MochiMood.Idle
                },
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .offset(riderX, RIDER_Y)
            )
        }
    }
}

private val WAVE_FADE_WIDTH = 24.dp
private const val WAVE_FREQ_1 = 0.04f
private const val WAVE_FREQ_2 = 0.02f
// Mochi on the thumb: his size, how high he floats above the bar, how far the echo wave sinks,
// and half a turn (the echo runs in opposite phase).
private val RIDER_SIZE = 18.dp
private val RIDER_Y = (-12).dp
private const val ECHO_DROP_PX = 3f
private const val PI_F = 3.1415927f

