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
    /**
     * Which backup answered when it was not Hermes: "or" (OpenRouter) or "local" (the model on this phone or
     * the laptop's). Null for Hermes and for Alan's lines. The same field (`via`) as the gateway's thread.
     */
    val via: String? = null,
) {
    val fromJinx: Boolean get() = src == "jinx"

    /** Written on Telegram, the PC or the G2 glasses: never a phone's to answer, and labelled in the panel. */
    val fromElsewhere: Boolean get() = src == "tg" || src == "pc" || src == "g2"

    /** What the panel says under a line written elsewhere; null for this conversation's own lines. */
    val sourceLabel: String? get() = when (src) {
        "tg" -> "Telegram"
        "pc" -> "PC"
        "g2" -> "Gafas"
        else -> null
    }

    /**
     * How the line goes to Telegram (the gateway's mirror_prefix): as written, with no mark of who or where,
     * except ⚠️ in front of a reply of Jinx's that a backup gave instead of Hermes.
     */
    val mirrorPrefix: String get() = if (src == "jinx" && via in BACKUP_VIAS) BACKUP_PREFIX else ""
}

/**
 * Something Jinx (Hermes) wants to do and is waiting for Alan to allow: a command it flagged as dangerous.
 * [choices] are what Hermes offers (once, session, always, deny); the phone never offers "always".
 */
data class JinxApproval(
    val runId: String,
    val requestId: String,
    val command: String,
    val description: String,
    val choices: List<String>,
    val ts: Double,
) {
    val key: String get() = "$runId|$requestId"
}

/** What has to be true before a Jinx approval can be allowed from the phone. Pure, tested. */
object JinxApprovalReview {
    /** The server keeps at most this many characters of a command and ends a longer one with "…". */
    const val SERVER_LIMIT = 2000

    /** The server cut this command: its tail cannot be reviewed here, so it is not allowed from here. */
    fun cut(command: String): Boolean = command.length >= SERVER_LIMIT - 10 && command.endsWith("…")

    /** The buttons the phone shows, in order: allowing once, for the session when Hermes offers it, denying. */
    fun buttons(a: JinxApproval): List<String> =
        listOf("once", "session").filter { it in a.choices } + "deny"
}

/** The backups that mark Jinx's line in Telegram (the gateway's BACKUP_MODES) and how. */
val BACKUP_VIAS = listOf("or", "local")
const val BACKUP_PREFIX = "⚠️ "

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
    data class Approvals(val items: List<JinxApproval>) : ChatEvent
    data class Approved(val choice: String) : ChatEvent
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
    fun approvals(): String = JSONObject().put("op", "approvals").toString()

    fun approve(a: JinxApproval, choice: String): String = JSONObject().put("op", "approve")
        .put("run_id", a.runId).put("request_id", a.requestId).put("choice", choice).toString()

    private val CHOICES = listOf("once", "session", "always", "deny")

    fun approvalFromJson(o: JSONObject): JinxApproval? {
        val run = o.optString("run_id").takeIf { it.isNotEmpty() } ?: return null
        val offered = o.optJSONArray("choices")?.let { a -> (0 until a.length()).map { a.optString(it) } }.orEmpty()
        return JinxApproval(
            runId = run,
            requestId = o.optString("request_id"),
            command = o.optString("command"),
            description = o.optString("description"),
            choices = CHOICES.filter { it in offered }.ifEmpty { listOf("once", "deny") },
            ts = o.optDouble("ts", 0.0),
        )
    }

    fun lineToJson(l: ChatLine): JSONObject = JSONObject()
        .put("id", l.id).put("ts", l.ts).put("src", l.src).put("text", l.text)
        .put("cid", l.cid ?: JSONObject.NULL).put("mirrored", l.mirrored)
        .also { o -> l.mood?.let { o.put("mood", it) } }
        .also { o -> l.via?.takeIf { it in BACKUP_VIAS }?.let { o.put("via", it) } }

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
            via = o.optString("via").takeIf { it in BACKUP_VIAS },
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
            "approvals" -> ChatEvent.Approvals(
                o.optJSONArray("items")?.let { a -> (0 until a.length()).mapNotNull { a.optJSONObject(it)?.let(::approvalFromJson) } }.orEmpty(),
            )
            "approved" -> ChatEvent.Approved(o.optString("choice"))
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
