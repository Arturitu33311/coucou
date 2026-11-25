/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * Look and timings studied from "Glance" by Jonny Oo (https://github.com/jonnyoo/glance, MIT License); the Compose
 * code is original. See GlanceUnlock.kt and NOTICE-glance.md.
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.ui.Alignment
import androidx.compose.ui.unit.IntOffset
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

private val GlanceGreen = Color(0xFF30D158)
private val GlanceRed = Color(0xFFFF453A)

/** The switch and the style, watched live (the settings card changes them while the island is up). */
@Composable
fun rememberGlancePrefs(): Pair<GlanceMode, GlanceStyle> {
    val app = LocalContext.current.applicationContext
    var prefs by remember { mutableStateOf(GlanceSettings.mode(app) to GlanceSettings.style(app)) }
    DisposableEffect(app) {
        val off = GlanceSettings.listen(app) { prefs = GlanceSettings.mode(app) to GlanceSettings.style(app) }
        onDispose(off)
    }
    return prefs
}

/**
 * The unlock pill: the island's own pill widens into a [baseWidth]+64dp capsule with a lock on one flank and a ring on
 * the other (the middle stays empty so a punch-hole camera is never drawn over), the ring closes, a tick draws itself,
 * then it folds back. Everything is a function of one clock ([GlanceUnlockLogic]); nothing loops except the debug SCAN.
 * Say "unlocked", never "recognised": the phone does not tell us how it was opened.
 */
@Composable
fun GlanceUnlockPill(
    kind: GlanceKind,
    style: GlanceStyle,
    baseWidth: Dp,
    height: Dp,
    shape: Shape,
    color: Color,
    onDone: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val reduced = rememberReducedMotion()
    val start = if (reduced) GlanceUnlockLogic.REDUCED_START_MS else 0L
    var t by remember { mutableLongStateOf(start) }
    val done by rememberUpdatedState(onDone)
    LaunchedEffect(kind, reduced) {
        val total = GlanceUnlockLogic.totalMs(kind, reduced) + start
        val t0 = androidx.compose.runtime.withFrameMillis { it }
        var ticked = false
        while (t < total) {
            androidx.compose.runtime.withFrameMillis { now -> t = (now - t0) + start }
            if (!ticked && kind != GlanceKind.SCAN && t >= 650) {
                ticked = true
                Haptics.tap(context)
            }
        }
        done()
    }
    val mochiMood by remember(kind) { androidx.compose.runtime.derivedStateOf { GlanceUnlockLogic.mochiMood(kind, t) } }
    val density = LocalContext.current.resources.displayMetrics.density
    val openWidthPx = ((baseWidth + 64.dp).value * density).toInt()
    val basePx = (baseWidth.value * density).toInt()
    val heightPx = (height.value * density).toInt()
    Box(
        modifier
            .layout { measurable, _ ->
                val f = GlanceUnlockLogic.pillOpen(kind, t)
                val w = (basePx + (openWidthPx - basePx) * f).toInt().coerceAtLeast(heightPx)
                val p = measurable.measure(Constraints.fixed(w, heightPx))
                layout(w, heightPx) { p.place(0, 0) }
            }
            .graphicsLayer { alpha = GlanceUnlockLogic.pillAlpha(kind, t, reduced) }
            .clip(shape)
            .background(color),
    ) {
        if (style == GlanceStyle.MOCHI) {
            // Jinx's Mochi takes the lock's flank: he watches the ring while it searches, is glad when it closes.
            val reachPx = openWidthPx / 2f - heightPx * 0.30f - heightPx * 0.50f / 2f
            val mochiDp = height * 0.68f
            Box(
                Modifier
                    .align(Alignment.Center)
                    .offset { IntOffset((-reachPx + GlanceUnlockLogic.shakeDp(kind, t) * density).toInt(), 0) }
                    .graphicsLayer {
                        val f = GlanceUnlockLogic.pillOpen(kind, t)
                        val end = GlanceUnlockLogic.holdEndMs(kind)
                        val fade = if (t > end) 1f - ((t - end) / 180f).coerceIn(0f, 1f) else 1f
                        alpha = ((f - 0.35f) / 0.5f).coerceIn(0f, 1f) * fade
                    },
            ) {
                MochiGlyph(size = mochiDp, tint = MochiEmerald, mood = mochiMood, lookX = 1f)
            }
        }
        Canvas(Modifier.fillMaxSize()) {
            val kindNow = kind
            val tNow = t
            val h = size.height
            val glyph = h * 0.50f
            val edge = h * 0.30f
            // Flanks sit at their final place from the pill's centre; the clip reveals them as it widens.
            val reach = openWidthPx / 2f - edge - glyph / 2f
            val cy = h / 2f
            val cx = size.width / 2f
            val f = GlanceUnlockLogic.pillOpen(kindNow, tNow)
            val appear = ((f - 0.35f) / 0.5f).coerceIn(0f, 1f)
            val fade = if (tNow > GlanceUnlockLogic.holdEndMs(kindNow)) {
                1f - ((tNow - GlanceUnlockLogic.holdEndMs(kindNow)) / 180f).coerceIn(0f, 1f)
            } else 1f
            val a = appear * fade
            val shake = GlanceUnlockLogic.shakeDp(kindNow, tNow) * density
            translate(left = shake) {
                if (style == GlanceStyle.RING) {
                    drawLock(Offset(cx - reach, cy), glyph, GlanceUnlockLogic.lockOpen(kindNow, tNow), a)
                }
                drawRingAndMark(Offset(cx + reach, cy), glyph, kindNow, tNow, a)
            }
        }
    }
}

private fun DrawScope.drawLock(c: Offset, g: Float, open: Float, alpha: Float) {
    val ink = Color.White.copy(alpha = alpha)
    val bodyW = g * 0.80f
    val bodyH = g * 0.58f
    val bodyTop = c.y - g * 0.04f
    val left = c.x - bodyW / 2f
    drawRoundRect(ink, Offset(left, bodyTop), androidx.compose.ui.geometry.Size(bodyW, bodyH),
        androidx.compose.ui.geometry.CornerRadius(g * 0.14f))
    val sw = g * 0.12f
    val archR = bodyW * 0.30f
    val archCy = bodyTop - g * 0.02f
    val pivot = Offset(c.x - archR, archCy)
    // Left leg planted; the right leg swings up and out as it opens.
    drawLine(ink, Offset(c.x - archR, bodyTop + g * 0.02f), pivot, strokeWidth = sw, cap = StrokeCap.Round)
    rotate(degrees = -34f * open, pivot = pivot) {
        val arm = Path().apply {
            arcTo(
                androidx.compose.ui.geometry.Rect(c.x - archR, archCy - archR, c.x + archR, archCy + archR),
                180f, 180f, false,
            )
            lineTo(c.x + archR, bodyTop + g * 0.10f)
        }
        drawPath(arm, ink, style = Stroke(width = sw, cap = StrokeCap.Round))
    }
}

private fun DrawScope.drawRingAndMark(c: Offset, g: Float, kind: GlanceKind, t: Long, alpha: Float) {
    val sweep = GlanceUnlockLogic.ringSweep(kind, t)
    val mark = GlanceUnlockLogic.markProgress(kind, t)
    val tint = when (kind) {
        GlanceKind.SUCCESS -> lerp(Color.White, GlanceGreen, mark)
        GlanceKind.FAILURE -> lerp(Color.White, GlanceRed, GlanceUnlockLogic.ease(((t - 150) / 300f).coerceIn(0f, 1f)))
        GlanceKind.SCAN -> Color.White
    }.copy(alpha = alpha)
    val r = g * 0.5f
    val w = g * 0.12f
    val rot = GlanceUnlockLogic.ringRotation(kind, t)
    drawArc(
        color = tint,
        startAngle = -90f + rot,
        sweepAngle = 360f * sweep,
        useCenter = false,
        topLeft = Offset(c.x - r, c.y - r),
        size = androidx.compose.ui.geometry.Size(2 * r, 2 * r),
        style = Stroke(width = w, cap = StrokeCap.Round),
    )
    if (mark <= 0f) return
    val box = g * 0.56f
    val ox = c.x - box / 2f
    val oy = c.y - box / 2f
    fun pt(x: Float, y: Float) = Offset(ox + box * x, oy + box * y)
    val style = Stroke(width = w, cap = StrokeCap.Round, join = StrokeJoin.Round)
    if (kind == GlanceKind.SUCCESS) {
        drawPolyline(listOf(pt(0.16f, 0.52f), pt(0.40f, 0.78f), pt(0.86f, 0.22f)), mark, tint, style)
    } else {
        // The cross: first stroke in the first half of the draw, the second in the rest.
        drawPolyline(listOf(pt(0.22f, 0.22f), pt(0.78f, 0.78f)), (mark * 2f).coerceAtMost(1f), tint, style)
        drawPolyline(listOf(pt(0.78f, 0.22f), pt(0.22f, 0.78f)), ((mark - 0.5f) * 2f).coerceAtLeast(0f), tint, style)
    }
}

/** Draws the first [progress] (0..1) of the polyline's length, so a tick appears to be written, not faded. */
private fun DrawScope.drawPolyline(pts: List<Offset>, progress: Float, color: Color, style: Stroke) {
    if (progress <= 0f) return
    val lens = pts.zipWithNext { a, b -> (b - a).getDistance() }
    var want = lens.sum() * progress
    val path = Path().apply { moveTo(pts[0].x, pts[0].y) }
    for (i in lens.indices) {
        val take = want.coerceAtMost(lens[i])
        val f = if (lens[i] == 0f) 0f else take / lens[i]
        val e = pts[i] + (pts[i + 1] - pts[i]) * f
        path.lineTo(e.x, e.y)
        want -= take
        if (want <= 0f) break
    }
    drawPath(path, color, style = style)
}
