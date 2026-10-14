/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * Mochi with a life of his own: the Compose side of [MochiIdleScript]. [rememberCalmMood] decides
 * awake or asleep (night, or 20 quiet minutes) and [LivelyMochi] plays his little gestures, the
 * thought bubble, the charging bolt and the countdown ring around a plain [MochiGlyph].
 */

package com.agupta07505.smartisland.coucou.ui

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.database.ContentObserver
import android.os.BatteryManager
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.provider.Settings
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.agupta07505.smartisland.coucou.presence.DeviceActivity
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import java.util.Calendar
import kotlin.random.Random

/** Whether the person has turned system animations off ("Remove animations"): his little gestures stop too. */
@Composable
fun rememberReducedMotion(): Boolean {
    val app = LocalContext.current.applicationContext
    fun read() = Settings.Global.getFloat(app.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    var reduced by remember { mutableStateOf(read()) }
    DisposableEffect(app) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) {
                reduced = read()
            }
        }
        runCatching {
            app.contentResolver.registerContentObserver(
                Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE), false, observer,
            )
        }
        onDispose { runCatching { app.contentResolver.unregisterContentObserver(observer) } }
    }
    return reduced
}

private fun screenOn(context: Context): Boolean =
    (context.getSystemService(Context.POWER_SERVICE) as? PowerManager)?.isInteractive != false

/**
 * What he is when nothing else decides it. Re-evaluated on a slow clock while awake (the 20 minutes
 * and the "due soon" window drift with time), quickly while asleep so a touch wakes him at once, and
 * hardly at all while the screen is off: the screen coming on (an unlock) re-evaluates immediately.
 * [remindAts] are the open tasks' reminder times in unix seconds.
 */
@Composable
fun rememberCalmMood(remindAts: List<Long>): MochiMood {
    val app = LocalContext.current.applicationContext
    val unlockMs = rememberUnlockMs()
    fun compute(): MochiMood {
        val now = System.currentTimeMillis()
        return MochiIdleScript.calmMood(
            night = MochiReactions.isNight(now),
            idleSeconds = DeviceActivity.idleSeconds(),
            soonTask = MochiIdleScript.isSoon(remindAts, now),
        )
    }
    var calm by remember { mutableStateOf(compute()) }
    LaunchedEffect(remindAts, unlockMs) {
        while (isActive) {
            calm = compute()
            delay(
                when {
                    !screenOn(app) -> 60_000L
                    calm == MochiMood.Sleepy -> 2_000L
                    else -> 20_000L
                },
            )
        }
    }
    return calm
}

/** How far into the 15 minutes before the next reminder it is (0: none close); the ring around Mochi. */
@Composable
fun rememberSoonProgress(remindAts: List<Long>): Float {
    val app = LocalContext.current.applicationContext
    var progress by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(remindAts) {
        while (isActive) {
            progress = MochiIdleScript.soonProgress(remindAts, System.currentTimeMillis())
            delay(if (!screenOn(app)) 60_000L else if (progress > 0f) 10_000L else 30_000L)
        }
    }
    return progress
}

/** Plugged in, watched live (sticky battery state for the start, plug events after). */
@Composable
fun rememberCharging(): Boolean {
    val app = LocalContext.current.applicationContext
    var charging by remember { mutableStateOf(false) }
    DisposableEffect(app) {
        fun read(i: Intent?): Boolean {
            val plugged = i?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0
            return plugged != 0
        }
        // A null receiver just returns the last sticky battery broadcast.
        charging = read(app.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED)))
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(c: Context?, intent: Intent?) {
                charging = when (intent?.action) {
                    Intent.ACTION_POWER_CONNECTED -> true
                    Intent.ACTION_POWER_DISCONNECTED -> false
                    else -> charging
                }
            }
        }
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_POWER_CONNECTED)
            addAction(Intent.ACTION_POWER_DISCONNECTED)
        }
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            app.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            app.registerReceiver(receiver, filter)
        }
        onDispose { runCatching { app.unregisterReceiver(receiver) } }
    }
    return charging
}

/**
 * [MochiGlyph] that does things by himself while [mood] is [MochiMood.Idle]: a gesture every
 * 6–15 s (yawn, stretch, looking up or around, a hop, a whistle, a scratch, a wink, a thought bubble),
 * a stretch when he wakes, a bolt while charging. [tickleSignal] bumps → he giggles. [countdown] (0..1)
 * draws a ring that closes as a reminder comes due. Gestures rest while the screen is off and while
 * the system's animations are off; every other mood is drawn exactly as before.
 */
@Composable
fun LivelyMochi(
    size: Dp = 24.dp,
    tint: Color = MochiEmerald,
    mood: MochiMood = MochiMood.Idle,
    modifier: Modifier = Modifier,
    lookX: Float = 0f,
    outfit: Outfit = Outfit.None,
    tickleSignal: Int = 0,
    countdown: Float = 0f,
) {
    val app = LocalContext.current.applicationContext
    var gesture by remember { mutableStateOf(IdleGesture.None) }
    var thought by remember { mutableStateOf("💭") }
    val progress = remember { Animatable(0f) }
    val charging = rememberCharging()
    val reduced by rememberUpdatedState(rememberReducedMotion())
    var lastMood by remember { mutableStateOf(mood) }
    var lastTickle by remember { mutableStateOf(tickleSignal) }

    LaunchedEffect(mood, tickleSignal) {
        val woke = lastMood == MochiMood.Sleepy && mood == MochiMood.Idle
        val tickled = tickleSignal != lastTickle
        lastMood = mood
        lastTickle = tickleSignal
        if (mood != MochiMood.Idle) {
            gesture = IdleGesture.None
            return@LaunchedEffect
        }
        val rnd = Random(System.nanoTime())
        suspend fun play(g: IdleGesture) {
            gesture = g
            progress.snapTo(0f)
            progress.animateTo(1f, tween(MochiIdleScript.durationMs(g).toInt(), easing = LinearEasing))
            gesture = IdleGesture.None
        }
        var prev = IdleGesture.None
        if (MochiIdleScript.gesturesAllowed(reduced, screenOn(app))) {
            if (tickled) {
                prev = IdleGesture.Tickle
                play(IdleGesture.Tickle)
            } else if (woke) {
                // Waking up: a proper stretch before anything else.
                prev = IdleGesture.Stretch
                play(IdleGesture.Stretch)
            }
        }
        while (isActive) {
            delay(MochiIdleScript.gapMs(rnd))
            if (!MochiIdleScript.gesturesAllowed(reduced, screenOn(app))) continue
            val hour = Calendar.getInstance().get(Calendar.HOUR_OF_DAY)
            val g = MochiIdleScript.next(prev, hour, allowThought = true, rnd = rnd)
            if (g == IdleGesture.Think) thought = MochiIdleScript.thoughtFor(hour, rnd)
            play(g)
            prev = g
        }
    }

    val ring by animateFloatAsState(countdown, tween(600), label = "countdownRing")
    val inner by animateDpAsState(if (countdown > 0f) size * 0.82f else size, tween(300), label = "countdownSize")
    Box(modifier.size(size), contentAlignment = Alignment.Center) {
        if (ring > 0f) {
            // A thin ring that closes as the reminder comes due: a state, not a decoration.
            Canvas(Modifier.size(size)) {
                val stroke = (this.size.minDimension * 0.07f).coerceAtLeast(1.5f)
                val inset = stroke / 2f
                val box = Size(this.size.width - stroke, this.size.height - stroke)
                drawArc(tint.copy(alpha = 0.22f), 0f, 360f, false, Offset(inset, inset), box, style = Stroke(stroke))
                drawArc(
                    tint.copy(alpha = 0.95f), -90f, 360f * ring.coerceIn(0f, 1f), false,
                    Offset(inset, inset), box, style = Stroke(stroke, cap = StrokeCap.Round),
                )
            }
        }
        MochiGlyph(
            size = inner,
            tint = tint,
            mood = mood,
            lookX = lookX,
            outfit = outfit,
            gesture = gesture,
            gestureProgress = progress.value,
            charging = charging,
            thought = thought,
        )
    }
}
