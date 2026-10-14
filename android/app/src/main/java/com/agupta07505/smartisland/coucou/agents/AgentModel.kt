/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.agents

import com.agupta07505.smartisland.coucou.presence.Arbiter
import com.agupta07505.smartisland.coucou.presence.Beat
import org.json.JSONArray
import org.json.JSONObject

/** One question of an AskUserQuestion. */
data class QuestionItem(val question: String, val header: String, val multi: Boolean, val options: List<Option>) {
    data class Option(val label: String, val description: String)
}

/** A request that waits for a human: a permission, or a question. */
data class Pending(
    val id: String,
    val isQuestion: Boolean,
    val session: String,
    val project: String,
    val tool: String,
    val detail: String,
    val questions: List<QuestionItem>,
    val timeoutS: Int,
    val receivedAt: Long,
)

enum class SessionState { Working, Idle, NeedsYou }

data class SessionInfo(
    val session: String,
    val project: String,
    val state: SessionState,
    val tool: String,
    val detail: String,
    val updatedAt: Long,
    /** When this state began: for a run, its first moment (tools do not restart it). */
    val since: Long = updatedAt,
    /** How many tools this run has started: a sense of how far along it is. */
    val steps: Int = 0,
    /** This is a run that just ended well (a Stop after work): shown as done for a few seconds. */
    val finished: Boolean = false,
    /** How long that run lasted, for "Done · 2 min 14 s". */
    val ranMs: Long = 0,
)

/** What the laptop's Coucou says (see remote.rs). */
sealed class RemoteMsg {
    data class Hello(val pending: List<Pending>) : RemoteMsg()
    data class Event(val event: String, val session: String, val project: String, val tool: String, val detail: String, val message: String) : RemoteMsg()
    data class Request(val pending: Pending) : RemoteMsg()
    data class Resolved(val id: String, val decision: String) : RemoteMsg()
    data class Ack(val ok: Boolean, val id: String?, val why: String?) : RemoteMsg()
    /** The laptop's own list of tasks, merged with the phone's (see remote.rs `tasks_sync`): a JSON array. */
    data class Tasks(val json: String) : RemoteMsg()
    /** The laptop's own beat: how recently it was used, whether it reaches the server, what it hears and sees. */
    data class Peer(val beat: Beat, val music: Boolean, val news: Boolean, val limit: Boolean = false) : RemoteMsg()
    /** The laptop says another phone left the link. */
    data class PeerGone(val device: String) : RemoteMsg()
    data object Pong : RemoteMsg()
    data object Disabled : RemoteMsg()
    data object Offline : RemoteMsg()
}

object RemoteProtocol {
    /** The phone's whole list, tombstones included, for the laptop to merge with its own. */
    fun tasksSync(tasksJson: String): String =
        JSONObject().put("op", "tasks_sync").put("tasks", JSONArray(tasksJson)).toString()

    fun parse(line: String, now: Long): RemoteMsg? {
        val o = runCatching { JSONObject(line) }.getOrNull() ?: return null
        fun s(k: String) = o.optString(k, "").takeIf { it != "null" } ?: ""
        return when (o.optString("t")) {
            "hello" -> {
                val arr = o.optJSONArray("pending")
                RemoteMsg.Hello((0 until (arr?.length() ?: 0)).mapNotNull { pendingOf(arr!!.getJSONObject(it), now) })
            }
            "event" -> RemoteMsg.Event(s("event"), s("session"), s("project"), s("tool"), s("detail"), s("message"))
            "approval", "question" -> pendingOf(o, now)?.let { RemoteMsg.Request(it) }
            "resolved" -> RemoteMsg.Resolved(s("request_id"), s("decision"))
            "ack" -> RemoteMsg.Ack(o.optBoolean("ok"), o.optString("request_id").takeIf { it.isNotEmpty() }, o.optString("why").takeIf { it.isNotEmpty() })
            "tasks" -> o.optJSONArray("tasks")?.let { RemoteMsg.Tasks(it.toString()) }
            "peer_gone" -> o.optString("device").takeIf { it in Arbiter.KNOWN }?.let { RemoteMsg.PeerGone(it) }
            "peer" -> {
                // Only the laptop speaks on this link: its own beat, and the other phone's relayed by
                // it. Only in terms of ids we know (DeviceActivity ignores this phone's own).
                val device = o.optString("device")
                if (device !in Arbiter.KNOWN) null
                else RemoteMsg.Peer(
                    Beat(
                        device = device,
                        idleS = o.optLong("idle", 0L).coerceIn(0L, MAX_IDLE_S),
                        screenOn = o.optBoolean("screen_on", true),
                        serverOk = o.optBoolean("server_ok", false),
                        serverActive = o.optString("server_active").takeIf { it in Arbiter.KNOWN },
                    ),
                    music = o.optBoolean("music", false),
                    news = o.optBoolean("news", false),
                    // Claude Code is rate-limited on the laptop: he is tired here too.
                    limit = o.optBoolean("limit", false),
                )
            }
            "pong" -> RemoteMsg.Pong
            "disabled" -> RemoteMsg.Disabled
            "offline" -> RemoteMsg.Offline
            else -> null
        }
    }

    private fun pendingOf(o: JSONObject, now: Long): Pending? {
        val id = o.optString("request_id").takeIf { it.isNotEmpty() } ?: return null
        val isQuestion = o.optString("t") == "question"
        val qs = o.optJSONArray("questions")
        val questions = (0 until (qs?.length() ?: 0)).map { i ->
            val q = qs!!.getJSONObject(i)
            val os = q.optJSONArray("options")
            QuestionItem(
                q.optString("question"), q.optString("header"), q.optBoolean("multi"),
                (0 until (os?.length() ?: 0)).map { j -> os!!.getJSONObject(j).let { QuestionItem.Option(it.optString("label"), it.optString("description")) } },
            )
        }
        return Pending(
            id, isQuestion, o.optString("session"), o.optString("project"), o.optString("tool"), o.optString("detail"),
            questions, o.optInt("timeout_s", 108), now,
        )
    }

    fun answer(id: String, decision: String) = JSONObject().put("op", "answer").put("request_id", id).put("decision", decision).toString()
    fun decline(id: String) = JSONObject().put("op", "decline").put("request_id", id).toString()
    fun ping() = JSONObject().put("op", "ping").toString()

    /** A week: past that an idle value means nothing, and an absurd one must not outlast a real one. */
    const val MAX_IDLE_S = 7L * 24 * 3600

    /** This phone's beat for the laptop: state only, nothing it could act on. */
    fun peer(beat: Beat, music: Boolean, news: Boolean): String =
        JSONObject()
            .put("op", "peer")
            .put("device", beat.device)
            .put("idle", beat.idleS.coerceIn(0L, MAX_IDLE_S))
            .put("screen_on", beat.screenOn)
            .put("server_ok", beat.serverOk)
            .put("server_active", beat.serverActive ?: JSONObject.NULL)
            .put("music", music)
            .put("news", news)
            .toString()

    /** `answers` maps each question's text to the chosen label, or to a list of labels for a multi-select one. */
    fun answerQuestion(id: String, answers: Map<String, List<String>>, multi: Set<String>): String {
        val a = JSONObject()
        for ((q, labels) in answers) {
            if (q in multi) a.put(q, org.json.JSONArray(labels)) else a.put(q, labels.firstOrNull() ?: "")
        }
        return JSONObject().put("op", "answer_question").put("request_id", id).put("answers", a).toString()
    }
}

/** What the phone knows of Claude Code, built from what the laptop says. */
class AgentModel {
    val sessions = LinkedHashMap<String, SessionInfo>()
    val pending = LinkedHashMap<String, Pending>()

    /** When a tool of Claude Code last failed (0: never): Mochi winces for a moment, as on the laptop. */
    @Volatile var lastFailureAt = 0L

    /** How long without news before a session that was working is shown as idle. */
    companion object {
        const val STALE_MS = 90_000L
        const val FORGET_MS = 30 * 60_000L

        /**
         * How long a "needs you" that no request backs (a Notification of Claude Code, which cannot be
         * answered from here) is believed without news: the person answered it in the terminal, or
         * left, and no hook says so. Without this a session stayed "asking" for ever.
         */
        const val ATTENTION_MS = 15 * 60_000L

        /** Slack after a request's own time limit before the phone drops it (the laptop's clock rules). */
        const val PENDING_GRACE_S = 20

        /** How long a run that ended is shown as done before it is just idle. */
        const val FINISHED_MS = 7_000L
    }

    fun apply(msg: RemoteMsg, now: Long) {
        when (msg) {
            is RemoteMsg.Hello -> {
                pending.clear()
                msg.pending.forEach { pending[it.id] = it }
            }
            is RemoteMsg.Request -> pending[msg.pending.id] = msg.pending
            is RemoteMsg.Resolved -> pending.remove(msg.id)
            is RemoteMsg.Event -> event(msg, now)
            else -> Unit
        }
        // (A request still waiting makes its session "needs you" in [view], without being written into the
        // session: a request answered at the terminal never says so, and must not outlive the run.)
    }

    private fun event(e: RemoteMsg.Event, now: Long) {
        if (e.session.isEmpty()) return
        if (e.event == "PostToolUseFailure") lastFailureAt = now
        // What a session says next proves its earlier request is over: the tool ran (or failed), the turn
        // ended, the session ended, or a new prompt came. Without this a request answered at the terminal
        // kept the session at "needs you" until its own time limit and a quarter hour more.
        val endsRequests = e.event == "Stop" || e.event == "SessionEnd" || e.event == "UserPromptSubmit"
        val toolRan = (e.event == "PostToolUse" || e.event == "PostToolUseFailure") && e.tool.isNotEmpty()
        pending.entries.removeAll { (_, p) -> p.session == e.session && (endsRequests || (toolRan && p.tool == e.tool)) }
        val old = sessions[e.session]
        val project = e.project.ifEmpty { old?.project ?: "" }
        when (e.event) {
            "SessionEnd" -> sessions.remove(e.session)
            // A subagent finishing is not the session finishing: it goes on until its own Stop.
            "SubagentStop" ->
                sessions[e.session] = (old ?: SessionInfo(e.session, project, SessionState.Idle, "", "", now)).copy(updatedAt = now)
            // The turn ended: after work (or after asking) it is "done" for a moment, then idle.
            "Stop" -> {
                val didWork = old != null && old.state != SessionState.Idle
                sessions[e.session] = SessionInfo(
                    e.session, project, SessionState.Idle, old?.tool ?: "", old?.detail ?: "", now,
                    since = now, steps = old?.steps ?: 0,
                    finished = didWork, ranMs = if (didWork) now - old!!.since else 0L,
                )
            }
            "SessionStart" ->
                sessions[e.session] = SessionInfo(e.session, project, SessionState.Idle, old?.tool ?: "", old?.detail ?: "", now)
            // Claude Code says "waiting for your input" when it has been idle a while: that is not
            // asking for anything. A permission it cannot raise as a request (a background job's)
            // arrives as a Notification only, and is worth the person's attention.
            "Notification" -> {
                val m = e.message
                val idlePrompt = (m.contains("waiting for your input", true) || m.contains("esperando", true)) &&
                    !(m.contains("permission", true) || m.contains("permiso", true))
                sessions[e.session] = SessionInfo(
                    e.session, project,
                    if (idlePrompt) SessionState.Idle else SessionState.NeedsYou,
                    "", m.ifEmpty { "needs you" }, now,
                )
            }
            // A new prompt: it is thinking until its first tool (no tool, no detail), as on the laptop.
            "UserPromptSubmit" ->
                sessions[e.session] = SessionInfo(e.session, project, SessionState.Working, "", "", now)
            "PreToolUse", "PostToolUse", "PostToolUseFailure", "SubagentStart" -> {
                val running = old?.state == SessionState.Working
                val started = e.event == "PreToolUse" || e.event == "SubagentStart"
                sessions[e.session] = SessionInfo(
                    e.session, project, SessionState.Working,
                    e.tool.ifEmpty { old?.tool ?: "" }, if (e.tool.isNotEmpty()) e.detail else (old?.detail ?: ""), now,
                    since = if (running) old!!.since else now,
                    steps = (if (running) old!!.steps else 0) + (if (started) 1 else 0),
                )
            }
            else -> sessions[e.session] = (old ?: SessionInfo(e.session, project, SessionState.Idle, "", "", now)).copy(updatedAt = now)
        }
    }

    /** Sessions as they should look now: a "working" one that has been quiet for long is idle; old ones go. */
    fun view(now: Long): List<SessionInfo> {
        // A request the laptop gave up on (its own limit plus a little) is not waiting any more, even if
        // the "resolved" line was lost with the link.
        pending.entries.removeAll { now - it.value.receivedAt > (it.value.timeoutS + PENDING_GRACE_S) * 1000L }
        val asked = pending.values.map { it.session }.toSet()
        // A session with a request waiting is "needs you" whatever it last said; one that exists only
        // because of a request (its events never reached us) is made up from the request.
        val byRequest = pending.values.groupBy { it.session }.mapValues { (_, ps) -> ps.minBy { it.receivedAt } }
        val withRequests = LinkedHashMap(sessions)
        for ((sid, p) in byRequest) {
            val s = sessions[sid]
            withRequests[sid] = (s ?: SessionInfo(sid, p.project, SessionState.NeedsYou, p.tool, p.detail, p.receivedAt))
                .copy(
                    state = SessionState.NeedsYou, tool = p.tool, detail = p.detail, updatedAt = p.receivedAt,
                    since = if (s?.state == SessionState.NeedsYou) s.since else p.receivedAt,
                )
        }
        val shown = withRequests.values.map {
            when {
                // "Done" is a moment, not a state: after it the session is simply idle.
                it.finished && now - it.since > FINISHED_MS -> it.copy(finished = false)
                it.state == SessionState.Working && now - it.updatedAt > STALE_MS -> it.copy(state = SessionState.Idle)
                // A "needs you" nothing backs: the terminal answered it, or the person left. It fades.
                it.state == SessionState.NeedsYou && it.session !in asked && now - it.updatedAt > ATTENTION_MS ->
                    it.copy(state = SessionState.Idle)
                else -> it
            }
        }
        // Whatever has been quiet long enough is forgotten (judged after fading, so a stale "needs you" goes too).
        sessions.entries.removeAll { e -> shown.any { it.session == e.key && it.state != SessionState.NeedsYou && now - it.updatedAt > FORGET_MS } }
        return shown
            .filter { it.session in sessions || it.session in byRequest }
            .sortedWith(compareBy({ it.state.ordinal.let { o -> if (it.state == SessionState.NeedsYou) -1 else o } }, { -it.updatedAt }))
    }

    /** The oldest request first: it is the one whose time runs out first. */
    fun nextPending(): Pending? = pending.values.minByOrNull { it.receivedAt }
}
