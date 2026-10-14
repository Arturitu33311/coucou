/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import com.agupta07505.smartisland.coucou.ui.MochiMood
import java.text.Normalizer

/**
 * Jinx's mood, read from what she just said: which of Mochi's emotes her reply carries. The PC has the same
 * rules (windows/src/core/jinx-mood.ts); both must pass jinx-mood-vectors.json (scripts/check-contracts.sh).
 *
 * Rules are tried in order and the first that matches wins. A rule matches on an emoji anywhere, or on a
 * word: "llor*" is a word start, "uy" a whole word, "por fin" a phrase. Accents and case don't count.
 */
object JinxMood {
    private class Rule(val emote: String, val emoji: List<String>, words: List<String>) {
        val words = words.map(::wordRegex)
    }

    private val RULES = listOf(
        Rule("crying", listOf("😭"), listOf("llor*", "estoy llorando")),
        Rule("laughing", listOf("😂", "🤣", "😆", "😹"), listOf("jaja*", "jeje*", "jiji*", "jsjs*", "xd", "lol", "haha*")),
        Rule("love", listOf("❤", "♥", "😍", "🥰", "😘", "💕", "💖", "💗", "💜", "💚", "💙", "🫶"), listOf("te quiero", "te amo", "te adoro", "<3")),
        Rule("scared", listOf("😱", "😨", "😰", "🙀"), listOf("miedo", "aterr*", "peligro", "cuidado")),
        Rule("grumpy", listOf("😠", "😡", "😤", "🙄", "💢"), listOf("enoj*", "molest*", "ugh", "harta", "harto", "otra vez")),
        Rule("sad", listOf("😢", "😞", "😔", "🥺", "💔", "😿"), listOf("triste*", "lo siento", "lo lamento", "lament*", "no pude", "no puedo", "fall*")),
        Rule("flustered", listOf("😅", "😵", "💦", "🫣"), listOf("ups", "uy", "oops", "perdon", "me equivoque")),
        Rule("shy", listOf("🙈", "😳", "☺"), listOf("verguenza", "me sonroj*", "que pena")),
        Rule("smug", listOf("😏", "😎"), listOf("obvio", "obviamente", "te lo dije", "por supuesto")),
        Rule("confused", listOf("🤔", "❓", "😕"), listOf("no entiendo", "no entendi", "no lo se", "ni idea", "no estoy segur*", "a que te refieres", "confund*", "confus*")),
        Rule("surprised", listOf("😮", "😲", "😯", "🤯", "‼"), listOf("wow", "guau", "no puede ser", "en serio?", "increible*")),
        Rule("excited", listOf("🎉", "🥳", "🤩", "✨", "🚀"), listOf("genial*", "emocion*", "que emocion", "yay", "siii*")),
        Rule("proud", listOf("✅", "🏆", "⭐", "🌟"), listOf("listo", "ya quedo", "ya esta hecho", "lo logr*", "terminad*", "completad*")),
        Rule("relieved", listOf("😌", "😮‍💨"), listOf("por fin", "menos mal", "que alivio", "alivio", "uf", "uff*")),
        Rule("determined", listOf("💪", "🔥", "⚡"), listOf("vamos", "a darle", "manos a la obra", "me encargo", "dejamelo", "yo me ocupo")),
        Rule("curious", listOf("👀", "🧐"), listOf("cuentame", "interesante", "curios*", "y eso?", "como asi?")),
        Rule("sleepy", listOf("😴", "🥱", "💤"), listOf("sueno", "dormir", "buenas noches", "a mimir", "cansad*")),
        Rule("bored", listOf("😑", "😐"), listOf("aburr*", "meh")),
        Rule("happy", listOf("😊", "🙂", "😄", "😁", "😃", "😀", "🤗", ":)", ":D"), listOf("hola", "buenos dias", "buenas tardes", "feliz", "me alegra*", "que bien", "perfecto", "excelente", "claro")),
    )

    /** Lower case, no accents (ñ → n too). */
    fun fold(text: String): String =
        Normalizer.normalize(text, Normalizer.Form.NFD).replace(Regex("\\p{Mn}+"), "").lowercase()

    private fun wordRegex(word: String): Regex {
        val stem = word.endsWith("*")
        val body = Regex.escape(fold(if (stem) word.dropLast(1) else word))
        return Regex("(?<![\\p{L}\\p{N}])$body" + if (stem) "" else "(?![\\p{L}\\p{N}])")
    }

    /** The emote (the PC's BotEmoteName) Jinx's reply carries, or null when it reads neutral. Pure, tested. */
    fun emote(text: String?): String? {
        if (text.isNullOrEmpty()) return null
        val folded = fold(text)
        return RULES.firstOrNull { r -> r.emoji.any { text.contains(it) } || r.words.any { it.containsMatchIn(folded) } }?.emote
    }

    /** The emote's face on the phone's Mochi. */
    fun moodOf(emote: String?): MochiMood? = when (emote) {
        "happy" -> MochiMood.Happy
        "surprised" -> MochiMood.Surprised
        "sleepy" -> MochiMood.Sleepy
        "dance" -> MochiMood.Dancing
        null -> null
        else -> MochiMood.entries.firstOrNull { it.emote == emote }
    }

    /** What Mochi's face shows for this reply of Jinx's. */
    fun moodOfText(text: String?): MochiMood? = moodOf(emote(text))

    /** How long her last reply sets Mochi's face in the chat; after that he just listens again. */
    const val FRESH_S = 15 * 60

    /**
     * Mochi's face at the top of Jinx's chat: thinking while she answers, then the mood of her reply while
     * it is the latest line and fresh, listening otherwise. Pure, tested.
     */
    fun headerMood(lines: List<ChatLine>, busy: Boolean, nowS: Double): MochiMood {
        if (busy) return MochiMood.Thinking
        val last = lines.lastOrNull()?.takeIf { it.fromJinx } ?: return MochiMood.Listening
        // The gateway stamps seconds; a millisecond stamp is read as such.
        val ts = if (last.ts > 1e11) last.ts / 1000.0 else last.ts
        if (nowS - ts > FRESH_S) return MochiMood.Listening
        return moodOfText(last.text) ?: MochiMood.Listening
    }
}
