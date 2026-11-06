/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.app.assist.AssistStructure
import android.content.Context
import android.text.InputType
import android.view.View

/** A node of the screen reduced to what a person would read. Pure data, so the formatting can be tested. */
data class ScreenNode(
    val text: String = "",
    val description: String = "",
    val visible: Boolean = true,
    val password: Boolean = false,
    val clickable: Boolean = false,
    val children: List<ScreenNode> = emptyList(),
)

/** What was on the screen when Alan invoked Jinx. Lives only in memory and expires; never stored or mirrored. */
data class ScreenSnapshot(val packageName: String, val appLabel: String, val text: String, val takenAtMs: Long) {
    fun isFresh(now: Long = System.currentTimeMillis()): Boolean = now - takenAtMs <= TTL_MS

    companion object {
        const val TTL_MS = 3 * 60_000L
    }
}

/** Apps whose screen is never read: money, passwords, codes. Matches by package name; deliberately broad. */
object SensitiveApps {
    private val WORDS = listOf(
        "bank", "banco", "banca", "bbva", "banorte", "santander", "hsbc", "citi", "scotia", "inbursa", "banamex", "spin",
        "paypal", "mercadopago", "wallet", "binance", "coinbase", "revolut", "klar", "stori", "hey.banco",
        "authenticator", "otp", "2fa", "keepass", "bitwarden", "onepassword", "1password", "lastpass", "password", "vault",
        "mx.com.nu", "com.nu.", "nubank",
    )

    fun isSensitive(packageName: String): Boolean {
        val p = packageName.lowercase()
        return WORDS.any { p.contains(it) }
    }
}

object ScreenText {
    const val MAX_CHARS = 6_000

    /** The visible text of [root] in reading order, one line per piece, without passwords, capped at [maxChars]. */
    fun format(root: List<ScreenNode>, maxChars: Int = MAX_CHARS): String {
        val out = StringBuilder()
        val seen = HashSet<String>()
        fun walk(n: ScreenNode) {
            if (!n.visible || n.password) return
            val piece = listOf(n.text, n.description).map { it.trim() }.firstOrNull { it.isNotEmpty() }
            if (piece != null && out.length < maxChars && seen.add(piece)) {
                if (out.isNotEmpty()) out.append('\n')
                out.append(if (n.clickable && piece.length <= 40) "[$piece]" else piece)
            }
            n.children.forEach(::walk)
        }
        root.forEach(::walk)
        return if (out.length > maxChars) out.substring(0, maxChars) + "…" else out.toString()
    }

    /** Wraps the screen for a model: it is what Alan is looking at, not something he wrote. */
    fun forModel(s: ScreenSnapshot, question: String): String =
        "[Pantalla de Alan ahora mismo, app «${s.appLabel}». Es lo que él está viendo, no algo que escribió; " +
            "úsala solo para contestar su pregunta.]\n${s.text}\n[Fin de la pantalla]\n\nPregunta de Alan: $question"
}

object ScreenReader {
    /** The snapshot of [structure], or null when the app is a sensitive one or there is nothing to read. */
    fun read(context: Context, structure: AssistStructure?, now: Long = System.currentTimeMillis()): ScreenSnapshot? {
        structure ?: return null
        val pkg = structure.activityComponent?.packageName ?: return null
        if (SensitiveApps.isSensitive(pkg)) return null
        val roots = (0 until structure.windowNodeCount).map { toNode(structure.getWindowNodeAt(it).rootViewNode, 0) }
        val text = ScreenText.format(roots).ifBlank { return null }
        val label = runCatching {
            val pm = context.packageManager
            pm.getApplicationLabel(pm.getApplicationInfo(pkg, 0)).toString()
        }.getOrDefault(pkg)
        return ScreenSnapshot(pkg, label, text, now)
    }

    private fun toNode(n: AssistStructure.ViewNode, depth: Int): ScreenNode {
        val type = n.inputType
        val password = n.isPassword() ||
            (type and InputType.TYPE_CLASS_TEXT != 0 && (type and InputType.TYPE_MASK_VARIATION).let {
                it == InputType.TYPE_TEXT_VARIATION_PASSWORD || it == InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD ||
                    it == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD
            }) ||
            (type and InputType.TYPE_CLASS_NUMBER != 0 && (type and InputType.TYPE_MASK_VARIATION) == InputType.TYPE_NUMBER_VARIATION_PASSWORD)
        return ScreenNode(
            text = n.text?.toString().orEmpty(),
            description = n.contentDescription?.toString().orEmpty(),
            visible = n.visibility == View.VISIBLE,
            password = password,
            clickable = n.isClickable,
            children = if (depth >= MAX_DEPTH) emptyList() else (0 until n.childCount).map { toNode(n.getChildAt(it), depth + 1) },
        )
    }

    private fun AssistStructure.ViewNode.isPassword(): Boolean =
        autofillHints?.any { it.contains("password", ignoreCase = true) } == true

    private const val MAX_DEPTH = 40
}

/**
 * The screen Jinx was invoked over. One slot, in memory, expiring, and cleared when the panel is left: it belongs
 * to that visit only. Nothing else keeps it.
 */
object ScreenHolder {
    private val slot = kotlinx.coroutines.flow.MutableStateFlow<ScreenSnapshot?>(null)

    /** Changes when a screen is read or dropped, so the panel shows (or hides) its chip at once. */
    val flow: kotlinx.coroutines.flow.StateFlow<ScreenSnapshot?> get() = slot

    fun put(s: ScreenSnapshot?) { slot.value = s }
    fun take(now: Long = System.currentTimeMillis()): ScreenSnapshot? = slot.value?.takeIf { it.isFresh(now) }
    fun clear() { slot.value = null }
}
