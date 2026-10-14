/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import org.json.JSONArray
import org.json.JSONObject

/** One message of the shared conversation. `src` is tg | s21 | s22 | pc | jinx (pc: Coucou on the laptop). */
data class ChatLine(
    val id: String,
    val ts: Double,
    val src: String,
    val text: String,
    val cid: String?,
    val mirrored: Boolean,
    /** Jinx's mood when this line is hers and the gateway sent one: a name of [JinxMood.EMOTES], else null. */
    val mood: String? = null,
) {
    val fromJinx: Boolean get() = src == "jinx"

    /** Written on Telegram or on the PC: never a phone's to answer, and labelled in the panel. */
    val fromElsewhere: Boolean get() = src == "tg" || src == "pc"

    /** What the panel says under a line written elsewhere; null for this conversation's own lines. */
    val sourceLabel: String? get() = when (src) {
        "tg" -> "Telegram"
        "pc" -> "PC"
        else -> null
    }

    /** How the line goes to Telegram (same as the laptop gateway's MIRROR_PREFIX). */
    val mirrorPrefix: String get() = when (src) {
        "jinx" -> "🟠 "
        "pc" -> "💻 "
        else -> "📱 "
    }
}

sealed interface ChatEvent {
    data object Ack : ChatEvent
    /** [mirrored]: the gateway confirms both lines reached Telegram (true for gateways that do not say). */
    data class Done(
        val mode: String,
        val text: String,
        val mirrored: Boolean = true,
        /** The mood Jinx sent with this reply (one of [JinxMood.EMOTES]); null when she sent none. */
        val mood: String? = null,
    ) : ChatEvent
    data class Failure(val msg: String) : ChatEvent
    data class History(val items: List<ChatLine>) : ChatEvent
    data class Merged(val added: Int, val items: List<ChatLine>, val mirroredIds: List<String>) : ChatEvent
    data class Core(val core: String, val memory: String) : ChatEvent
    data class Secrets(val openrouterKey: String, val telegramToken: String, val chat: String) : ChatEvent
}

open class ChatException(message: String) : Exception(message)

/** The requests and events `jinx-chat` on the laptop speaks (see the spec). Pure: tested on the JVM. */
object ChatProtocol {
    fun history(n: Int): String = JSONObject().put("op", "history").put("n", n).toString()

    fun send(text: String, cid: String, src: String): String =
        JSONObject().put("op", "send").put("text", text).put("cid", cid).put("src", src).toString()

    fun merge(lines: List<ChatLine>, have: List<String>): String =
        JSONObject().put("op", "merge")
            .put("lines", JSONArray().also { a -> lines.forEach { a.put(lineToJson(it)) } })
            .put("have", JSONArray(have))
            .toString()

    fun core(): String = JSONObject().put("op", "core").toString()
    fun provision(): String = JSONObject().put("op", "provision").toString()

    fun lineToJson(l: ChatLine): JSONObject = JSONObject()
        .put("id", l.id).put("ts", l.ts).put("src", l.src).put("text", l.text)
        .put("cid", l.cid ?: JSONObject.NULL).put("mirrored", l.mirrored)
        .also { o -> l.mood?.let { o.put("mood", it) } }

    fun lineFromJson(o: JSONObject): ChatLine? {
        val id = o.optString("id").takeIf { it.isNotEmpty() } ?: return null
        val src = o.optString("src").takeIf { it.isNotEmpty() } ?: return null
        if (!o.has("text") || !o.has("ts")) return null
        return ChatLine(
            id = id,
            ts = o.optDouble("ts"),
            src = src,
            text = o.optString("text"),
            cid = if (o.isNull("cid")) null else o.optString("cid").takeIf { it.isNotEmpty() },
            mirrored = o.optBoolean("mirrored", false),
            mood = JinxMood.validEmote(if (o.isNull("mood")) null else o.optString("mood")),
        )
    }

    private fun lines(arr: JSONArray?): List<ChatLine> =
        (0 until (arr?.length() ?: 0)).mapNotNull { i -> arr!!.optJSONObject(i)?.let(::lineFromJson) }

    fun parse(raw: String): List<ChatEvent> = raw.lineSequence().mapNotNull { l ->
        val o = runCatching { JSONObject(l.trim()) }.getOrNull() ?: return@mapNotNull null
        when (o.optString("ev")) {
            "ack" -> ChatEvent.Ack
            "done" -> ChatEvent.Done(
                o.optString("mode"), o.optString("text"), o.optBoolean("mirrored", true),
                JinxMood.validEmote(if (o.isNull("mood")) null else o.optString("mood")),
            )
            "error" -> ChatEvent.Failure(o.optString("msg", "error"))
            "history" -> ChatEvent.History(lines(o.optJSONArray("items")))
            "merged" -> ChatEvent.Merged(
                o.optInt("added"),
                lines(o.optJSONArray("items")),
                o.optJSONArray("mirrored_ids")?.let { a -> (0 until a.length()).map { a.getString(it) } }.orEmpty(),
            )
            "core" -> ChatEvent.Core(o.optString("core"), o.optString("memory"))
            "secrets" -> ChatEvent.Secrets(o.optString("openrouter_key"), o.optString("telegram_token"), o.optString("chat"))
            else -> null
        }
    }.toList()

    /** The answer of a `send`, or the gateway's error as an exception. */
    fun expectDone(raw: String): ChatEvent.Done = expect(raw)

    inline fun <reified T : ChatEvent> expect(raw: String): T {
        val ev = parse(raw)
        ev.filterIsInstance<ChatEvent.Failure>().firstOrNull()?.let { throw ChatException(it.msg) }
        return ev.filterIsInstance<T>().firstOrNull() ?: throw ChatException("Jinx no contestó")
    }
}
