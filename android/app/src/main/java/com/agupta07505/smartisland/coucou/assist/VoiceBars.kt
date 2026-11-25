/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlin.math.sin

/** What Jinx is doing, as « OK Coucou » on the laptop shows it: waiting, hearing Alan, thinking, speaking. */
enum class VoicePhase { Idle, Listening, Thinking, Speaking }

const val VOICE_BANDS = 9

/**
 * Where bar [i] wants to be (0…1) at [nowMs]. The same movements as the laptop's pill (pill/main.ts dotGoal):
 * a slow wave while thinking, a lively one while she speaks, the voice while listening (the phone's recognizer
 * gives one loudness, so the bands share it with a little shimmer), a faint ripple while it waits for you to speak.
 */
fun barGoal(phase: VoicePhase, i: Int, nowMs: Long, level: Float): Float = when (phase) {
    VoicePhase.Thinking -> 0.25f + 0.25f * sin(nowMs / 160f - i * 0.7f)
    VoicePhase.Speaking -> 0.2f + 0.35f * abs(sin(nowMs / 110f + i * 1.3f) * sin(nowMs / 270f - i * 0.6f))
    VoicePhase.Listening ->
        if (level > 0.03f) level * (0.55f + 0.45f * abs(sin(nowMs / 130f + i * 1.1f))) else 0.08f + 0.08f * sin(nowMs / 400f - i * 0.5f)
    VoicePhase.Idle -> 0f
}

/** One step of the easing the laptop uses: each bar goes 14 × per second of the way to its goal. */
fun easeBar(current: Float, goal: Float, dtSeconds: Float): Float = current + (goal - current) * minOf(1f, dtSeconds * 14f)

/** The nine bars of the voice. With [animate] off (system animations off) they only show the state, still. */
@Composable
fun VoiceBars(phase: VoicePhase, level: Float, color: Color, modifier: Modifier = Modifier, animate: Boolean = true) {
    val levels = remember { FloatArray(VOICE_BANDS) }
    var frame by remember { mutableLongStateOf(0L) }
    val phaseNow by rememberUpdatedState(phase)
    val levelNow by rememberUpdatedState(level)
    LaunchedEffect(animate) {
        var last = 0L
        while (true) {
            androidx.compose.runtime.withFrameNanos { ns ->
                val dt = if (last == 0L) 0.016f else ((ns - last) / 1e9f).coerceAtMost(0.05f)
                last = ns
                val ms = ns / 1_000_000L
                for (i in 0 until VOICE_BANDS) {
                    val goal = if (animate) barGoal(phaseNow, i, ms, levelNow) else if (phaseNow == VoicePhase.Idle) 0f else 0.25f
                    levels[i] = easeBar(levels[i], goal, dt)
                }
                frame = ns
            }
            if (!animate && phaseNow == VoicePhase.Idle) kotlinx.coroutines.delay(500)
        }
    }
    Canvas(modifier.width(((VOICE_BANDS * 4) + (VOICE_BANDS - 1) * 3).dp).height(24.dp)) {
        frame.let { }  // reading it is what redraws the bars each frame
        val w = 4.dp.toPx()
        val gap = 3.dp.toPx()
        val minH = 4.dp.toPx()
        val maxH = size.height
        for (i in 0 until VOICE_BANDS) {
            val h = (minH + (maxH - minH) * levels[i].coerceIn(0f, 1f))
            drawRoundRect(color, Offset(i * (w + gap), (size.height - h) / 2f), Size(w, h), CornerRadius(w / 2f))
        }
    }
}
