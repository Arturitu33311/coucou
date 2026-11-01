/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 * Mochi is Coucou's character, drawn in code as on the Mac, the PC and the iPhone.
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin

/** What Mochi feels. Alert/Happy/Listening/Sleepy are his desk states; Surprised, Glance and
 * Dancing are his reactions to the island (news, then looking at it, music); Tired is low
 * battery and Celebrating is an emptied task list. The moods with an [emote] are Jinx's (what her
 * reply carries, see JinxMood): the same faces as the PC's emotes of that name (engine.ts EMOTE_EYE). */
enum class MochiMood(val emote: String? = null) {
    Idle, Alert, Happy, Listening, Sleepy, Surprised, Glance, Dancing, Tired, Celebrating, Working, Searching, Thinking, Error,
    Love("love"), Proud("proud"), Excited("excited"), Confused("confused"), Shy("shy"), Sad("sad"),
    Curious("curious"), Bored("bored"), Scared("scared"), Grumpy("grumpy"), Smug("smug"), Laughing("laughing"),
    Crying("crying"), Relieved("relieved"), Determined("determined"), Flustered("flustered"),
}

/** Blush on his cheeks (shy, in love, laughing, flustered). */
private val Blush = Color(0xFFFF7A9A)

/** Tears and the water in teary eyes (engine.ts "teary"). */
private val TearBlue = Color(0xFF7CC7FF)

/** The blue of Claude Code's working dots on the laptop's Mochi (engine.ts C.working). */
private val WorkingBlue = Color(0xFF4F9BFF)

/** …and of its searching dots (engine.ts C.searching). */
private val SearchingIndigo = Color(0xFF6365F2)

/** …and while it thinks (engine.ts C.thinking). */
private val ThinkingViolet = Color(0xFF8B5CF6)

/** Mochi's emerald: what he wears whenever no other colour means something (overdue is amber,
 * Claude is orange, questions are blue — those stay). */
val MochiEmerald = Color(0xFF10B981)

/** Confetti for the celebration: coral, white, amber, sky, green and lilac, cycling. */
private val CONFETTI = listOf(
    Color(0xFFE08A6C),
    Color.White,
    Color(0xFFF5A524),
    Color(0xFF7DD3FC),
    Color(0xFF4ADE80),
    Color(0xFFC4B5FD),
)

/**
 * Mochi: a soft squircle with two eyes. He blinks now and then and breathes; [MochiMood.Alert]
 * opens his eyes wide and makes him bob, [MochiMood.Happy] turns them into smiles,
 * [MochiMood.Listening] makes them lean in, [MochiMood.Sleepy] closes them and floats a "z",
 * [MochiMood.Surprised] stretches him up with wide eyes and a "!", [MochiMood.Dancing]
 * sways him side to side with happy eyes and music notes, and [MochiMood.Tired] droops his lids
 * with a bead of sweat for low battery. [MochiMood.Happy] also bounces gently (unlock greeting).
 * [MochiMood.Glance] slides his pupils toward the main pill ([lookX]); [MochiMood.Celebrating]
 * jumps with confetti when the task list empties. Left alone in [MochiMood.Idle] he peeks sideways
 * about every 12 s, never often.
 */
@Composable
fun MochiGlyph(
    size: Dp = 24.dp,
    tint: Color = MochiEmerald,
    mood: MochiMood = MochiMood.Idle,
    modifier: Modifier = Modifier,
    // Where he looks when glancing (bubble passes toward-main: +1 main is right, -1 left).
    lookX: Float = 0f,
    // What he wears (see MochiWardrobe.kt); Outfit.None draws exactly what he always drew.
    outfit: Outfit = Outfit.None,
    // A little thing he does on his own while calm (see MochiIdleScript); only drawn in Idle.
    gesture: IdleGesture = IdleGesture.None,
    // 0..1 along the gesture.
    gestureProgress: Float = 0f,
    // Plugged in and charging: contented eyes and a pulsing bolt (Idle only).
    charging: Boolean = false,
    // The emoji floating in his thought bubble while he does [IdleGesture.Think].
    thought: String = "💭",
) {
    // The outfit goes on with a little overshoot, each time it changes.
    val enter = remember(outfit) { Animatable(0f) }
    LaunchedEffect(outfit) { if (outfit != Outfit.None) enter.animateTo(1f, tween(350)) }
    val simplifiedOutfit = size < 40.dp
    val rawFit = if (outfit == Outfit.None) 1f else Wardrobe.fit(outfit)
    // At island size the pill and the bubble clip whatever falls outside the box he is drawn in, so
    // everything that floats above his head (a thought bubble, "!", notes, "z") has to fit inside it
    // too. How much room that needs, above the top of his body, in box heights:
    val roomNeeded = if (!simplifiedOutfit) 0f else when {
        mood == MochiMood.Idle && gesture == IdleGesture.Think -> 0.34f
        mood == MochiMood.Surprised -> 0.21f
        mood == MochiMood.Dancing -> 0.22f
        mood == MochiMood.Sleepy -> 0.22f
        mood == MochiMood.Love || mood == MochiMood.Confused || mood == MochiMood.Excited ||
            mood == MochiMood.Proud || mood == MochiMood.Grumpy -> 0.22f
        else -> 0f
    }.let { GlyphFit.need(it) }
    // Shrink the whole picture just enough to fit his body plus that room (never more than his outfit
    // already needs), and slide it down so its top touches the box (see GlyphFit).
    val targetFit = GlyphFit.target(rawFit, roomNeeded)
    val fit by animateFloatAsState(targetFit, tween(220), label = "fit")
    val drop by animateFloatAsState(GlyphFit.drop(targetFit, roomNeeded), tween(220), label = "drop")
    val small = simplifiedOutfit
    val transition = rememberInfiniteTransition(label = "mochi")
    // Eyes open for ~3.6 s, shut for a blink of 0.15 s.
    val blink by transition.animateFloat(
        initialValue = 1f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = keyframes {
                durationMillis = 3800
                1f at 0
                1f at 3600
                0.08f at 3700 using LinearEasing
                1f at 3800
            },
            repeatMode = RepeatMode.Restart,
        ),
        label = "blink",
    )
    // Slow bounce/breathing (1.1 s): shared by the alert bob, the happy bounce, the surprise
    // tremble and the calm breathing — one tempo for the whole body keeps him coherent.
    val bob by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = keyframes {
                durationMillis = 1100
                0f at 0
                1f at 550
                0f at 1100
            },
            repeatMode = RepeatMode.Restart,
        ),
        label = "bob",
    )
    // The dance beat: one sway every 1.4 s. It runs on its own phase (not on `bob`, which is
    // the alert bounce) so dancing and bobbing never interfere with each other. Kept slow on
    // purpose: at phone-island size anything faster reads as nervous.
    val beat by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis = 1400, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "beat",
    )
    // A fresh surprise pops in once and settles back: a soft one-shot spring (not a loop), so
    // every new arrival replays the pop instead of throbbing forever.
    val pop by animateFloatAsState(
        targetValue = if (mood == MochiMood.Surprised) 1f else 0f,
        animationSpec = spring(dampingRatio = 0.45f, stiffness = 220f),
        label = "pop",
    )
    val dancing = mood == MochiMood.Dancing
    val surprised = mood == MochiMood.Surprised
    val tired = mood == MochiMood.Tired
    val celebrating = mood == MochiMood.Celebrating
    val glancing = mood == MochiMood.Glance
    val g = if (mood == MochiMood.Idle) gesture else IdleGesture.None
    // Bell-shaped 0..1..0 over the gesture: every move eases in and out with it.
    val env = if (g == IdleGesture.None) 0f else sin(PI.toFloat() * gestureProgress.coerceIn(0f, 1f))
    val gp = gestureProgress.coerceIn(0f, 1f)
    val gestureTilt = when (g) {
        IdleGesture.Scratch -> sin(gp * 6f * PI.toFloat()) * 6f * env
        IdleGesture.Whistle -> sin(gp * 2f * PI.toFloat()) * 4f
        IdleGesture.Tickle -> sin(gp * 10f * PI.toFloat()) * 10f * env
        IdleGesture.Hop -> sin(gp * 2f * PI.toFloat()) * 3f
        else -> 0f
    }
    // Text decorations ("!", notes, "z") go through Compose's own text measurer: no platform-canvas
    // interop, so they work on every canvas that draws him (island overlay included).
    val textMeasurer = rememberTextMeasurer()
    // Idle curiosity: about every 12 s he peeks once each way (never often — the island must
    // stay calm). Mood-level reactions (glance, surprise) override this entirely.
    val peek by transition.animateFloat(
        initialValue = 0f,
        targetValue = 0f,
        animationSpec = infiniteRepeatable(
            animation = keyframes {
                durationMillis = 12_000
                0f at 0
                0f at 8000
                1f at 9000 using LinearEasing
                0f at 10_100 using LinearEasing
                -1f at 11_100 using LinearEasing
                0f at 12_000 using LinearEasing
            },
            repeatMode = RepeatMode.Restart,
        ),
        label = "peek",
    )
    val wave = (beat * 2 * PI).toFloat()
    // Whole-body language, done as GPU transforms (no redraw): tilt/sway/hop for dancing, a
    // tremble for surprise, a weary lean for low battery, and quiet breathing only while calm
    // (dance, surprise and tiredness already move him).
    // NOTE: the pop scale applies only while surprised — every other mood draws at full size.
    val tilt = when {
        dancing -> sin(wave) * 13f
        celebrating -> sin(wave) * 8f
        tired -> 7f
        glancing -> lookX * 6f
        // Jinx's moods, as the PC's emotes move him (engine.ts triggerEmote), held while the mood lasts.
        mood == MochiMood.Confused -> sin(wave) * 10f
        mood == MochiMood.Curious -> 9f
        mood == MochiMood.Proud || mood == MochiMood.Smug -> -7f
        mood == MochiMood.Shy -> -6f
        mood == MochiMood.Love -> sin(wave) * 5f
        mood == MochiMood.Laughing -> -5f + sin(wave * 2f) * 3f
        mood == MochiMood.Sad || mood == MochiMood.Crying -> 3f
        else -> gestureTilt
    }
    val shiftX = when {
        dancing -> sin(wave) * 0.07f
        surprised -> (bob - 0.5f) * 0.05f
        mood == MochiMood.Scared -> sin(wave * 24f) * 0.015f
        mood == MochiMood.Flustered -> sin(wave * 10f) * 0.025f
        else -> 0f
    }
    val hop = when {
        mood == MochiMood.Excited -> -abs(sin(wave * 2f)) * 0.08f
        mood == MochiMood.Laughing -> -abs(sin(wave * 5f)) * 0.035f
        mood == MochiMood.Proud -> -0.03f
        dancing -> -abs(sin(wave)) * 0.09f
        celebrating -> -abs(sin(wave)) * 0.13f
        g == IdleGesture.Hop -> -abs(sin(gp * 2f * PI.toFloat())) * 0.16f
        g == IdleGesture.Tickle -> -abs(sin(gp * 5f * PI.toFloat())) * 0.05f * env
        else -> 0f
    }
    val breathe = if (!dancing && !surprised && !tired) 1f + (bob - 0.5f) * 0.03f else 1f
    // Squash and stretch of Jinx's moods: puffed up when grumpy, a deep sigh when bored or relieved,
    // a sob when crying, squished when sad, planted when determined.
    val moodSx = when (mood) {
        MochiMood.Grumpy -> 1.06f
        MochiMood.Bored, MochiMood.Relieved -> 1f + (bob - 0.5f) * 0.05f
        MochiMood.Shy -> 0.96f
        else -> 1f
    }
    val moodSy = when (mood) {
        MochiMood.Grumpy -> 0.95f
        MochiMood.Sad -> 0.94f
        MochiMood.Crying -> 0.96f - abs(sin(wave * 3f)) * 0.035f
        MochiMood.Bored, MochiMood.Relieved -> 0.97f - (bob - 0.5f) * 0.05f
        MochiMood.Determined -> 1.03f
        MochiMood.Curious -> 1.03f
        else -> 1f
    }
    val scale = if (surprised) 0.6f + pop * 0.4f else 1f
    // Stretch reaches up and narrows; a yawn swells him a little.
    val gestureStretchY = when (g) {
        IdleGesture.Stretch -> 1f + 0.14f * env
        IdleGesture.Yawn -> 1f + 0.05f * env
        else -> 1f
    }
    val gestureStretchX = if (g == IdleGesture.Stretch) 1f - 0.05f * env else 1f
    Canvas(
        modifier
            .size(size)
            .graphicsLayer {
                rotationZ = tilt
                translationX = shiftX * size.toPx()
                translationY = (hop + drop) * size.toPx()
                // (`fit` shrinks him with a big outfit on, so a witch's brim stays in his box.)
                scaleX = scale * breathe * fit * gestureStretchX * moodSx
                scaleY = scale * (if (dancing) 2f - breathe else 1f) * fit * gestureStretchY * moodSy
            },
    ) {
        val w = this.size.width
        val h = this.size.height
        val stretch = if (surprised) 1.06f else 1f
        val bodyH = h * 0.78f * stretch
        // Each mood lifts differently: Alert bobs hard, Happy bounces gently (task done, hello on
        // unlock), Surprised jumps once with the pop, Dancing hops on the beat. Tired slumps: no lift.
        val lift = when {
            mood == MochiMood.Alert -> -bob * h * 0.07f
            mood == MochiMood.Happy -> -bob * h * 0.035f
            surprised -> -pop * h * 0.08f
            dancing -> -abs(sin(wave)) * h * 0.05f
            mood == MochiMood.Sad || mood == MochiMood.Crying -> h * 0.035f
            else -> 0f
        }
        val top = (h - bodyH) / 2f + lift
        // The outfit is drawn on the head's ball (width = 2.28 R): its centre, the unit, and where he looks.
        val head = Head(
            R = w / 2.28f,
            yaw = when {
                glancing -> lookX * 0.45f
                mood == MochiMood.Idle -> peek * 0.35f + when (g) {
                    IdleGesture.LookAround -> sin(gp * 2f * PI.toFloat()) * 0.4f
                    else -> 0f
                }
                dancing -> sin(wave) * 0.25f
                else -> 0f
            },
            physDx = if (dancing) sin(wave) * 0.4f else if (celebrating) sin(wave) * 0.3f else 0f,
            physDy = if (mood == MochiMood.Alert || mood == MochiMood.Happy) -bob * 0.15f else 0f,
            simplified = simplifiedOutfit,
        )
        val headCy = top + bodyH / 2f
        val beatTime = beat * 1.4f
        drawOutfitBehind(outfit, head, w / 2f, headCy, enter.value, beatTime)
        if (outfit.recolorsBody()) {
            drawRoundRect(
                brush = androidx.compose.ui.graphics.Brush.verticalGradient(listOf(PumpkinTop, PumpkinBottom), startY = top, endY = top + bodyH),
                topLeft = Offset(0f, top),
                size = Size(w, bodyH),
                cornerRadius = CornerRadius(bodyH * 0.42f, bodyH * 0.42f),
            )
        } else {
            drawRoundRect(
                color = tint,
                topLeft = Offset(0f, top),
                size = Size(w, bodyH),
                cornerRadius = CornerRadius(bodyH * 0.42f, bodyH * 0.42f),
            )
        }
        val ink = Color(0xFF1B1B20)
        val eyeY = top + bodyH * 0.46f - (if (g == IdleGesture.LookUp) bodyH * 0.07f * env else 0f)
        val lean = if (mood == MochiMood.Listening) w * 0.04f else 0f
        // Looking aside: on purpose toward the main pill when glancing, idly both ways in Idle.
        val glanceShift = if (glancing) lookX * w * 0.10f else 0f
        val idleShift = if (mood == MochiMood.Idle) {
            peek * w * 0.07f + if (g == IdleGesture.LookAround) sin(gp * 2f * PI.toFloat()) * w * 0.08f else 0f
        } else 0f
        val contented = (charging && mood == MochiMood.Idle && g == IdleGesture.None) || g == IdleGesture.Tickle
        val happyEyes = mood == MochiMood.Happy || dancing || celebrating || contented
        val eyesShut = mood == MochiMood.Sleepy || (g == IdleGesture.Yawn && env > 0.25f) ||
            (g == IdleGesture.Stretch && env > 0.25f)
        val xs = listOf(w * 0.32f + lean + glanceShift + idleShift, w * 0.68f + lean + glanceShift + idleShift)
        for ((eyeIdx, cx) in xs.withIndex()) {
            when {
                eyesShut -> {
                    // closed eyes: a short line each
                    val r = w * 0.07f
                    drawLine(ink, Offset(cx - r, eyeY + r * 0.2f), Offset(cx + r, eyeY + r * 0.2f), strokeWidth = w * 0.05f, cap = StrokeCap.Round)
                }
                happyEyes || (g == IdleGesture.Wink && eyeIdx == 1 && env > 0.3f) -> {
                    // ^ ^ : a small arc for each eye (a wink is just the right one)
                    val r = w * 0.085f
                    val p = Path().apply {
                        moveTo(cx - r, eyeY + r * 0.6f)
                        quadraticTo(cx, eyeY - r * 1.4f, cx + r, eyeY + r * 0.6f)
                    }
                    drawPath(p, ink, style = Stroke(width = w * 0.055f, cap = StrokeCap.Round))
                }
                mood.emote != null -> emoteEye(mood, cx, eyeY, w, if (eyeIdx == 0) -1f else 1f, blink, wave, ink)
                surprised -> {
                    // Wide open and frozen: a surprise never blinks, or the moment is lost.
                    val rx = w * 0.08f
                    val ry = rx * 2.05f
                    drawOval(ink, topLeft = Offset(cx - rx, eyeY - ry), size = Size(rx * 2, ry * 2))
                }
                tired -> {
                    // Heavy lids at low battery: half-mast ovals that still blink (slowly, with `blink`).
                    val rx = w * 0.062f
                    val ry = (rx * 0.55f).coerceAtLeast(0.5f) * (0.35f + 0.65f * blink)
                    drawOval(ink, topLeft = Offset(cx - rx, eyeY - ry), size = Size(rx * 2, ry * 2))
                }
                mood == MochiMood.Error -> {
                    // Flat eyes: a tool failed (the laptop's Mochi does the same).
                    val r = w * 0.075f
                    drawLine(ink, Offset(cx - r, eyeY), Offset(cx + r, eyeY), strokeWidth = w * 0.05f, cap = StrokeCap.Round)
                }
                glancing -> {
                    // Eyes slid toward the news, wide and blinking: he is watching the main pill.
                    val rx = w * 0.062f
                    val ry = rx * 1.6f * blink
                    drawOval(ink, topLeft = Offset(cx - rx, eyeY - ry), size = Size(rx * 2, ry * 2))
                }
                else -> {
                    val rx = w * (if (mood == MochiMood.Alert) 0.075f else 0.058f)
                    val ry = rx * 1.55f * blink
                    drawOval(ink, topLeft = Offset(cx - rx, eyeY - ry), size = Size(rx * 2, ry * 2))
                }
            }
        }
        // In front of his eyes (glasses sit on them), under the badges and decorations.
        drawOutfitFront(outfit, head, w / 2f, headCy, enter.value, beatTime, eyeDy = -0.1f * bodyH)
        if (g == IdleGesture.Yawn && env > 0.1f) {
            // The mouth opens wide as he yawns.
            val mw = w * 0.07f * env
            val mh = w * 0.10f * env
            drawOval(Color(0xFF1B1B20), topLeft = Offset(w / 2f - mw, eyeY + bodyH * 0.24f), size = Size(mw * 2, mh * 2))
        }
        if (g == IdleGesture.Whistle) {
            val r = w * 0.035f
            drawCircle(Color(0xFF1B1B20), radius = r, center = Offset(w / 2f, eyeY + bodyH * 0.27f))
            if (small) {
                glyph(textMeasurer, "♪", w * 0.80f, top - h * (0.0f + gp * 0.10f), w * 0.24f, Color.White, (1f - gp).coerceIn(0f, 1f))
            } else {
                glyph(textMeasurer, "♪", w * 0.80f, top - h * (0.02f + gp * 0.22f), w * 0.30f, Color.White, (1f - gp).coerceIn(0f, 1f))
            }
        }
        if (g == IdleGesture.Think) {
            // A thought bubble: two little puffs and a cloud with something on his mind.
            val a = (env * 3f).coerceIn(0f, 1f)
            val cloud = Color.White.copy(alpha = 0.92f * a)
            if (small) {
                // Island size: the cloud sits in the upper right of the box, inside what the pill shows.
                drawCircle(cloud, radius = w * 0.03f, center = Offset(w * 0.58f, top + bodyH * 0.04f))
                drawCircle(cloud, radius = w * 0.045f, center = Offset(w * 0.66f, top - h * 0.04f))
                drawCircle(cloud, radius = w * 0.20f, center = Offset(w * 0.74f, top - h * 0.14f))
                glyph(textMeasurer, thought, w * 0.74f, top - h * 0.14f, w * 0.26f, Color.White, a)
            } else {
                drawCircle(cloud, radius = w * 0.035f, center = Offset(w * 0.70f, top + bodyH * 0.02f))
                drawCircle(cloud, radius = w * 0.055f, center = Offset(w * 0.80f, top - h * 0.07f))
                drawCircle(cloud, radius = w * 0.24f, center = Offset(w * 0.78f, top - h * 0.24f))
                glyph(textMeasurer, thought, w * 0.78f, top - h * 0.24f, w * 0.30f, Color.White, a)
            }
        }
        if (charging && mood == MochiMood.Idle) {
            // Plugged in: a bolt that breathes with him.
            glyph(textMeasurer, "⚡", w * 0.88f, top + h * (if (small) 0.03f else -0.02f), w * (if (small) 0.24f else 0.28f), Color(0xFFFACC15), 0.6f + 0.4f * bob)
        }
        if (mood == MochiMood.Error) {
            // A red dot over his head, like the laptop's error badge.
            drawCircle(Color(0xFFEF4444), radius = w * 0.06f, center = Offset(w * 0.5f, top - h * 0.02f))
        }
        if (mood == MochiMood.Working || mood == MochiMood.Searching || mood == MochiMood.Thinking) {
            // Three dots over his head lighting up in turn: Claude Code is working (same badge as the laptop's
            // Mochi; indigo while it searches).
            val phase = (beat * 3f).toInt() % 3
            val dots = when (mood) {
                MochiMood.Searching -> SearchingIndigo
                MochiMood.Thinking -> ThinkingViolet
                else -> WorkingBlue
            }
            for (i in 0 until 3) {
                drawCircle(
                    color = dots.copy(alpha = if (i == phase) 1f else 0.35f),
                    radius = w * 0.045f,
                    center = Offset(w * (0.30f + i * 0.20f), top - h * 0.02f),
                )
            }
        }
        if (surprised && pop > 0.4f) {
            // "!" over his head, fading in with the pop.
            if (small) {
                glyph(textMeasurer, "!", w * 0.5f, top - h * 0.04f, w * 0.28f, Color.White, pop.coerceIn(0f, 1f))
            } else {
                glyph(textMeasurer, "!", w * 0.5f, top - h * 0.10f, w * 0.34f, Color.White, pop.coerceIn(0f, 1f))
            }
        }
        if (dancing) {
            // Two notes taking turns in the air.
            val up = (sin(wave) + 1f) / 2f
            if (small) {
                glyph(textMeasurer, "♪", w * 0.10f, top - h * (0.0f + up * 0.10f), w * 0.24f, Color.White, 0.35f + up * 0.65f)
                glyph(textMeasurer, "♫", w * 0.90f, top - h * (0.0f + (1f - up) * 0.10f), w * 0.22f, Color.White, 0.35f + (1f - up) * 0.65f)
            } else {
                glyph(textMeasurer, "♪", w * 0.06f, top - h * (0.02f + up * 0.16f), w * 0.30f, Color.White, 0.35f + up * 0.65f)
                glyph(textMeasurer, "♫", w * 0.94f, top - h * (0.02f + (1f - up) * 0.16f), w * 0.26f, Color.White, 0.35f + (1f - up) * 0.65f)
            }
        }
        if (mood == MochiMood.Sleepy) {
            // A "z" drifts up as his eyes close.
            val shut = 1f - blink
            if (small) {
                glyph(textMeasurer, "z", w * 0.86f, top - h * (0.0f + shut * 0.08f), w * (0.20f + shut * 0.08f), Color.White, 0.30f + shut * 0.60f)
            } else {
                glyph(textMeasurer, "z", w * 0.88f, top - h * (0.06f + shut * 0.14f), w * (0.22f + shut * 0.10f), Color.White, 0.30f + shut * 0.60f)
            }
        }
        if (tired) {
            // One bead of sweat: low battery is exhausting. It slides down and back with `bob`.
            val dropX = w * 0.94f
            val dropY = top + bodyH * (0.18f + bob * 0.10f)
            val r = w * 0.045f
            val sweat = Color(0xFF7DD3FC)
            drawCircle(sweat, radius = r, center = Offset(dropX, dropY))
            val tip = Path().apply {
                moveTo(dropX - r * 0.8f, dropY - r * 0.2f)
                quadraticTo(dropX, dropY - r * 1.8f, dropX + r * 0.8f, dropY - r * 0.2f)
                close()
            }
            drawPath(tip, sweat)
        }
        if (mood.emote != null) emoteDecorations(mood, textMeasurer, w, h, top, bodyH, eyeY, xs, wave, bob, small)
        if (celebrating) {
            // Confetti for an emptied list: six dots orbiting him, each halfway around from the
            // next, rising and falling with the beat. Fixed palette, no allocation per frame.
            val cx0 = w / 2f
            val cy0 = top + bodyH * 0.4f
            for (i in 0 until 6) {
                val a = wave + i * (2 * PI / 6).toFloat()
                // (At island size the confetti orbits inside the box instead of past its sides.)
                val orbit = w * ((if (small) 0.40f else 0.52f) + (if (small) 0.06f else 0.10f) * sin(wave * 2f + i))
                drawCircle(
                    color = CONFETTI[i % CONFETTI.size],
                    radius = w * 0.032f,
                    center = Offset(cx0 + cos(a) * orbit, cy0 + sin(a) * orbit * 0.7f),
                )
            }
        }
    }
}

/** A single character over the glyph ("!", a note, a "z"), centered on (cx, y): drawn with
 * Compose's own text pipeline, so decorations need no platform canvas and cost nothing when
 * their mood is off. */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.glyph(
    measurer: androidx.compose.ui.text.TextMeasurer,
    char: String,
    cx: Float,
    y: Float,
    textSize: Float,
    color: Color,
    alpha: Float,
) {
    val layout = measurer.measure(
        AnnotatedString(char),
        style = TextStyle(
            color = color.copy(alpha = alpha.coerceIn(0f, 1f)),
            fontSize = textSize.toSp(),
            fontWeight = FontWeight.Bold,
        ),
    )
    drawText(
        layout,
        topLeft = Offset(cx - layout.size.width / 2f, y - layout.size.height / 2f),
    )
}

/** An eye, [rx] by [ry] around ([cx], [cy]), cut by a straight lid: only what is below the line from its
 * outer edge ([outer] of the way down) to its inner edge ([inner]) shows. [side] is -1 for his left eye. */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.liddedEye(
    cx: Float, cy: Float, rx: Float, ry: Float, side: Float, inner: Float, outer: Float, ink: Color,
) {
    val topY = cy - ry
    val yOuter = topY + 2f * ry * outer
    val yInner = topY + 2f * ry * inner
    // His left eye's outer edge is on the left; the right eye's on the right.
    val (yl, yr) = if (side < 0f) yOuter to yInner else yInner to yOuter
    val cut = Path().apply {
        moveTo(cx - rx * 1.5f, yl - (yr - yl) * 0.25f)
        lineTo(cx + rx * 1.5f, yr + (yr - yl) * 0.25f)
        lineTo(cx + rx * 1.5f, cy + ry * 2f)
        lineTo(cx - rx * 1.5f, cy + ry * 2f)
        close()
    }
    clipPath(cut) { drawOval(ink, topLeft = Offset(cx - rx, cy - ry), size = Size(rx * 2, ry * 2)) }
}

/** The PC's emote eyes (engine.ts, the Mac's newer fifteen), one eye at a time. [wave] runs 0..2π on the beat. */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.emoteEye(
    mood: MochiMood, cx: Float, cy: Float, w: Float, side: Float, blink: Float, wave: Float, ink: Color,
) {
    val rx = w * 0.062f
    val ry = rx * 1.55f
    val stroke = w * 0.05f
    when (mood) {
        MochiMood.Love -> {
            // Heart eyes, in rose.
            val r = w * 0.085f
            val heart = Path().apply {
                moveTo(cx, cy + r * 0.9f)
                cubicTo(cx - r * 1.4f, cy - r * 0.1f, cx - r * 0.7f, cy - r * 1.2f, cx, cy - r * 0.45f)
                cubicTo(cx + r * 0.7f, cy - r * 1.2f, cx + r * 1.4f, cy - r * 0.1f, cx, cy + r * 0.9f)
                close()
            }
            drawPath(heart, Color(0xFFE11D48))
        }
        MochiMood.Proud -> {
            // Star eyes, in amber, turning slowly.
            val r = w * 0.09f
            val star = Path()
            for (i in 0 until 10) {
                val a = wave * 0.25f * side + i * (PI.toFloat() / 5f) - PI.toFloat() / 2f
                val rr = if (i % 2 == 0) r else r * 0.45f
                val x = cx + cos(a) * rr
                val y = cy + sin(a) * rr
                if (i == 0) star.moveTo(x, y) else star.lineTo(x, y)
            }
            star.close()
            drawPath(star, Color(0xFFF7B32B))
        }
        MochiMood.Excited -> {
            // Big glossy eyes with twinkling highlights.
            val ex = rx * 1.15f
            val ey = ry * 1.12f * blink.coerceAtLeast(0.3f)
            drawOval(ink, topLeft = Offset(cx - ex, cy - ey), size = Size(ex * 2, ey * 2))
            if (blink > 0.5f) {
                val tw = 1f + 0.14f * sin(wave * 3f + side)
                drawCircle(Color.White, radius = ex * 0.42f * tw, center = Offset(cx - ex * 0.28f, cy - ey * 0.35f))
                drawCircle(Color.White.copy(alpha = 0.9f), radius = ex * 0.2f * (2f - tw), center = Offset(cx + ex * 0.3f, cy + ey * 0.3f))
            }
        }
        MochiMood.Confused -> {
            // "o -": one round eye, one tilted dash.
            if (side < 0f) {
                drawCircle(ink, radius = rx * 0.95f, center = Offset(cx, cy))
            } else {
                val d = rx * 1.3f
                drawLine(ink, Offset(cx - d, cy - d * 0.22f), Offset(cx + d, cy + d * 0.22f), strokeWidth = stroke * 1.1f, cap = StrokeCap.Round)
            }
        }
        MochiMood.Shy -> {
            // Small eyes, looking down and away.
            val sx = cx - w * 0.03f
            val sy = cy + ry * 0.45f
            val ex = rx * 0.6f
            val ey = rx * 0.6f * blink.coerceAtLeast(0.25f)
            drawOval(ink, topLeft = Offset(sx - ex, sy - ey), size = Size(ex * 2, ey * 2))
        }
        MochiMood.Sad, MochiMood.Crying -> {
            // Outer corners droop; crying eyes well up with a wobbling water line.
            val ey = ry * 0.9f * blink.coerceAtLeast(0.3f)
            liddedEye(cx, cy, rx, ey, side, inner = 0.08f, outer = 0.55f, ink = ink)
            if (mood == MochiMood.Crying) {
                val water = cy + ey * 0.15f + sin(wave * 2f + side) * ey * 0.12f
                clipPath(Path().apply { addOval(androidx.compose.ui.geometry.Rect(cx - rx, cy - ey, cx + rx, cy + ey)) }) {
                    drawRect(TearBlue.copy(alpha = 0.85f), topLeft = Offset(cx - rx, water), size = Size(rx * 2, ey * 2))
                }
            }
            drawCircle(Color.White.copy(alpha = 0.85f), radius = rx * 0.28f, center = Offset(cx + side * rx * 0.3f, cy - ey * 0.05f))
        }
        MochiMood.Grumpy -> liddedEye(cx, cy, rx * 1.05f, ry * 0.9f, side, inner = 0.62f, outer = 0.08f, ink = ink)
        MochiMood.Bored -> {
            // Flat, unimpressed lids at half height.
            liddedEye(cx, cy, rx * 1.05f, ry, side, inner = 0.5f, outer = 0.5f, ink = ink)
            drawLine(ink, Offset(cx - rx * 1.2f, cy), Offset(cx + rx * 1.2f, cy), strokeWidth = stroke * 0.7f, cap = StrokeCap.Round)
        }
        MochiMood.Smug -> {
            // Lazy lids and a side-eye: both eyes glance the same way.
            val sx = cx + rx * 0.3f
            liddedEye(sx, cy, rx, ry, side, inner = 0.4f, outer = 0.58f, ink = ink)
        }
        MochiMood.Determined -> {
            // Focused: lowered inner lid and a firm brow.
            val ey = ry * 0.95f
            liddedEye(cx, cy, rx, ey, side, inner = 0.36f, outer = 0.1f, ink = ink)
            val inX = cx - side * rx * 1.1f
            val outX = cx + side * rx * 1.25f
            drawLine(ink, Offset(inX, cy - ey * 1.05f), Offset(outX, cy - ey * 1.6f), strokeWidth = stroke, cap = StrokeCap.Round)
        }
        MochiMood.Scared -> {
            // Tall trembling eyes with a catch-light.
            val tx = cx + sin(wave * 30f) * rx * 0.1f
            val ey = ry * 1.3f * blink.coerceAtLeast(0.3f)
            val ex = rx * 0.95f
            drawOval(ink, topLeft = Offset(tx - ex, cy - ey), size = Size(ex * 2, ey * 2))
            if (blink > 0.5f) drawCircle(Color.White.copy(alpha = 0.9f), radius = ex * 0.32f, center = Offset(tx - ex * 0.3f, cy - ey * 0.45f))
        }
        MochiMood.Curious -> {
            // One eye bigger and raised: the "hm?" look.
            val k = if (side > 0f) 1.3f else 0.8f
            val ey = ry * k * blink
            val dy = if (side > 0f) -ry * 0.2f else 0f
            drawOval(ink, topLeft = Offset(cx - rx * k, cy + dy - ey), size = Size(rx * k * 2, ey * 2))
        }
        MochiMood.Laughing -> {
            // "> <" squeezed shut.
            val d = rx * 1.05f
            val p = Path().apply {
                moveTo(cx + side * d * 0.75f, cy - d * 0.8f)
                lineTo(cx - side * d * 0.6f, cy)
                lineTo(cx + side * d * 0.75f, cy + d * 0.8f)
            }
            drawPath(p, ink, style = Stroke(width = stroke, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
        }
        MochiMood.Relieved -> {
            // Eyes closed, content: soft downward arcs.
            val r = w * 0.08f
            val p = Path().apply {
                moveTo(cx - r, cy - r * 0.2f)
                quadraticTo(cx, cy + r * 1.0f, cx + r, cy - r * 0.2f)
            }
            drawPath(p, ink, style = Stroke(width = stroke, cap = StrokeCap.Round))
        }
        MochiMood.Flustered -> {
            // "~" squiggle.
            val p = Path()
            val steps = 14
            for (i in 0..steps) {
                val u = i / steps.toFloat()
                val x = cx - rx * 1.4f + u * rx * 2.8f
                val y = cy + sin(u * 2f * PI.toFloat() + wave * 2f * side) * ry * 0.3f
                if (i == 0) p.moveTo(x, y) else p.lineTo(x, y)
            }
            drawPath(p, ink, style = Stroke(width = stroke, cap = StrokeCap.Round))
        }
        else -> drawOval(ink, topLeft = Offset(cx - rx, cy - ry * blink), size = Size(rx * 2, ry * 2 * blink))
    }
}

/** A bead of sweat at ([x], [y]). */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.sweatDrop(x: Float, y: Float, r: Float) {
    val sweat = Color(0xFF7DD3FC)
    drawCircle(sweat, radius = r, center = Offset(x, y))
    val tip = Path().apply {
        moveTo(x - r * 0.8f, y - r * 0.2f)
        quadraticTo(x, y - r * 1.8f, x + r * 0.8f, y - r * 0.2f)
        close()
    }
    drawPath(tip, sweat)
}

/** What floats around him in Jinx's moods: hearts, sparkles, a "?", tears, sweat, a sigh, a vein, blush. */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.emoteDecorations(
    mood: MochiMood,
    measurer: androidx.compose.ui.text.TextMeasurer,
    w: Float, h: Float, top: Float, bodyH: Float, eyeY: Float, eyes: List<Float>,
    wave: Float, bob: Float, small: Boolean,
) {
    val up = (sin(wave) + 1f) / 2f
    val above = if (small) 0.10f else 0.18f
    if (mood == MochiMood.Shy || mood == MochiMood.Love || mood == MochiMood.Laughing || mood == MochiMood.Flustered || mood == MochiMood.Smug) {
        val a = if (mood == MochiMood.Smug) 0.35f else 0.55f
        for (x in eyes) {
            drawOval(Blush.copy(alpha = a), topLeft = Offset(x - w * 0.075f, eyeY + bodyH * 0.13f), size = Size(w * 0.15f, w * 0.07f))
        }
    }
    when (mood) {
        MochiMood.Love -> {
            glyph(measurer, "♥", w * 0.14f, top - h * (0.02f + up * above), w * 0.24f, Color(0xFFFB7185), 0.35f + up * 0.65f)
            glyph(measurer, "♥", w * 0.86f, top - h * (0.02f + (1f - up) * above), w * 0.20f, Color(0xFFFB7185), 0.35f + (1f - up) * 0.65f)
        }
        MochiMood.Proud, MochiMood.Excited -> {
            val c = if (mood == MochiMood.Proud) Color(0xFFF7B32B) else Color.White
            glyph(measurer, "✦", w * 0.12f, top - h * (0.0f + up * above), w * 0.22f, c, 0.3f + up * 0.7f)
            glyph(measurer, "✦", w * 0.88f, top - h * (0.0f + (1f - up) * above), w * 0.18f, c, 0.3f + (1f - up) * 0.7f)
        }
        MochiMood.Confused -> glyph(measurer, "?", w * 0.82f, top - h * (if (small) 0.06f else 0.12f) - up * h * 0.03f, w * (if (small) 0.26f else 0.32f), Color.White, 0.85f)
        MochiMood.Grumpy -> glyph(measurer, "💢", w * 0.84f, top - h * (if (small) 0.04f else 0.08f), w * (if (small) 0.22f else 0.26f), Color.White, 0.6f + 0.4f * bob)
        MochiMood.Sad -> {
            // One tear running down from his left eye.
            val y = eyeY + bodyH * (0.08f + bob * 0.22f)
            drawCircle(TearBlue, radius = w * 0.03f, center = Offset(eyes[0] - w * 0.04f, y))
        }
        MochiMood.Crying -> {
            // Streams of tears from both eyes.
            for ((i, x) in eyes.withIndex()) {
                val side = if (i == 0) -1f else 1f
                for (k in 0 until 2) {
                    val p = ((wave / (2f * PI.toFloat())) * 2f + k * 0.5f + i * 0.25f) % 1f
                    drawCircle(
                        TearBlue.copy(alpha = 1f - p * 0.6f),
                        radius = w * 0.028f,
                        center = Offset(x + side * w * (0.06f + p * 0.08f), eyeY + bodyH * (0.06f + p * 0.32f)),
                    )
                }
            }
        }
        MochiMood.Scared, MochiMood.Flustered -> sweatDrop(w * 0.94f, top + bodyH * (0.18f + bob * 0.10f), w * 0.045f)
        MochiMood.Bored, MochiMood.Relieved -> {
            // A sigh drifting out from the side of his face.
            val p = (wave / (2f * PI.toFloat()))
            drawCircle(Color.White.copy(alpha = 0.55f * (1f - p)), radius = w * (0.04f + p * 0.05f), center = Offset(w * (0.92f + p * 0.12f), eyeY + bodyH * 0.2f - p * h * 0.08f))
        }
        else -> {}
    }
}
