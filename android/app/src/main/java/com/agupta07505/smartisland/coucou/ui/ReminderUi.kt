/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Notifications
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.agupta07505.smartisland.R
import com.agupta07505.smartisland.coucou.CoucouTasks
import com.agupta07505.smartisland.coucou.SnoozePresets
import com.agupta07505.smartisland.model.IslandNotification

/** The reminder's colour: amber, as in Coucou on the PC. */
val ReminderAmber = Color(0xFFF5A524)

/** Left of the collapsed island: Mochi, looking up. */
@Composable
fun ReminderCollapsedGlyph() {
    MochiGlyph(size = 24.dp, tint = ReminderAmber, mood = MochiMood.Alert)
}

/** Right of the collapsed island: a bell that rings, then rests. */
@Composable
fun ReminderCollapsedRight() {
    val transition = rememberInfiniteTransition(label = "bell")
    val angle by transition.animateFloat(
        initialValue = 0f,
        targetValue = 0f,
        animationSpec = infiniteRepeatable(
            animation = keyframes {
                durationMillis = 2400
                0f at 0
                16f at 200 using FastOutSlowInEasing
                -13f at 400
                9f at 600
                -6f at 800
                0f at 1000
                0f at 2400
            },
            repeatMode = RepeatMode.Restart,
        ),
        label = "ring",
    )
    Icon(
        Icons.Rounded.Notifications,
        contentDescription = "Reminder",
        tint = ReminderAmber,
        modifier = Modifier
            .size(16.dp)
            .graphicsLayer { rotationZ = angle },
    )
}

/** The open island: the task, one tap to finish it, and three ways to put it off (ten minutes, an hour, tomorrow). */
@Composable
fun ReminderExpanded(
    notification: IslandNotification?,
    bottomPadding: Dp,
    onCollapse: () -> Unit,
) {
    val context = LocalContext.current
    val id = notification?.key?.let(CoucouTasks::taskIdOf)
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 18.dp)
            .padding(top = 6.dp, bottom = bottomPadding),
        verticalArrangement = Arrangement.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            MochiGlyph(size = 38.dp, tint = ReminderAmber, mood = MochiMood.Alert, outfit = rememberMochiOutfit(MochiMood.Alert))
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    notification?.title.orEmpty(),
                    color = Color.White,
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(stringResource(R.string.coucou_reminder_label), color = Color.White.copy(alpha = 0.62f), fontSize = 12.sp)
            }
            // Put away without doing anything to the task.
            Icon(
                Icons.Rounded.Close, stringResource(R.string.coucou_reminder_dismiss), tint = Color.White.copy(alpha = 0.62f),
                modifier = Modifier
                    .size(32.dp)
                    .clip(CircleShape)
                    .clickable {
                        id?.let { CoucouTasks.withdraw(context, it) }
                        onCollapse()
                    }
                    .padding(6.dp),
            )
        }
        Spacer(Modifier.size(10.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ActionPill(stringResource(R.string.coucou_reminder_done), Modifier.weight(1.25f), Color(0xFF4ADE80), icon = true) {
                id?.let { CoucouTasks.setDone(context, it, true) }
                onCollapse()
            }
            ActionPill(stringResource(R.string.coucou_reminder_10min), Modifier.weight(1f), ReminderAmber) {
                id?.let { CoucouTasks.snooze(context, it, SnoozePresets.SHORT_S) }
                onCollapse()
            }
            ActionPill(stringResource(R.string.coucou_reminder_1h), Modifier.weight(1f), ReminderAmber) {
                id?.let { CoucouTasks.snooze(context, it, SnoozePresets.HOUR_S) }
                onCollapse()
            }
            ActionPill(stringResource(R.string.coucou_reminder_tomorrow), Modifier.weight(1.1f), ReminderAmber) {
                id?.let {
                    val offset = { t: Long -> java.util.TimeZone.getDefault().getOffset(t * 1000) / 1000L }
                    CoucouTasks.snooze(context, it, SnoozePresets.untilTomorrow(System.currentTimeMillis() / 1000, offset))
                }
                onCollapse()
            }
        }
    }
}

@Composable
private fun ActionPill(
    label: String,
    modifier: Modifier,
    color: Color,
    icon: Boolean = false,
    cross: Boolean = false,
    onClick: () -> Unit,
) {
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(50))
            .background(color.copy(alpha = 0.16f))
            .clickable(onClick = onClick)
            .padding(vertical = 9.dp),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon) Icon(Icons.Rounded.Check, null, tint = color, modifier = Modifier.size(15.dp))
        if (cross) Icon(Icons.Rounded.Close, null, tint = color, modifier = Modifier.size(15.dp))
        if (icon || cross) Spacer(Modifier.width(4.dp))
        Text(label, color = color, fontSize = 13.sp, fontWeight = FontWeight.Medium, maxLines = 1)
    }
}

/** A small round marker used where a dot is enough. */
@Composable
fun AmberDot(size: Dp = 8.dp) {
    Spacer(Modifier.size(size).clip(CircleShape).background(ReminderAmber))
}
