/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.agupta07505.smartisland.coucou.CoucouTasks
import com.agupta07505.smartisland.model.IslandNotification

/** The desk's state, carried by the notification itself: `progress` is how many tasks are open, `category` "overdue" or "clear". */
object DeskState {
    const val OVERDUE = "overdue"
    const val CLEAR = "clear"
}

/**
 * Left of the collapsed island, when his desk is the only thing on it and so he lives in the main
 * pill instead of the bubble. He is the same Mochi as the bubble's: same moods (news, music, Claude
 * working, overdue, night), same outfits, same little gestures. [notification] only carries the count
 * on the right; what he feels is read from the tasks and the room, exactly like the bubble.
 */
@Composable
@Suppress("UNUSED_PARAMETER")
fun DeskCollapsedGlyph(notification: IslandNotification?) {
    val b = bubbleMood(lookDir = 0f)
    LivelyMochi(24.dp, b.tint, b.mood, outfit = rememberMochiOutfit(b.mood), countdown = b.ring)
}

/** Right of the collapsed island: how many things are open. */
@Composable
fun DeskCollapsedRight(notification: IslandNotification?) {
    val n = notification?.progress ?: 0
    if (n > 0) {
        val overdue = notification?.category == DeskState.OVERDUE
        CountBadge(n, if (overdue) ReminderAmber else MochiEmerald, if (overdue) ReminderAmber else Color.White)
    }
}

/**
 * How many things are open. When the number changes (a task done, one added) it rolls to the new value
 * and gives a small pulse: the island answers what was just done, instead of silently being different.
 */
@Composable
fun CountBadge(n: Int, tint: Color, textColor: Color) {
    val reduced = rememberReducedMotion()
    val pulse = remember { Animatable(1f) }
    var last by remember { mutableIntStateOf(n) }
    LaunchedEffect(n) {
        if (n != last) {
            last = n
            if (!reduced) {
                pulse.snapTo(1.14f)
                pulse.animateTo(1f, spring(dampingRatio = 0.55f, stiffness = 500f))
            }
        }
    }
    Box(
        Modifier
            .graphicsLayer { scaleX = pulse.value; scaleY = pulse.value }
            .clip(RoundedCornerShape(50))
            .background(tint.copy(alpha = 0.22f))
            .padding(horizontal = 8.dp, vertical = 2.dp),
    ) {
        AnimatedContent(
            targetState = n,
            transitionSpec = {
                if (reduced) {
                    fadeIn(tween(0)) togetherWith fadeOut(tween(0))
                } else {
                    (slideInVertically(tween(180)) { it / 2 } + fadeIn(tween(180))) togetherWith
                        (slideOutVertically(tween(140)) { -it / 2 } + fadeOut(tween(110)))
                }
            },
            label = "count",
        ) { v ->
            Text("$v", color = textColor, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
        }
    }
}

/**
 * What Mochi feels like in his bubble. Priority is attention-first: a finished list celebrates
 * over everything (it only lasts 3 s), then something overdue, a tired battery, a hello, news
 * (pop first, stare after), music; at night he naps through calm tasks; emptiness naps by day.
 */
private data class BubbleMood(val mood: MochiMood, val tint: Color, val open: Int, val ring: Float = 0f)

@Composable
private fun bubbleMood(lookDir: Float): BubbleMood {
    val tasks by CoucouTasks.state.collectAsState()
    val scene = rememberMochiScene()
    val now = System.currentTimeMillis()
    val open = tasks.filter { !it.done }
    val overdue = open.any { it.remindAt != null && it.remindAt * 1000 <= now }
    val celebrating = rememberCelebration(open.size)
    val remindAts = open.mapNotNull { it.remindAt }
    val calm = rememberCalmMood(remindAts)
    val ring = rememberSoonProgress(remindAts)
    val mood = when {
        celebrating -> MochiMood.Celebrating
        overdue -> MochiMood.Alert
        scene.batteryLow -> MochiMood.Tired
        scene.greeted -> MochiMood.Happy
        // Jinx is speaking: it is her mood he wears (and nothing of Claude Code's: that has its own card).
        scene.jinxMood != null -> scene.jinxMood
        scene.surprised -> MochiMood.Surprised
        scene.glancing -> MochiMood.Glance
        scene.musicPlaying -> MochiMood.Dancing
        else -> MochiReactions.calmOrJinx(calm, scene.jinxBackground)
    }
    return BubbleMood(mood, if (overdue) ReminderAmber else MochiEmerald, open.size, ring)
}

/**
 * Mochi's secondary bubble: him, always, feeling whatever is going on (celebration, overdue,
 * news and its stare, music, night, nothing). He reads the tasks and the island directly, so
 * every place that shows him stays in sync with no parameters to thread through — except
 * [lookDir], which only the island knows (which side the main pill sits on).
 */
@Composable
fun MochiBubble(size: Dp = 22.dp, lookDir: Float = 0f) {
    val b = bubbleMood(lookDir)
    LivelyMochi(size, b.tint, b.mood, lookX = lookDir, outfit = rememberMochiOutfit(b.mood), countdown = b.ring)
}

/** His bubble grown into a mini-pill (expanded island): him, with how many things are open. */
@Composable
fun MochiBubblePill(lookDir: Float = 0f) {
    val b = bubbleMood(lookDir)
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        LivelyMochi(20.dp, b.tint, b.mood, lookX = lookDir, outfit = rememberMochiOutfit(b.mood), countdown = b.ring)
        if (b.open > 0) {
            CountBadge(b.open, b.tint, if (b.mood == MochiMood.Alert) ReminderAmber else Color.White)
        }
    }
}
