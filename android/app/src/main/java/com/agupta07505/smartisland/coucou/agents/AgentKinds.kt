/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.agents

import androidx.compose.ui.graphics.Color

/**
 * The coding agents the laptop follows, by the name its relay tags them with (windows/src/core/pills.ts: the
 * same names and colours as their pills on the PC). "" is Claude Code itself.
 */
object AgentKinds {
    data class Kind(val name: String, val color: Color)

    private val CLAUDE = Kind("Claude Code", Color(0xFFD97757))

    private val KNOWN = mapOf(
        "antigravity" to Kind("Antigravity", Color(0xFFE879F9)),
        "opencode" to Kind("OpenCode", Color(0xFF4ADE80)),
        "codex" to Kind("Codex", Color(0xFF2DD4BF)),
        "gemini" to Kind("Gemini CLI", Color(0xFF8AB4F8)),
        "cursor" to Kind("Cursor", Color(0xFFC0C4CC)),
        "copilot" to Kind("Copilot CLI", Color(0xFF818CF8)),
        "amp" to Kind("Amp", Color(0xFFF59E0B)),
        "hermes" to Kind("Hermes", Color(0xFFC084FC)),
        "muse" to Kind("Muse Code", Color(0xFF38BDF8)),
    )

    /** The same rule as the laptop's (hooks.ts validateAgent): a-z, 0-9 and "-", at most 24, never "claude". */
    fun valid(raw: String?): String {
        val a = raw ?: return ""
        return a.takeIf { it.isNotEmpty() && it.length <= 24 && it != "claude" && it.all { c -> c in 'a'..'z' || c in '0'..'9' || c == '-' } } ?: ""
    }

    /** Its name and colour; an agent we do not know by name gets its tag, capitalised, in grey. */
    fun of(agent: String): Kind = when {
        agent.isEmpty() -> CLAUDE
        else -> KNOWN[agent] ?: Kind(agent.replaceFirstChar { it.uppercase() }, Color(0xFF9CA3AF))
    }

    /** The agent whose name a card carries (its appName), or Claude Code's. */
    fun byName(name: String?): Kind = (KNOWN.values + CLAUDE).firstOrNull { it.name == name } ?: CLAUDE

    /** What the card's header calls them: one name when they are all the same agent, else "Agentes". */
    fun title(agents: Collection<String>): String = agents.toSet().singleOrNull()?.let { of(it).name } ?: "Agentes"
}
