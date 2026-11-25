package com.agupta07505.smartisland.coucou.presence

import org.json.JSONArray

/** A button on one of Jinx's notices (Hecho / Mañana / Descartar). `id` is what the server acts on. */
data class NoticeAction(val id: String, val label: String)

/** The buttons the open card shows, for the notice [msgId]. */
data class NoticeButtons(val msgId: Long, val actions: List<NoticeAction>)

/**
 * Notice buttons, pure so they can be tested. The rules are the presence server's (`clean_actions`): at most three,
 * ids in snake_case, unique, labels trimmed to 24 characters. The phone validates them again on its own: it never
 * shows a button the server would refuse.
 */
object NoticeActions {
    const val MAX = 3
    const val MAX_LABEL = 24
    private val ID = Regex("^[a-z][a-z_]{0,19}$")

    fun parse(arr: JSONArray?): List<NoticeAction> {
        if (arr == null) return emptyList()
        val out = mutableListOf<NoticeAction>()
        val seen = mutableSetOf<String>()
        for (i in 0 until arr.length()) {
            val o = arr.optJSONObject(i) ?: continue
            val id = o.optString("id", "")
            val label = o.optString("label", "").trim()
            if (!ID.matches(id) || label.isEmpty() || !seen.add(id)) continue
            out += NoticeAction(id, label.take(MAX_LABEL))
            if (out.size == MAX) break
        }
        return out
    }

    /** What the card should show: the buttons of a notice nobody has acted on yet, else none. */
    fun forCard(msg: PresenceMsg): NoticeButtons? =
        if (msg.actions.isNotEmpty() && msg.acted == null) NoticeButtons(msg.id, msg.actions) else null
}
