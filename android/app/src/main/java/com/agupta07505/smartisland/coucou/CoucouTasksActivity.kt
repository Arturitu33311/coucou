/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import com.agupta07505.smartisland.coucou.ui.Chip
import com.agupta07505.smartisland.coucou.ui.CoucouColors
import com.agupta07505.smartisland.coucou.ui.EmptyState
import com.agupta07505.smartisland.coucou.ui.GroupLabel
import com.agupta07505.smartisland.coucou.ui.ScreenHeader
import com.agupta07505.smartisland.coucou.ui.SectionCard
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.TextButton
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.agupta07505.smartisland.coucou.phone.PhoneSetupActivity
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.jinxSync
import com.agupta07505.smartisland.coucou.ui.Haptics
import com.agupta07505.smartisland.coucou.ui.MochiGlyph
import com.agupta07505.smartisland.coucou.ui.MochiMood
import com.agupta07505.smartisland.coucou.ui.rememberMochiOutfit
import com.agupta07505.smartisland.coucou.ui.WhenCalendarButton
import com.agupta07505.smartisland.coucou.ui.WhenPickerPanel
import com.agupta07505.smartisland.coucou.ui.rememberWhenState
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * The Coucou app's entry for the pendientes (the launcher icon, a reminder's notification, the island):
 * the app opened on that tab. `EXTRA_TAB` picks another one.
 */
class CoucouTasksActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // What SmartIsland's own activity did on opening: bring its services back if the system stopped them.
        com.agupta07505.smartisland.util.SystemServiceRecovery.requestRecovery(this)
        enableEdgeToEdge()
        val tab = CoucouTab.entries.firstOrNull { it.name == intent.getStringExtra(EXTRA_TAB) } ?: CoucouTab.Tasks
        setContent { CoucouHome(initial = tab) }
    }

    override fun onResume() {
        super.onResume()
        com.agupta07505.smartisland.util.SystemServiceRecovery.requestRecovery(this)
    }

    companion object {
        const val EXTRA_TAB = "coucou_tab"
    }
}

/** The pendientes with their state: the list, the sentence being typed, the day picked, the sync with Jinx. */
@Composable
internal fun TasksScreen(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    var tasks by remember { mutableStateOf(CoucouTasks.list(context)) }
    var input by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var picking by remember { mutableStateOf(false) }
    val whenState = rememberWhenState()
    val jinxOn = context.jinxSync
    val outcome by CoucouRuntime.jinx.collectAsState()
    // The sync may have changed the list (she closed one, a task was handed over): show it as it is.
    LaunchedEffect(outcome) { tasks = CoucouTasks.list(context) }
    LaunchedEffect(Unit) {
        CoucouRuntime.apply(context)
        CoucouRuntime.kick()
    }

    // The picked day rides along exactly like in the island: the button only opens the inline
    // panel (no dialog), and a successful add clears the choice.
    fun add() {
        if (input.isBlank()) return
        runCatching { CoucouTasks.add(context, input, pickedDay = whenState.dayStart, pickedHour = whenState.hour) }
            .onSuccess { tasks = it; input = ""; error = null; picking = false; whenState.reset() }
            .onFailure { error = it.message }
    }

    TasksContent(
        tasks = tasks,
        input = input,
        onInput = { input = it },
        onAdd = { add() },
        picking = picking,
        dayPicked = whenState.dayStart != null,
        onTogglePicker = { picking = !picking },
        picker = { WhenPickerPanel(whenState) },
        error = error,
        syncError = outcome.error?.takeIf { jinxOn },
        jinxOn = jinxOn,
        onDone = { id, done -> tasks = CoucouTasks.setDone(context, id, done).also { Haptics.tap(context) } },
        onDelete = { id -> tasks = CoucouTasks.delete(context, id) },
        modifier = modifier,
    )
}

/** The pendientes, stateless (drawn as they are given, so a test can draw them). */
@Composable
internal fun TasksContent(
    tasks: List<CoucouTask>,
    input: String,
    onInput: (String) -> Unit,
    onAdd: () -> Unit,
    picking: Boolean,
    dayPicked: Boolean,
    onTogglePicker: () -> Unit,
    picker: @Composable () -> Unit,
    error: String?,
    syncError: String?,
    jinxOn: Boolean,
    onDone: (String, Boolean) -> Unit,
    onDelete: (String) -> Unit,
    modifier: Modifier = Modifier,
    now: Long = System.currentTimeMillis() / 1000,
) {
    val open = tasks.filter { !it.done }.sortedWith(compareBy({ it.remindAt ?: Long.MAX_VALUE }, { it.created }))
    val done = tasks.filter { it.done }.sortedByDescending { it.completedAt ?: 0 }
    val late = open.count { (it.remindAt ?: Long.MAX_VALUE) < now }
    val subtitle = when {
        open.isEmpty() -> "Nada por hacer"
        late > 0 -> "${open.size} por hacer · $late vencida${if (late > 1) "s" else ""}"
        else -> "${open.size} por hacer"
    }

    LazyColumn(
        modifier.fillMaxSize().padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        contentPadding = PaddingValues(bottom = 24.dp),
    ) {
        item(key = "header") {
            ScreenHeader(
                title = "Pendientes",
                subtitle = subtitle,
                leading = { MochiGlyph(size = 40.dp, mood = if (late > 0) MochiMood.Alert else MochiMood.Idle, outfit = rememberMochiOutfit(MochiMood.Idle)) },
                actions = { if (jinxOn) Chip(if (syncError == null) "con Jinx" else "sin sync", dot = if (syncError == null) CoucouColors.Accent else CoucouColors.Warning) },
            )
        }
        item(key = "add") {
            SectionCard {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    TextField(
                        value = input,
                        onValueChange = onInput,
                        modifier = Modifier.weight(1f),
                        singleLine = true,
                        placeholder = { Text("llamar a mamá a las 17:30 · en 20 min", color = CoucouColors.TextTertiary, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                        shape = RoundedCornerShape(14.dp),
                        colors = TextFieldDefaults.colors(
                            focusedContainerColor = CoucouColors.SurfaceHigh,
                            unfocusedContainerColor = CoucouColors.SurfaceHigh,
                            focusedIndicatorColor = Color.Transparent,
                            unfocusedIndicatorColor = Color.Transparent,
                            cursorColor = CoucouColors.Accent,
                            focusedTextColor = CoucouColors.Text,
                            unfocusedTextColor = CoucouColors.Text,
                        ),
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                        keyboardActions = KeyboardActions(onDone = { onAdd() }),
                    )
                    Spacer(Modifier.width(4.dp))
                    WhenCalendarButton(picked = dayPicked, onClick = onTogglePicker)
                    IconButton(
                        onClick = onAdd,
                        enabled = input.isNotBlank(),
                        colors = IconButtonDefaults.iconButtonColors(
                            containerColor = CoucouColors.Accent,
                            contentColor = Color(0xFF04140D),
                            disabledContainerColor = CoucouColors.SurfaceHigh,
                            disabledContentColor = CoucouColors.TextTertiary,
                        ),
                    ) { Icon(Icons.Rounded.Add, "Añadir") }
                }
                if (picking) picker()
                Text(
                    "Escribe la hora en la frase («mañana a las 9», «en 20 min») o elige un día en el calendario.",
                    color = CoucouColors.TextTertiary, fontSize = 12.sp,
                )
                error?.let { Text(it, color = CoucouColors.Warning, fontSize = 12.5.sp) }
            }
        }
        syncError?.let { e ->
            item(key = "sync") {
                Text(
                    "No pude sincronizar con Jinx: $e",
                    color = CoucouColors.Warning, fontSize = 12.5.sp,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
                        .background(CoucouColors.Warning.copy(alpha = 0.10f)).padding(horizontal = 12.dp, vertical = 8.dp),
                )
            }
        }
        if (open.isEmpty() && done.isEmpty()) {
            item(key = "empty") { EmptyState("Nada pendiente", "Lo que añadas aquí te lo recuerda Mochi en la isla, y Jinx también lo ve.") }
        }
        if (open.isNotEmpty()) item(key = "open-label") { GroupLabel("Por hacer") }
        items(open, key = { it.id }) { t -> TaskRow(t, now, onDone, onDelete) }
        if (done.isNotEmpty()) item(key = "done-label") { GroupLabel("Hechas") }
        items(done, key = { it.id }) { t -> TaskRow(t, now, onDone, onDelete) }
    }
}

private val whenFormat = SimpleDateFormat("EEE d MMM · HH:mm", Locale.getDefault())

@Composable
private fun TaskRow(t: CoucouTask, now: Long, onDone: (String, Boolean) -> Unit, onDelete: (String) -> Unit) {
    val late = !t.done && (t.remindAt ?: Long.MAX_VALUE) < now
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp))
            .background(if (t.done) CoucouColors.Surface.copy(alpha = 0.6f) else CoucouColors.Surface)
            .clickable { onDone(t.id, !t.done) }
            .padding(start = 6.dp, end = 2.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(44.dp), contentAlignment = Alignment.Center) {
            Box(
                Modifier.size(22.dp).clip(CircleShape)
                    .background(if (t.done) CoucouColors.Accent else Color.Transparent)
                    .border(2.dp, if (t.done) CoucouColors.Accent else if (late) CoucouColors.Warning else CoucouColors.TextTertiary, CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                if (t.done) Icon(Icons.Rounded.Check, "Hecha", tint = Color(0xFF04140D), modifier = Modifier.size(15.dp))
            }
        }
        Column(Modifier.weight(1f).padding(vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                t.title,
                color = CoucouColors.Text.copy(alpha = if (t.done) 0.4f else 1f),
                textDecoration = if (t.done) TextDecoration.LineThrough else null,
                fontSize = 15.sp,
            )
            val meta = buildList {
                t.remindAt?.takeIf { !t.done }?.let { add((if (late) "Vencida · " else "") + whenFormat.format(Date(it * 1000))) }
                if (t.jinx) add("con Jinx")
                t.origin?.let { add("de $it") }
            }
            if (meta.isNotEmpty()) {
                Text(
                    meta.joinToString("  ·  "),
                    color = if (late) CoucouColors.Warning else CoucouColors.TextTertiary,
                    fontSize = 12.sp,
                )
            }
        }
        IconButton(onClick = { onDelete(t.id) }) {
            Icon(Icons.Rounded.Close, "Borrar", tint = CoucouColors.TextTertiary, modifier = Modifier.size(18.dp))
        }
    }
}
