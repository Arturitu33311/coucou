/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * How Jinx's messages look on the island: Mochi in his emerald, in whatever
 * mood the message carries, with the text beside him. The mood travels in
 * the notification category as "mochi:<Mood>:<kind>" (see IslandJinx).
 */

package com.agupta07505.smartisland.coucou.presence

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.agupta07505.smartisland.coucou.ui.MochiEmerald
import com.agupta07505.smartisland.coucou.ui.MochiGlyph
import com.agupta07505.smartisland.coucou.ui.MochiMood
import com.agupta07505.smartisland.model.IslandNotification

/** "mochi:Happy:info" -> Happy; anything unknown -> Listening. Pure, tested. */
fun jinxMoodOf(category: String?): MochiMood {
    val name = category?.split(":")?.getOrNull(1) ?: return MochiMood.Listening
    return runCatching { MochiMood.valueOf(name) }.getOrDefault(MochiMood.Listening)
}

/** "mochi:Happy:alert" -> true. Pure, tested. */
fun jinxIsAlert(category: String?): Boolean =
    category?.split(":")?.getOrNull(2) == "alert"

/** Left of the collapsed island: Mochi in emerald, in the message's mood. */
@Composable
fun JinxMsgCollapsedGlyph(notification: IslandNotification?) {
    MochiGlyph(24.dp, MochiEmerald, jinxMoodOf(notification?.category))
}

/** Right of the collapsed island: a bell when it is urgent, nothing otherwise. */
@Composable
fun JinxMsgCollapsedRight(notification: IslandNotification?) {
    if (jinxIsAlert(notification?.category)) {
        com.agupta07505.smartisland.coucou.ui.ReminderCollapsedRight()
    }
}

/** The open island: what Jinx said, and one button to mark it read. */
@Composable
fun JinxMsgExpanded(
    notification: IslandNotification?,
    bottomPadding: Dp,
    onCollapse: () -> Unit,
) {
    val context = LocalContext.current
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 18.dp)
            .padding(top = 6.dp, bottom = bottomPadding),
        verticalArrangement = Arrangement.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            MochiGlyph(size = 38.dp, tint = MochiEmerald, mood = jinxMoodOf(notification?.category))
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    "Jinx",
                    color = Color.White,
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    notification?.text.orEmpty(),
                    color = Color.White.copy(alpha = 0.85f),
                    fontSize = 14.sp,
                    maxLines = 4,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Spacer(Modifier.size(10.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            Button(onClick = {
                MochiPresence.ackShown(context)
                onCollapse()
            }) { Text("Entendido") }
        }
    }
}
