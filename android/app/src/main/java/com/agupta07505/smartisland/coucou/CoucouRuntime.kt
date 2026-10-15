/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.content.Context
import com.agupta07505.smartisland.coucou.agents.AgentRemote
import com.agupta07505.smartisland.coucou.agents.AgentUi
import com.agupta07505.smartisland.coucou.agents.SessionState
import com.agupta07505.smartisland.coucou.phone.JinxSync
import com.agupta07505.smartisland.coucou.phone.PhoneSettings
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.followClaude
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.followJinx
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.jinxSync
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.showDesk
import com.agupta07505.smartisland.coucou.presence.DeviceActivity
import com.agupta07505.smartisland.coucou.presence.MochiPresence
import com.agupta07505.smartisland.coucou.ui.DeskState
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.distinctUntilChanged
import com.agupta07505.smartisland.coucou.phone.SshPendientes
import com.agupta07505.smartisland.coucou.phone.SyncOutcome
import com.agupta07505.smartisland.di.SmartIslandRepositories
import com.agupta07505.smartisland.model.AGENT_NEEDS_YOU
import com.agupta07505.smartisland.model.IslandMode
import com.agupta07505.smartisland.model.IslandNotification
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/**
 * What runs for as long as the island does: the sync with Jinx's pendientes and the link to Claude
 * Code on the laptop, each only if it was turned on and paired. Started by the overlay service (and
 * by the screens, which only need it to be there); safe to start twice.
 */
object CoucouRuntime {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var agents: AgentRemote? = null
    private var agentsWatch: Job? = null
    private var jinxLoop: Job? = null
    private var deskJob: Job? = null
    private var deskWatch: Job? = null
    private val kicks = Channel<Unit>(Channel.CONFLATED)

    private val _jinx = MutableStateFlow(SyncOutcome(emptyList(), emptyList()))
    val jinx: StateFlow<SyncOutcome> = _jinx.asStateFlow()

    private val _agentUi = MutableStateFlow(AgentUi())
    val agentUi: StateFlow<AgentUi> = _agentUi.asStateFlow()

    /** Sends a line to the laptop (an answer to a request on screen). False if there is no link. */
    fun sendToLaptop(line: String): Boolean = agents?.send(line) ?: false

    /** Starts (or stops) each part according to the switches. Called at start and when a switch changes. */
    @Synchronized
    fun apply(context: Context) {
        val app = context.applicationContext
        ReminderScheduler.rearm(app)
        CoucouTasks.refresh(app)
        // Who is at which device: runs whatever else is switched on or off.
        DeviceActivity.start(app, scope)

        // Mochi, always in the island with the tasks
        if (deskJob == null) {
            deskJob = scope.launch {
                combine(CoucouTasks.state, _jinx) { tasks, j -> tasks to j.fromJinx.count { it.source != "teams" } }
                    .distinctUntilChanged()
                    .collect { (tasks, theirs) -> IslandDesk.publish(app, tasks, theirs) }
            }
        } else {
            IslandDesk.publish(app, CoucouTasks.state.value, _jinx.value.fromJinx.count { it.source != "teams" })
        }

        // ...and if anything takes him off the island, he is put back.
        if (deskWatch == null) {
            deskWatch = scope.launch {
                SmartIslandRepositories.notificationRepository(app).notifications
                    .map { list -> list.any { it.key == IslandDesk.KEY } }
                    .distinctUntilChanged()
                    .collect { present ->
                        if (!present && app.showDesk) IslandDesk.publish(app, CoucouTasks.state.value, _jinx.value.fromJinx.count { it.source != "teams" })
                    }
            }
        }

        // Claude Code on the laptop
        if (app.followClaude && agents == null) {
            val a = AgentRemote(app, scope)
            agents = a
            a.start()
            agentsWatch = scope.launch {
                var posted = emptySet<String>()
                a.ui.collect { ui ->
                    _agentUi.value = ui
                    posted = IslandAgents.publish(app, ui, posted)
                }
            }
        } else if (!app.followClaude && agents != null) {
            agents?.stop()
            agentsWatch?.cancel()
            IslandAgents.clear(app)
            agents = null
            _agentUi.value = AgentUi()
        }

        // Jinx's texts on this phone, through presence (no voice: text only)
        if (app.followJinx) {
            MochiPresence.start(app, scope)
        } else {
            MochiPresence.stop(app)
        }

        // Jinx's pendientes
        if (app.jinxSync && jinxLoop?.isActive != true) {
            jinxLoop = scope.launch {
                while (true) {
                    runSync(app)
                    // Every few minutes, or at once when something asks (a task changed, the screen opened).
                    withTimeoutOrNull(5 * 60_000L) { kicks.receive() }
                    delay(400)
                }
            }
        } else if (!app.jinxSync && jinxLoop != null) {
            jinxLoop?.cancel()
            jinxLoop = null
            _jinx.value = SyncOutcome(TaskStore.of(app).load(), emptyList())
        }
    }

    /** Closes or dismisses one of Jinx's own pendientes, then looks at her list again. */
    fun resolveJinx(context: Context, id: String, action: String) {
        val app = context.applicationContext
        scope.launch {
            runCatching { SshPendientes(app, PhoneSettings.profile(app, PhoneSettings.SERVER)).resolve(id, action) }
            runSync(app)
        }
    }

    /** A task changed, or the screen opened: sync soon. */
    fun kick() {
        kicks.trySend(Unit)
    }

    /** A task changed here: the laptop hears of it as soon as the link is there (now, or when it comes back). */
    fun kickLaptop() {
        agents?.requestTaskSync()
    }

    private fun runSync(app: Context) {
        val profile = PhoneSettings.profile(app, PhoneSettings.SERVER)
        if (!profile.paired) {
            _jinx.value = SyncOutcome(TaskStore.of(app).load(), emptyList(), "Pair the server in Coucou's phone settings")
            return
        }
        _jinx.value = JinxSync.run(SshPendientes(app, profile), TaskStore.of(app), System.currentTimeMillis() / 1000, DismissQueue.of(app))
        CoucouTasks.refresh(app)
    }
}

/** Puts what Claude Code is doing, and what it asks, on the island. */
object IslandAgents {
    private const val KEY_AGENT = "coucou_agent"

    /** Was the Claude Code card already asking for the person (so it only opens by itself once). */
    @Volatile private var attentionShown = false
    fun approvalKey(id: String) = "coucou_approval_$id"
    fun questionKey(id: String) = "coucou_question_$id"
    fun idOf(key: String) = key.substringAfter("coucou_approval_").substringAfter("coucou_question_")

    /** Returns the keys now on the island, so the next call can take off what is gone. */
    fun publish(context: Context, ui: AgentUi, before: Set<String>): Set<String> {
        val repo = SmartIslandRepositories.notificationRepository(context)
        val now = System.currentTimeMillis()
        val keys = mutableSetOf<String>()

        for (p in ui.pending) {
            val key = if (p.isQuestion) questionKey(p.id) else approvalKey(p.id)
            keys += key
            if (key !in before) {
                repo.postNotification(
                    IslandNotification(
                        key = key,
                        packageName = context.packageName,
                        appName = com.agupta07505.smartisland.coucou.agents.AgentKinds.of(p.agent).name,
                        title = if (p.isQuestion) (p.questions.firstOrNull()?.question ?: "Claude asks") else p.tool,
                        text = if (p.isQuestion) p.project else p.detail,
                        timeMillis = now,
                        mode = if (p.isQuestion) IslandMode.Question else IslandMode.Approval,
                    ),
                    autoExpand = true,
                )
            }
        }

        // One line for the sessions: who is busy, and with what. Merely working is the quietest thing
        // on the island (it shows only when nothing else does). A session that needs the person — and
        // whose request cannot be answered from here, like a background job's permission, which
        // reaches the phone as a Notification only — is raised to just under a real question.
        val busy = ui.sessions.filter { it.state != SessionState.Idle }
        val asked = ui.pending.map { it.session }.toSet()
        val needing = busy.filter { it.state == SessionState.NeedsYou && it.session !in asked }
        // A run that has just ended is told as done for a few seconds (the model clears the flag by itself).
        val justDone = ui.sessions.filter { it.state == SessionState.Idle && it.finished }
        if (ui.status == AgentUi.Status.Connected && (busy.isNotEmpty() || justDone.isNotEmpty()) && ui.pending.isEmpty()) {
            val s = needing.firstOrNull() ?: busy.firstOrNull() ?: justDone.first()
            val isDone = busy.isEmpty()
            val attention = needing.isNotEmpty()
            // Starting to ask: post it as new, so it opens by itself (an update would only change the card).
            if (attention && !attentionShown) repo.removeNotification(KEY_AGENT)
            keys += KEY_AGENT
            repo.postNotification(
                IslandNotification(
                    key = KEY_AGENT,
                    packageName = context.packageName,
                    // Claude Code, Antigravity, OpenCode…: the collapsed pill wears that agent's colour.
                    appName = com.agupta07505.smartisland.coucou.agents.AgentKinds.of(s.agent).name,
                    title = (s.project.ifEmpty { com.agupta07505.smartisland.coucou.agents.AgentKinds.of(s.agent).name }) +
                        if (busy.size > 1) "  +${busy.size - 1}" else "",
                    // What it is doing, in words ("Editing Mochi.kt"); its kind rides in the category so the
                    // collapsed pill can wear the matching face and colour.
                    text = com.agupta07505.smartisland.coucou.ui.activityLine(
                        context,
                        if (isDone) {
                            com.agupta07505.smartisland.coucou.agents.AgentActivity.finished(s.ranMs)
                        } else {
                            com.agupta07505.smartisland.coucou.agents.AgentActivity.of(s.tool, s.detail, s.state)
                        },
                    ),
                    timeMillis = s.updatedAt,
                    mode = IslandMode.Agent,
                    category = if (attention) AGENT_NEEDS_YOU
                    else com.agupta07505.smartisland.coucou.ui.ACTIVITY_CATEGORY_PREFIX + (
                        if (isDone) com.agupta07505.smartisland.coucou.agents.ActivityKind.Finished.name
                        else com.agupta07505.smartisland.coucou.agents.AgentActivity.of(s.tool, s.detail, s.state).kind.name
                    ),
                ),
                // Opens by itself only when it starts asking for the person, not on every beat.
                autoExpand = attention && !attentionShown,
            )
            attentionShown = attention
        } else {
            attentionShown = false
        }

        for (gone in before - keys) repo.removeNotification(gone)
        return keys
    }

    fun clear(context: Context) {
        val repo = SmartIslandRepositories.notificationRepository(context)
        repo.notifications.value.filter { it.key == KEY_AGENT || it.key.startsWith("coucou_approval_") || it.key.startsWith("coucou_question_") }
            .forEach { repo.removeNotification(it.key) }
    }
}

/** Mochi in the island: one notification that is always there, and says how many things are open. */
object IslandDesk {
    const val KEY = "coucou_desk"

    fun publish(context: Context, tasks: List<CoucouTask>, jinxOpen: Int) {
        val repo = SmartIslandRepositories.notificationRepository(context)
        if (!context.showDesk) {
            repo.removeNotification(KEY)
            return
        }
        val now = System.currentTimeMillis() / 1000
        val open = tasks.filter { !it.done }
        val count = open.size + (if (context.jinxSyncOn()) jinxOpen else 0)
        val overdue = open.any { it.remindAt != null && it.remindAt <= now }
        repo.postNotification(
            IslandNotification(
                key = KEY,
                packageName = context.packageName,
                appName = "Coucou",
                title = if (count == 0) "All clear" else "$count to do",
                text = "Mochi",
                // The oldest of all: whatever else happens takes the front.
                timeMillis = 1L,
                mode = IslandMode.Desk,
                progress = count,
                category = when {
                    overdue -> DeskState.OVERDUE
                    count == 0 -> DeskState.CLEAR
                    else -> null
                },
            ),
            autoExpand = false,
        )
    }

    private fun Context.jinxSyncOn(): Boolean = with(PhoneSettings) { this@jinxSyncOn.jinxSync }
}
