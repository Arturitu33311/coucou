/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.keyframes
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material.icons.rounded.QuestionMark
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.res.stringResource
import com.agupta07505.smartisland.R
import com.agupta07505.smartisland.coucou.CoucouRuntime
import com.agupta07505.smartisland.coucou.IslandAgents
import com.agupta07505.smartisland.coucou.agents.ActivityKind
import com.agupta07505.smartisland.coucou.agents.AgentActivity
import com.agupta07505.smartisland.coucou.agents.ConfirmActivity
import com.agupta07505.smartisland.coucou.agents.Pending
import com.agupta07505.smartisland.coucou.agents.RemoteProtocol
import com.agupta07505.smartisland.coucou.touchIsland
import com.agupta07505.smartisland.coucou.agents.SessionState
import com.agupta07505.smartisland.model.IslandNotification
import kotlinx.coroutines.delay

/** Claude Code's colour, as on the PC. */
val ClaudeOrange = Color(0xFFD97757)
val QuestionBlue = Color(0xFF60A5FA)
private val Green = Color(0xFF4ADE80)
private val Red = Color(0xFFF4505E)

/** "Editing Mochi.kt" in the person's language: the verb from resources, the subject as the laptop sent it. */
fun activityLine(context: android.content.Context, a: com.agupta07505.smartisland.coucou.agents.Activity): String {
    val res = when (a.verb) {
        "Thinking" -> R.string.coucou_act_thinking
        "Reading" -> R.string.coucou_act_reading
        "Editing" -> R.string.coucou_act_editing
        "Writing" -> R.string.coucou_act_writing
        "Searching" -> R.string.coucou_act_searching
        "Listing" -> R.string.coucou_act_listing
        "Searching the web" -> R.string.coucou_act_web_search
        "Fetching" -> R.string.coucou_act_fetching
        "Running" -> R.string.coucou_act_running
        "Delegating" -> R.string.coucou_act_delegating
        "Planning" -> R.string.coucou_act_planning
        "Using" -> R.string.coucou_act_using
        "Needs you" -> R.string.coucou_act_needs_you
        "Done" -> R.string.coucou_act_done
        "Idle" -> R.string.coucou_act_idle
        else -> null
    }
    val verb = res?.let { context.getString(it) } ?: a.verb
    return if (a.subject.isEmpty()) verb else "$verb ${a.subject}"
}

/** What the island card says Claude Code is doing: the kind rides in the notification's category. */
const val ACTIVITY_CATEGORY_PREFIX = "activity:"

private fun kindOf(notification: IslandNotification?): ActivityKind? {
    val c = notification?.category ?: return null
    if (c == com.agupta07505.smartisland.model.AGENT_NEEDS_YOU) return ActivityKind.Asking
    if (!c.startsWith(ACTIVITY_CATEGORY_PREFIX)) return null
    val name = c.removePrefix(ACTIVITY_CATEGORY_PREFIX)
    return ActivityKind.entries.firstOrNull { it.name == name }
}

/** The colours of the laptop's Mochi dots: working blue, searching indigo, thinking violet. */
private val DotBlue = Color(0xFF4F9BFF)
private val DotIndigo = Color(0xFF6365F2)
private val DotViolet = Color(0xFF8B5CF6)

private val DoneGreen = Color(0xFF4ADE80)

private fun colorOf(kind: ActivityKind?): Color = when (kind) {
    ActivityKind.Finished -> DoneGreen
    ActivityKind.Thinking -> DotViolet
    ActivityKind.Searching, ActivityKind.Browsing -> DotIndigo
    ActivityKind.Asking, ActivityKind.Waiting -> ReminderAmber
    null, ActivityKind.Idle -> ClaudeOrange
    else -> DotBlue
}

private fun moodOf(kind: ActivityKind?): MochiMood = when (kind) {
    ActivityKind.Finished -> MochiMood.Happy
    ActivityKind.Thinking -> MochiMood.Thinking
    ActivityKind.Searching, ActivityKind.Browsing -> MochiMood.Searching
    ActivityKind.Asking -> MochiMood.Alert
    null, ActivityKind.Idle, ActivityKind.Waiting -> MochiMood.Listening
    else -> MochiMood.Working
}

/** Left of the collapsed island: Mochi in Claude's colour, with the face of what it is doing. */
@Composable
fun AgentCollapsedGlyph(notification: IslandNotification?) {
    when (notification?.mode) {
        com.agupta07505.smartisland.model.IslandMode.Approval ->
            MochiGlyph(24.dp, ReminderAmber, MochiMood.Alert, outfit = rememberMochiOutfit(MochiMood.Alert))
        com.agupta07505.smartisland.model.IslandMode.Question ->
            MochiGlyph(24.dp, QuestionBlue, MochiMood.Alert, outfit = rememberMochiOutfit(MochiMood.Alert))
        else -> {
            val kind = kindOf(notification)
            val mood = moodOf(kind)
            // In the colour of whoever is working: Claude Code, Antigravity, OpenCode…
            val tint = com.agupta07505.smartisland.coucou.agents.AgentKinds.byName(notification?.appName).color
            MochiGlyph(24.dp, tint, mood, outfit = rememberMochiOutfit(mood))
        }
    }
}

/**
 * Right of the collapsed island: a lock or a question mark when it asks; otherwise three dots taking
 * turns in the colour of what it is doing (violet thinking, indigo searching, blue working).
 */
@Composable
fun AgentCollapsedRight(notification: IslandNotification?) {
    when (notification?.mode) {
        com.agupta07505.smartisland.model.IslandMode.Approval ->
            Icon(Icons.Rounded.Lock, "Permission", tint = ReminderAmber, modifier = Modifier.size(15.dp))
        com.agupta07505.smartisland.model.IslandMode.Question ->
            Icon(Icons.Rounded.QuestionMark, "Question", tint = QuestionBlue, modifier = Modifier.size(16.dp))
        // Only the dots, in the colour of what it is doing: they sit at the right edge, clear of the
        // camera, which is exactly where a word would have gone under it. The sentence is on the open card.
        // Finished: a tick instead, for the moment it lasts.
        else -> if (kindOf(notification) == ActivityKind.Finished) {
            Icon(Icons.Rounded.Check, stringResource(R.string.coucou_act_done), tint = DoneGreen, modifier = Modifier.size(16.dp))
        } else {
            WorkingDots(colorOf(kindOf(notification)))
        }
    }
}

@Composable
private fun WorkingDots(color: Color) {
    val t = rememberInfiniteTransition(label = "dots")
    Row(horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
        for (i in 0..2) {
            val a by t.animateFloat(
                0.25f, 0.25f,
                infiniteRepeatable(
                    keyframes {
                        durationMillis = 1000
                        0.25f at 0
                        1f at 200 + i * 150
                        0.25f at 500 + i * 150
                    },
                    RepeatMode.Restart,
                ),
                label = "dot$i",
            )
            Box(Modifier.size(5.dp).graphicsLayer { alpha = a }.clip(CircleShape).background(color))
        }
    }
}

/**
 * The open island while Claude Code works. One line per session: its project and how long it has been at
 * it on top, what it is doing right now (Editing Mochi.kt, Running cargo test…) under, in the colour of
 * that kind of work. A session that needs the person says so first.
 */
@Composable
fun AgentExpanded(bottomPadding: Dp) {
    val ui by CoucouRuntime.agentUi.collectAsState()
    val context = LocalContext.current
    // One tick a second while the card is open, so the times move; closed, nothing runs.
    val now by produceState(System.currentTimeMillis()) {
        while (true) {
            value = System.currentTimeMillis()
            delay(1_000)
        }
    }
    val shown = ui.sessions.filter { it.state != SessionState.Idle }.ifEmpty { ui.sessions }
    val rows = shown.take(3)
    val head = rows.firstOrNull()?.let { AgentActivity.of(it.tool, it.detail, it.state).kind }
    // "Claude Code" when they are all Claude Code; "Agentes", with each one's name on its line, when they are mixed.
    val title = com.agupta07505.smartisland.coucou.agents.AgentKinds.title(shown.map { it.agent })
    val mixed = shown.map { it.agent }.toSet().size > 1
    Column(
        Modifier.fillMaxSize().padding(horizontal = 18.dp).padding(top = 6.dp, bottom = bottomPadding),
        verticalArrangement = Arrangement.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            val headKind = com.agupta07505.smartisland.coucou.agents.AgentKinds.of(rows.firstOrNull()?.agent ?: "")
            MochiGlyph(30.dp, headKind.color, moodOf(head), outfit = rememberMochiOutfit(moodOf(head)))
            Spacer(Modifier.width(10.dp))
            Text(title, color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            val working = shown.count { it.state == SessionState.Working }
            val asking = shown.count { it.state == SessionState.NeedsYou }
            val summary = listOfNotNull(
                if (working > 0) context.getString(R.string.coucou_agent_working, working) else null,
                if (asking > 0) context.getString(R.string.coucou_agent_asking, asking) else null,
            ).joinToString(" · ")
            if (summary.isNotEmpty()) Text(summary, color = Color.White.copy(alpha = 0.62f), fontSize = 11.sp)
        }
        Spacer(Modifier.size(6.dp))
        if (rows.isEmpty()) Text(stringResource(R.string.coucou_agent_nothing), color = Color.White.copy(alpha = 0.62f), fontSize = 13.sp)
        for (s in rows) {
            val act = if (s.state == SessionState.Idle && s.finished) {
                AgentActivity.finished(s.ranMs)
            } else {
                AgentActivity.of(s.tool, s.detail, s.state)
            }
            val color = colorOf(act.kind)
            Row(verticalAlignment = Alignment.Top, modifier = Modifier.padding(vertical = 3.dp)) {
                Box(Modifier.padding(top = 5.dp).size(7.dp).clip(CircleShape).background(if (s.state == SessionState.Idle && !s.finished) Color.White.copy(alpha = 0.3f) else color))
                Spacer(Modifier.width(8.dp))
                Column(Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        val projectName = s.project.ifEmpty { stringResource(R.string.coucou_agent_session) }
                        Text(
                            if (mixed) com.agupta07505.smartisland.coucou.agents.AgentKinds.of(s.agent).name + " · " + projectName else projectName,
                            color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.Medium,
                            maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
                        )
                        if (s.state != SessionState.Idle) {
                            Spacer(Modifier.width(8.dp))
                            val meta = buildString {
                                append(AgentActivity.elapsed(now - s.since))
                                if (s.state == SessionState.Working && s.steps > 0) {
                                    append(" · ")
                                    append(context.resources.getQuantityString(R.plurals.coucou_agent_steps, s.steps, s.steps))
                                }
                            }
                            Text(meta, color = Color.White.copy(alpha = 0.62f), fontSize = 11.sp, maxLines = 1)
                        }
                    }
                    Text(
                        activityLine(context, act),
                        color = if (s.state == SessionState.Idle && !s.finished) Color.White.copy(alpha = 0.62f) else color,
                        fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        if (shown.size > rows.size) {
            Text(
                stringResource(R.string.coucou_agent_more, shown.size - rows.size),
                color = Color.White.copy(alpha = 0.62f), fontSize = 11.sp, modifier = Modifier.padding(start = 15.dp, top = 2.dp),
            )
        }
    }
}

private fun pendingOf(notification: IslandNotification?, ui: com.agupta07505.smartisland.coucou.agents.AgentUi): Pending? {
    val id = notification?.key?.let(IslandAgents::idOf) ?: return null
    return ui.pending.firstOrNull { it.id == id }
}

/** What has to be seen of a command before it can be allowed from the phone. */
object ApprovalReview {
    /** About three lines of 12 sp monospace on the island's width. */
    const val FITS_CHARS = 100

    /** The laptop sends at most this many characters of a command and ends a longer one with "…". */
    private const val LAPTOP_LIMIT = 300

    /** The laptop cut this command: its tail cannot be reviewed here, so it is not allowed from here. */
    fun shortenedByLaptop(detail: String): Boolean = detail.length >= LAPTOP_LIMIT - 10 && detail.endsWith("…")
}

/** A permission request: what Claude Code wants to do, and Allow (with the fingerprint) or Deny. */
@Composable
fun ApprovalExpanded(notification: IslandNotification?, bottomPadding: Dp, onCollapse: () -> Unit) {
    val context = LocalContext.current
    val ui by CoucouRuntime.agentUi.collectAsState()
    val p = pendingOf(notification, ui)
    // Without a fingerprint or a screen lock to ask, a second tap is the confirmation.
    var armed by remember(notification?.key) { mutableStateOf(false) }
    val secure = remember { ConfirmActivity.available(context) }
    var message by remember(notification?.key) { mutableStateOf<String?>(null) }

    Column(
        Modifier.fillMaxSize().padding(horizontal = 18.dp).padding(top = 6.dp, bottom = bottomPadding),
        verticalArrangement = Arrangement.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            MochiGlyph(30.dp, ReminderAmber, MochiMood.Alert)
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    "${p?.project.orEmpty().ifEmpty { "Claude Code" }} wants to use ${p?.tool ?: notification?.title.orEmpty()}",
                    color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Spacer(Modifier.size(6.dp))
        // What is being allowed must be seen before it is allowed: a command that does not fit in three
        // lines has to be opened (tap it) first, and one the laptop itself had to shorten is not allowed
        // from here at all.
        val full = p?.detail ?: notification?.text.orEmpty()
        var read by remember(notification?.key) { mutableStateOf(false) }
        // What Compose really had to cut, not a guess from the length (the font scale and the island's
        // width change how much fits): while closed, a cut means "open it first"; opened, a cut that
        // remains means it cannot be reviewed here at all.
        var cut by remember(notification?.key, read) { mutableStateOf(false) }
        val tooLongToFit = cut || full.length > ApprovalReview.FITS_CHARS || full.count { it == '\n' } >= 3
        val shortenedByLaptop = ApprovalReview.shortenedByLaptop(full) || (read && cut)
        Text(
            full,
            onTextLayout = { cut = it.hasVisualOverflow },
            color = Color.White.copy(alpha = 0.78f), fontSize = 12.sp, fontFamily = FontFamily.Monospace,
            maxLines = if (read) 12 else 3, overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(8.dp))
                .background(Color.White.copy(alpha = 0.07f))
                .then(if (tooLongToFit) Modifier.clickable { read = true } else Modifier)
                .then(if (read) Modifier.heightIn(max = 110.dp).verticalScroll(rememberScrollState()) else Modifier)
                .padding(8.dp),
        )
        if (shortenedByLaptop) {
            Text("Too long to check here: answer it in the terminal", color = ReminderAmber, fontSize = 11.sp, modifier = Modifier.padding(top = 3.dp))
        } else if (tooLongToFit && !read) {
            Text("Tap the command to read all of it before allowing", color = ReminderAmber, fontSize = 11.sp, modifier = Modifier.padding(top = 3.dp))
        }
        message?.let { Text(it, color = Red, fontSize = 11.sp, modifier = Modifier.padding(top = 3.dp)) }
        Spacer(Modifier.size(8.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Pill("Deny", Modifier.weight(1f), Red) {
                p?.let { if (!CoucouRuntime.sendToLaptop(RemoteProtocol.answer(it.id, "deny"))) message = "No link to the laptop" else onCollapse() }
            }
            Pill("Terminal", Modifier.weight(1f), Color.White.copy(alpha = 0.6f)) {
                p?.let { if (!CoucouRuntime.sendToLaptop(RemoteProtocol.decline(it.id))) message = "No link to the laptop" else onCollapse() }
            }
            Pill(if (armed) "Tap to allow" else "Allow", Modifier.weight(1f), Green) {
                val req = p ?: return@Pill
                if (shortenedByLaptop) {
                    message = "Too long to check here: use Terminal"
                    return@Pill
                }
                if (tooLongToFit && !read) {
                    message = "Read the whole command first (tap it)"
                    return@Pill
                }
                val send = {
                    if (!CoucouRuntime.sendToLaptop(RemoteProtocol.answer(req.id, "allow"))) message = "No link to the laptop" else onCollapse()
                }
                when {
                    // The whole command (the laptop sends at most 300 characters), not a cut of it.
                    secure -> ConfirmActivity.ask(context, "Allow on the laptop?", "${req.tool}: ${req.detail}") { ok -> if (ok) send() else message = "Not confirmed" }
                    armed -> send()
                    else -> armed = true
                }
            }
        }
    }
}

/** An AskUserQuestion: the options as buttons; a multiple-choice question is ticked and then sent. */
@Composable
fun QuestionExpanded(notification: IslandNotification?, bottomPadding: Dp, onCollapse: () -> Unit) {
    val ui by CoucouRuntime.agentUi.collectAsState()
    val p = pendingOf(notification, ui)
    val questions = p?.questions.orEmpty()
    // One question at a time, as on the PC.
    var index by remember(notification?.key) { mutableStateOf(0) }
    val answers = remember(notification?.key) { mutableStateOf<Map<String, List<String>>>(emptyMap()) }
    var picked by remember(notification?.key, index) { mutableStateOf<List<String>>(emptyList()) }
    var message by remember(notification?.key) { mutableStateOf<String?>(null) }
    val q = questions.getOrNull(index)
    val context = LocalContext.current

    fun finish(all: Map<String, List<String>>) {
        val req = p ?: return
        val multi = questions.filter { it.multi }.map { it.question }.toSet()
        if (!CoucouRuntime.sendToLaptop(RemoteProtocol.answerQuestion(req.id, all, multi))) message = "No link to the laptop" else onCollapse()
    }

    fun next(chosen: List<String>) {
        val cur = q ?: return
        touchIsland(context)
        val all = answers.value + (cur.question to chosen)
        answers.value = all
        if (index + 1 < questions.size) index += 1 else finish(all)
    }

    Column(
        Modifier.fillMaxWidth().padding(horizontal = 18.dp).padding(top = 8.dp, bottom = bottomPadding.coerceAtLeast(10.dp)),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            MochiGlyph(28.dp, QuestionBlue, MochiMood.Alert)
            Spacer(Modifier.width(10.dp))
            Text(
                q?.question ?: notification?.title.orEmpty(),
                color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 3, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (questions.size > 1) Text("${index + 1}/${questions.size}", color = Color.White.copy(alpha = 0.5f), fontSize = 11.sp)
        }
        Spacer(Modifier.size(6.dp))
        Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(5.dp)) {
            for (o in q?.options.orEmpty().take(6)) {
                val on = o.label in picked
                Pill(
                    o.label + if (o.description.isNotEmpty()) "  ·  ${o.description}" else "",
                    Modifier.fillMaxWidth(),
                    if (on) Green else QuestionBlue,
                ) {
                    touchIsland(context)
                    if (q?.multi == true) picked = if (on) picked - o.label else picked + o.label else next(listOf(o.label))
                }
            }
        }
        Spacer(Modifier.size(6.dp))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Pill("Terminal", Modifier.weight(1f), Color.White.copy(alpha = 0.6f)) {
                p?.let { if (!CoucouRuntime.sendToLaptop(RemoteProtocol.decline(it.id))) message = "No link to the laptop" else onCollapse() }
            }
            if (q?.multi == true) Pill("Send", Modifier.weight(1f), Green) { if (picked.isNotEmpty()) next(picked) else message = "Pick at least one" }
        }
        message?.let { Text(it, color = Red, fontSize = 11.sp, modifier = Modifier.padding(top = 3.dp)) }
    }
}

@Composable
private fun Pill(label: String, modifier: Modifier, color: Color, onClick: () -> Unit) {
    Box(
        modifier = modifier
            .clip(RoundedCornerShape(50))
            .background(color.copy(alpha = 0.16f))
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = color, fontSize = 12.sp, fontWeight = FontWeight.Medium, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}
