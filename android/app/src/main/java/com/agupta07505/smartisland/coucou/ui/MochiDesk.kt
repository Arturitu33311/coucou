/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.agupta07505.smartisland.R
import com.agupta07505.smartisland.coucou.CoucouRuntime
import com.agupta07505.smartisland.coucou.CoucouTask
import com.agupta07505.smartisland.coucou.CoucouTasks
import com.agupta07505.smartisland.coucou.SnoozePresets
import com.agupta07505.smartisland.coucou.TaskParser
import com.agupta07505.smartisland.coucou.TaskPreview
import com.agupta07505.smartisland.coucou.WhenDay
import com.agupta07505.smartisland.coucou.touchIsland
import kotlinx.coroutines.delay
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import kotlin.math.roundToInt

val JinxGreen = Color(0xFF39FF14)
private val Done = Color(0xFF4ADE80)

/** The grey of the small lines: 62 % white on the island's black reads at 4.5:1 or better. */
private val Secondary = Color.White.copy(alpha = 0.62f)

/** What the person just did to a task, offered back for a few seconds (Undo) or only confirmed. */
private data class Note(val id: String, val title: String, val canUndo: Boolean, val until: Long)

private const val NOTE_MS = 4_500L

/** How far a row is dragged before letting go snoozes it. */
private const val SNOOZE_DRAG_PX = 160f

/**
 * Mochi's desk: what you open the island to find when nothing else is going on. Mochi, your open
 * tasks (a tap finishes one and makes him smile; a swipe snoozes it, a long press rewords it), and a
 * place to add one without leaving the island. He is awake by day and looks up when something is overdue.
 */
@Composable
fun MochiDesk(onReplyStateChanged: (Boolean) -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val tasks by CoucouTasks.state.collectAsState()
    var smileUntil by remember { mutableLongStateOf(0L) }
    var adding by remember { mutableStateOf(false) }
    var editingId by remember { mutableStateOf<String?>(null) }
    var input by remember { mutableStateOf("") }
    var picking by remember { mutableStateOf(false) }
    var note by remember { mutableStateOf<Note?>(null) }
    val whenState = rememberWhenState()
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    var tickles by remember { mutableIntStateOf(0) }
    // The line under his name changes every so often when there is no timed task to show.
    var phraseStep by remember { mutableIntStateOf(0) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(20_000)
            phraseStep++
        }
    }
    val focus = remember { FocusRequester() }

    // The desk is opened to be read: bring the list up to date, and ask Jinx for hers.
    LaunchedEffect(Unit) {
        CoucouTasks.refresh(context)
        CoucouRuntime.kick()
    }
    // Mochi stops smiling after a moment.
    LaunchedEffect(smileUntil) {
        if (smileUntil > 0) {
            delay((smileUntil - System.currentTimeMillis()).coerceAtLeast(0))
            now = System.currentTimeMillis()
        }
    }
    // What was just done is offered back (Undo) for a few seconds, then forgotten.
    LaunchedEffect(note) {
        val n = note ?: return@LaunchedEffect
        delay((n.until - System.currentTimeMillis()).coerceAtLeast(0))
        if (note === n) note = null
    }
    // The keyboard needs the island's window to take focus first (what a quick reply does).
    LaunchedEffect(adding) {
        if (adding) {
            delay(180)
            runCatching { focus.requestFocus() }
        }
    }

    val open = tasks.filter { !it.done }.sortedWith(compareBy({ it.remindAt ?: Long.MAX_VALUE }, { it.created }))
    val overdue = open.count { it.remindAt != null && it.remindAt * 1000 < now }
    val scene = rememberMochiScene()
    val celebrating = rememberCelebration(open.size)
    val remindAts = open.mapNotNull { it.remindAt }
    val calm = rememberCalmMood(remindAts)
    val ring = rememberSoonProgress(remindAts)
    // Same attention order as his bubble (no look direction here: the desk fills the island, so
    // the stare stays centered). Music is ambience: it only gets him dancing when nothing needs
    // him; at night he naps through calm tasks.
    val mood = when {
        celebrating -> MochiMood.Celebrating
        // Finishing a task: a short burst of confetti.
        smileUntil > now -> MochiMood.Celebrating
        overdue > 0 -> MochiMood.Alert
        scene.batteryLow -> MochiMood.Tired
        scene.greeted -> MochiMood.Happy
        scene.jinxMood != null -> scene.jinxMood
        scene.surprised -> MochiMood.Surprised
        scene.glancing -> MochiMood.Glance
        scene.musicPlaying -> MochiMood.Dancing
        else -> calm
    }
    val tint = if (overdue > 0) ReminderAmber else MochiEmerald

    fun reset() {
        input = ""
        adding = false
        editingId = null
        picking = false
        whenState.reset()
        onReplyStateChanged(false)
    }

    // The calendar choice rides along with the sentence: the day picked wins, the sentence's
    // time of day is kept ("viernes a las 17:30"). Both are cleared after adding, so the next
    // task starts clean instead of inheriting yesterday's day.
    fun submit() {
        touchIsland(context)
        val text = input.trim()
        val editing = editingId
        if (text.isNotEmpty()) {
            runCatching {
                if (editing != null) {
                    CoucouTasks.edit(context, editing, text)
                } else {
                    CoucouTasks.add(context, text, pickedDay = whenState.dayStart, pickedHour = whenState.hour)
                }
            }
        }
        reset()
    }

    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            // A tap on him tickles him (the island stays open: he is a pet, not a button).
            LivelyMochi(
                size = 38.dp, tint = tint, mood = mood, outfit = rememberMochiOutfit(mood),
                tickleSignal = tickles,
                countdown = ring,
                // (No clip: it would cut his hat and the confetti. No ripple either.)
                modifier = Modifier.clickable(
                    interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() },
                    indication = null,
                ) {
                    touchIsland(context)
                    Haptics.tap(context)
                    tickles++
                },
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                val next = open.firstOrNull { it.remindAt != null && it.remindAt * 1000 >= now }
                val hour = java.util.Calendar.getInstance().apply { timeInMillis = now }.get(java.util.Calendar.HOUR_OF_DAY)
                // What he says is the headline; the count and the next timed task ride underneath.
                val phrase = MochiIdleScript.phraseAt(MochiIdleScript.phrases(open.size, overdue, hour), phraseStep)
                val phraseArg = phrase?.arg
                val headline = when {
                    smileUntil > now -> stringResource(R.string.coucou_desk_nice)
                    mood == MochiMood.Dancing -> stringResource(R.string.coucou_desk_dancing)
                    mood == MochiMood.Sleepy -> stringResource(R.string.coucou_desk_sleeping)
                    phrase == null -> ""
                    phraseArg != null -> stringResource(phrase.res, phraseArg)
                    else -> stringResource(phrase.res)
                }
                val count = when (MochiIdleScript.statusKind(open.size, overdue)) {
                    MochiIdleScript.StatusKind.Overdue ->
                        context.resources.getQuantityString(R.plurals.coucou_desk_overdue, overdue, overdue)
                    MochiIdleScript.StatusKind.Clear -> stringResource(R.string.coucou_desk_all_clear)
                    MochiIdleScript.StatusKind.Todo -> stringResource(R.string.coucou_desk_todo, open.size)
                }
                val status = count + (next?.let { " · ${time(it.remindAt!!)}  ${it.title}" } ?: "")
                Crossfade(targetState = headline, label = "desk-headline") { line ->
                    Text(
                        line,
                        color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.SemiBold,
                        maxLines = 1, overflow = TextOverflow.Ellipsis,
                    )
                }
                Text(
                    status,
                    color = Secondary, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
        }

        for (t in open.take(4)) key(t.id) {
            TaskRow(
                t,
                onCheck = {
                    touchIsland(context)
                    Haptics.tap(context)
                },
                onDone = {
                    CoucouTasks.setDone(context, t.id, true)
                    smileUntil = System.currentTimeMillis() + 1600
                    now = System.currentTimeMillis()
                    note = Note(t.id, t.title, canUndo = true, until = System.currentTimeMillis() + NOTE_MS)
                },
                onSnooze = {
                    touchIsland(context)
                    Haptics.tap(context)
                    CoucouTasks.snooze(context, t.id, SnoozePresets.SHORT_S)
                    now = System.currentTimeMillis()
                    note = Note(t.id, t.title, canUndo = false, until = System.currentTimeMillis() + NOTE_MS)
                },
                onEdit = {
                    touchIsland(context)
                    Haptics.tap(context)
                    editingId = t.id
                    input = t.title
                    adding = true
                    picking = false
                    onReplyStateChanged(true)
                },
            )
        }

        // What was just done, offered back; a snooze is only confirmed.
        AnimatedVisibility(
            visible = note != null,
            enter = fadeIn(tween(120)) + expandVertically(tween(160)),
            exit = fadeOut(tween(120)) + shrinkVertically(tween(160)),
        ) {
            val n = note
            if (n != null) {
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Done.copy(alpha = 0.10f))
                        .padding(start = 12.dp, end = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        stringResource(if (n.canUndo) R.string.coucou_desk_done_task else R.string.coucou_desk_snoozed_task, n.title),
                        color = Color.White.copy(alpha = 0.85f), fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).padding(vertical = 8.dp),
                    )
                    if (n.canUndo) {
                        Text(
                            stringResource(R.string.coucou_desk_undo),
                            color = Done, fontSize = 12.sp, fontWeight = FontWeight.SemiBold,
                            modifier = Modifier
                                .clip(RoundedCornerShape(50))
                                .clickable {
                                    touchIsland(context)
                                    Haptics.tap(context)
                                    CoucouTasks.setDone(context, n.id, false)
                                    note = null
                                }
                                .padding(horizontal = 12.dp, vertical = 8.dp),
                        )
                    }
                }
            }
        }

        if (!adding) {
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(50)).background(Color.White.copy(alpha = 0.07f))
                    .clickable { touchIsland(context); adding = true; onReplyStateChanged(true) }.padding(horizontal = 14.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("＋", color = MochiEmerald, fontSize = 14.sp, fontWeight = FontWeight.Bold)
                Spacer(Modifier.width(8.dp))
                Text(stringResource(R.string.coucou_desk_add), color = Secondary, fontSize = 13.sp)
            }
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(50)).border(1.dp, MochiEmerald.copy(alpha = 0.6f), RoundedCornerShape(50))
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.weight(1f)) {
                        if (input.isEmpty()) {
                            Text(stringResource(R.string.coucou_desk_hint), color = Color.White.copy(alpha = 0.55f), fontSize = 13.sp, maxLines = 1)
                        }
                        BasicTextField(
                            value = input,
                            onValueChange = { input = it },
                            singleLine = true,
                            textStyle = TextStyle(color = Color.White, fontSize = 13.sp),
                            cursorBrush = SolidColor(MochiEmerald),
                            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                            keyboardActions = KeyboardActions(onDone = { submit() }),
                            modifier = Modifier.fillMaxWidth().focusRequester(focus),
                        )
                    }
                    if (editingId == null) {
                        WhenCalendarButton(picked = whenState.dayStart != null) {
                            touchIsland(context)
                            picking = !picking
                        }
                    }
                    Icon(
                        Icons.Rounded.Close, stringResource(R.string.coucou_desk_cancel), tint = Secondary,
                        modifier = Modifier.size(20.dp).clip(CircleShape).clickable { touchIsland(context); reset() },
                    )
                }
                // What the sentence will become, before it is confirmed: the time it was understood as.
                val understood = remember(input, whenState.dayStart, whenState.hour, editingId) {
                    if (input.isBlank()) {
                        null
                    } else {
                        val offset = { t: Long -> TimeZone.getDefault().getOffset(t * 1000) / 1000L }
                        val nowSec = System.currentTimeMillis() / 1000
                        val parsed = TaskParser.parseWhen(input, nowSec, offset)
                        val picked = whenState.dayStart
                        val at = if (picked != null && editingId == null) {
                            TaskParser.withDay(picked, parsed.whenSec, whenState.hour, offset)
                        } else {
                            parsed.whenSec
                        }
                        TaskPreview.label(at, nowSec) { t -> TimeZone.getDefault().getOffset(t * 1000) / 1000L }
                    }
                }
                AnimatedVisibility(visible = understood != null, enter = fadeIn(tween(120)), exit = fadeOut(tween(100))) {
                    val label = understood
                    if (label != null) {
                        val clock = "%02d:%02d".format(label.hour, label.minute)
                        val text = when (label.day) {
                            WhenDay.Today -> stringResource(R.string.coucou_when_today, clock)
                            WhenDay.Tomorrow -> stringResource(R.string.coucou_when_tomorrow, clock)
                            WhenDay.Other -> stringResource(
                                R.string.coucou_when_other,
                                SimpleDateFormat("EEE d MMM HH:mm", Locale.getDefault()).format(Date(label.atSec * 1000)),
                            )
                        }
                        Row(
                            Modifier.clip(RoundedCornerShape(50)).background(MochiEmerald.copy(alpha = 0.16f))
                                .padding(horizontal = 10.dp, vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text("⏰", fontSize = 11.sp)
                            Spacer(Modifier.width(6.dp))
                            Text(text, color = MochiEmerald, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                        }
                    }
                }
                if (picking && editingId == null) WhenPickerPanel(whenState)
            }
        }
    }
}

private fun time(unixSec: Long): String = SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date(unixSec * 1000))

/**
 * One open task. Tick the circle to finish it (it fills, then the row folds away, then it is done); drag
 * the row to the right to snooze it ten minutes; hold it to reword it.
 */
@Composable
private fun TaskRow(
    t: CoucouTask,
    onCheck: () -> Unit,
    onDone: () -> Unit,
    onSnooze: () -> Unit,
    onEdit: () -> Unit,
) {
    val late = t.remindAt != null && t.remindAt * 1000 < System.currentTimeMillis()
    val reduced = rememberReducedMotion()
    // Checked first (the circle fills with a tick), then the row folds away, then the task is really done:
    // what you did is acknowledged, instead of the line vanishing.
    var checked by remember(t.id) { mutableStateOf(false) }
    var gone by remember(t.id) { mutableStateOf(false) }
    var drag by remember(t.id) { mutableFloatStateOf(0f) }
    LaunchedEffect(checked) {
        if (checked) {
            delay(if (reduced) 0L else 150L)
            gone = true
            delay(if (reduced) 0L else 200L)
            onDone()
        }
    }
    val fill by animateColorAsState(if (checked) Done else Color.Transparent, tween(120), label = "check-fill")
    val reveal = (drag / SNOOZE_DRAG_PX).coerceIn(0f, 1f)
    AnimatedVisibility(
        visible = !gone,
        exit = shrinkVertically(tween(if (reduced) 0 else 180)) + fadeOut(tween(if (reduced) 0 else 150)),
    ) {
        Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))) {
            // What the swipe will do, uncovered as the row slides.
            if (drag > 0f) {
                Row(
                    Modifier.matchParentSize().background(ReminderAmber.copy(alpha = 0.10f + 0.20f * reveal)).padding(start = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        stringResource(R.string.coucou_desk_snooze_hint),
                        color = ReminderAmber.copy(alpha = 0.55f + 0.45f * reveal), fontSize = 12.sp, fontWeight = FontWeight.SemiBold,
                    )
                }
            }
            Row(
                Modifier
                    .fillMaxWidth()
                    .offset { IntOffset(drag.roundToInt(), 0) }
                    .background(Color(0xFF0E0E0F))
                    .background(Color.White.copy(alpha = 0.05f))
                    .pointerInput(t.id) {
                        detectHorizontalDragGestures(
                            onDragEnd = {
                                if (drag >= SNOOZE_DRAG_PX) onSnooze()
                                drag = 0f
                            },
                            onDragCancel = { drag = 0f },
                        ) { change, amount ->
                            change.consume()
                            drag = (drag + amount).coerceIn(0f, SNOOZE_DRAG_PX * 1.4f)
                        }
                    }
                    .pointerInput(t.id) {
                        detectTapGestures(onLongPress = { onEdit() })
                    }
                    .padding(horizontal = 8.dp, vertical = 5.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    Modifier.size(22.dp).clip(CircleShape).background(fill)
                        .border(1.6.dp, if (checked) Done else if (late) ReminderAmber else Color.White.copy(alpha = 0.5f), CircleShape)
                        .clickable(enabled = !checked) {
                            onCheck()
                            checked = true
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    if (checked) Icon(Icons.Rounded.Check, null, tint = Color(0xFF0B1F12), modifier = Modifier.size(14.dp))
                }
                Spacer(Modifier.width(10.dp))
                Text(
                    t.title,
                    color = Color.White.copy(alpha = if (checked) 0.55f else 1f), fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                t.remindAt?.let {
                    Text("⏰ ${time(it)}", color = if (late) ReminderAmber else Secondary, fontSize = 11.sp)
                    Spacer(Modifier.width(4.dp))
                }
            }
        }
    }
}
