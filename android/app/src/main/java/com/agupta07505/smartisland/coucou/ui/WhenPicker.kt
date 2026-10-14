/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.CalendarMonth
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale

/** One day of the strip: epoch seconds of its local midnight, for [com.agupta07505.smartisland.coucou.TaskParser.withDay]. */
data class StripDay(val dayStartSec: Long, val number: String, val weekday: String, val isToday: Boolean)

object WhenDays {
    /** Today and the next 13 days, each at its own local midnight (daylight-saving safe). */
    fun strip(nowMs: Long = System.currentTimeMillis()): List<StripDay> {
        val locale = Locale.getDefault()
        val letter = SimpleDateFormat("EEE", locale)
        val day = SimpleDateFormat("d", locale)
        return (0..13).map { i ->
            val c = Calendar.getInstance().apply {
                timeInMillis = nowMs
                set(Calendar.HOUR_OF_DAY, 0); set(Calendar.MINUTE, 0); set(Calendar.SECOND, 0); set(Calendar.MILLISECOND, 0)
                add(Calendar.DATE, i)
            }
            StripDay(
                dayStartSec = c.timeInMillis / 1000,
                number = day.format(c.time),
                weekday = letter.format(c.time).take(1).uppercase(locale),
                isToday = i == 0,
            )
        }
    }

    /** "vie 10 oct · 18:00": what the panel confirms back, so a picked day is never a mystery. */
    fun summary(dayStartSec: Long, hour: Int): String {
        val date = SimpleDateFormat("EEE d MMM", Locale.getDefault()).format(Date(dayStartSec * 1000))
        return "$date · %02d:00".format(hour)
    }
}

/**
 * A screen's calendar choice: a day (its local midnight) and an hour. Both add-task screens share
 * this shape. Nothing picked is itself the answer — then the sentence parser decides alone.
 */
class WhenState {
    var dayStart by mutableStateOf<Long?>(null)
    var hour by mutableIntStateOf(9)
    fun reset() {
        dayStart = null
        hour = 9
    }
}

@Composable
fun rememberWhenState(): WhenState = remember { WhenState() }

/** The little calendar button for the add-a-task row: dotted while a day is picked. */
@Composable
fun WhenCalendarButton(picked: Boolean, onClick: () -> Unit) {
    Box {
        IconButton(onClick = onClick, modifier = Modifier.size(32.dp)) {
            Icon(
                Icons.Rounded.CalendarMonth, "Pick a day",
                tint = if (picked) MochiEmerald else Color.White.copy(alpha = 0.5f),
                modifier = Modifier.size(20.dp),
            )
        }
        if (picked) {
            Box(
                Modifier.align(Alignment.TopEnd).padding(top = 6.dp, end = 6.dp).size(8.dp)
                    .clip(CircleShape).background(MochiEmerald),
            )
        }
    }
}

/**
 * The picker itself, inline (no window, so it works inside the island): a strip with the next
 * two weeks, hour chips, and what was picked with a way to clear it.
 */
@Composable
fun WhenPickerPanel(state: WhenState) {
    val days = remember { WhenDays.strip() }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            items(days, key = { it.dayStartSec }) { d ->
                val selected = state.dayStart == d.dayStartSec
                Column(
                    Modifier.clip(RoundedCornerShape(12.dp))
                        .background(if (selected) MochiEmerald else Color.White.copy(alpha = 0.07f))
                        .clickable { state.dayStart = if (selected) null else d.dayStartSec }
                        .padding(horizontal = 9.dp, vertical = 6.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(
                        if (d.isToday) "hoy" else d.weekday,
                        color = if (selected) Color(0xFF1B1B20) else Color.White.copy(alpha = 0.55f),
                        fontSize = 10.sp, fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        d.number,
                        color = if (selected) Color(0xFF1B1B20) else Color.White,
                        fontSize = 15.sp, fontWeight = FontWeight.Bold,
                    )
                }
            }
        }
        if (state.dayStart != null) {
            // A few whole hours, not a full clock: picking a time must stay a one-tap gesture
            // inside the island (a time spinner would not fit its height).
            val hours = remember { listOf(8, 9, 10, 12, 14, 16, 18, 20) }
            LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                items(hours) { h ->
                    val selected = state.hour == h
                    Box(
                        Modifier.clip(RoundedCornerShape(50))
                            .border(
                                1.dp,
                                if (selected) MochiEmerald else Color.White.copy(alpha = 0.18f),
                                RoundedCornerShape(50),
                            )
                            .background(if (selected) MochiEmerald.copy(alpha = 0.25f) else Color.Transparent)
                            .clickable { state.hour = h }
                            .padding(horizontal = 11.dp, vertical = 6.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            "%02d:00".format(h),
                            color = if (selected) MochiEmerald else Color.White.copy(alpha = 0.7f),
                            fontSize = 12.sp, fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    "⏰ " + WhenDays.summary(state.dayStart!!, state.hour),
                    color = Color.White.copy(alpha = 0.75f), fontSize = 12.sp, modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(4.dp))
                Icon(
                    Icons.Rounded.Close, "Clear day",
                    tint = Color.White.copy(alpha = 0.5f),
                    modifier = Modifier.size(20.dp).clip(CircleShape).clickable { state.reset() },
                )
            }
        }
    }
}
