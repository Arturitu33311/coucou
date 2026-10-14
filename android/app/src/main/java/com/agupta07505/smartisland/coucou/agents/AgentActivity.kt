/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * What Claude Code is doing, said the way a person would say it. The laptop sends a tool name and a
 * detail (a command, a path, a pattern); this turns them into a kind, a verb and a short subject so
 * the island can show "Editing Mochi.kt" instead of "Edit · /home/alan/dev/.../Mochi.kt".
 */

package com.agupta07505.smartisland.coucou.agents

/** The family of thing it is busy with: drives the colour, Mochi's face and the verb. */
enum class ActivityKind { Thinking, Reading, Editing, Searching, Browsing, Running, Delegating, Planning, Using, Asking, Waiting, Idle, Finished }

data class Activity(val kind: ActivityKind, val verb: String, val subject: String) {
    /** "Editing Mochi.kt", or just "Thinking" when there is no subject. */
    val line: String get() = if (subject.isEmpty()) verb else "$verb $subject"
}

object AgentActivity {
    /** The longest subject the island will carry: it is a glance, not a log. */
    const val SUBJECT_MAX = 48

    fun of(tool: String, detail: String, state: SessionState): Activity {
        if (state == SessionState.NeedsYou) {
            return Activity(ActivityKind.Asking, "Needs you", clean(detail).takeIf { it != "needs you" } ?: "")
        }
        if (state == SessionState.Idle) return Activity(ActivityKind.Idle, "Idle", "")
        return when {
            tool.isEmpty() -> Activity(ActivityKind.Thinking, "Thinking", "")
            tool == "Read" || tool == "NotebookRead" -> Activity(ActivityKind.Reading, "Reading", fileName(detail))
            tool == "Edit" || tool == "MultiEdit" || tool == "NotebookEdit" ->
                Activity(ActivityKind.Editing, "Editing", fileName(detail))
            tool == "Write" -> Activity(ActivityKind.Editing, "Writing", fileName(detail))
            tool == "Grep" -> Activity(ActivityKind.Searching, "Searching", clean(detail))
            tool == "Glob" || tool == "LS" -> Activity(ActivityKind.Searching, "Listing", clean(detail))
            tool == "WebSearch" -> Activity(ActivityKind.Browsing, "Searching the web", clean(detail))
            tool == "WebFetch" -> Activity(ActivityKind.Browsing, "Fetching", host(detail))
            tool == "Bash" || tool == "BashOutput" || tool == "KillShell" ->
                Activity(ActivityKind.Running, "Running", clean(detail))
            tool == "Task" || tool == "Agent" -> Activity(ActivityKind.Delegating, "Delegating", clean(detail))
            tool == "TodoWrite" || tool == "ExitPlanMode" || tool == "EnterPlanMode" ->
                Activity(ActivityKind.Planning, "Planning", "")
            tool.startsWith("mcp__") -> {
                val parts = tool.removePrefix("mcp__").split("__")
                val name = parts.getOrNull(1)?.replace('_', ' ') ?: parts.first()
                Activity(ActivityKind.Using, "Using", listOfNotNull(parts.firstOrNull(), name.takeIf { parts.size > 1 }).joinToString(" · "))
            }
            else -> Activity(ActivityKind.Using, "Using", tool)
        }
    }

    /** A run that has just ended: "Done 2 min 14 s" (the verb is told in the person's language by the UI). */
    fun finished(ranMs: Long): Activity =
        Activity(ActivityKind.Finished, "Done", if (ranMs >= 1000) elapsed(ranMs) else "")

    /** The last segment of a path ("/a/b/Mochi.kt" -> "Mochi.kt"); anything that is not a path is cleaned as text. */
    fun fileName(detail: String): String {
        val line = detail.trim().lineSequence().firstOrNull().orEmpty().trim().trim('"', '\'')
        if (line.isEmpty()) return ""
        val looksLikePath = !line.contains(' ') || line.startsWith("/") || line.startsWith("~") || line.startsWith("./")
        val name = if (looksLikePath) line.substringAfterLast('/').substringAfterLast('\\') else line
        return clean(name.ifEmpty { line })
    }

    /** "https://docs.rs/serde/latest" -> "docs.rs". */
    fun host(detail: String): String {
        val line = detail.trim().lineSequence().firstOrNull().orEmpty().trim()
        val m = Regex("""^[a-z]+://([^/\s:?#]+)""", RegexOption.IGNORE_CASE).find(line)
        return m?.groupValues?.get(1)?.removePrefix("www.") ?: clean(line)
    }

    /** First line, collapsed whitespace, cut with an ellipsis. */
    fun clean(detail: String): String {
        val first = detail.trim().lineSequence().firstOrNull().orEmpty().replace(Regex("\\s+"), " ").trim()
        return if (first.length <= SUBJECT_MAX) first else first.take(SUBJECT_MAX - 1).trimEnd() + "…"
    }

    /** "45 s", "2 min 14 s", "1 h 05 min": how long it has been at it, for a glance. */
    fun elapsed(ms: Long): String {
        val s = (ms / 1000).coerceAtLeast(0)
        return when {
            s < 60 -> "$s s"
            s < 3600 -> "${s / 60} min ${(s % 60).toString().padStart(2, '0')} s"
            else -> "${s / 3600} h ${((s % 3600) / 60).toString().padStart(2, '0')} min"
        }
    }
}
