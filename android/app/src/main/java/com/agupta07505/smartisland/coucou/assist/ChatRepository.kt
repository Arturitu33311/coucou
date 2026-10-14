/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import java.util.UUID

enum class Via { Laptop, Cloud, Local }

/** A brain that lives on this phone (cloud through OpenRouter, or the on-device model). */
interface LocalBrain {
    val via: Via

    /** Answers the conversation so far (ending with the user's line); throws [ChatException]. */
    fun reply(context: List<ChatLine>): String
}

/** Where a line is repeated so Telegram keeps the whole conversation. */
interface Mirror {
    fun deliver(text: String): Boolean
}

sealed interface SendResult {
    data class Answered(val line: ChatLine, val via: Via) : SendResult
    data class Pending(val reason: String) : SendResult
}

/**
 * One conversation, whichever brain answers it: the laptop's gateway if it can, else this phone's own
 * brains in order. Everything lands in the local [ThreadStore]; nothing is invented when nobody can answer.
 */
class ChatRepository(
    private val store: ThreadStore,
    private val deviceId: String,
    private val laptop: ChatApi?,
    private val brains: List<LocalBrain>,
    private val mirror: Mirror?,
    private val clock: () -> Double = { System.currentTimeMillis() / 1000.0 },
    private val newCid: () -> String = { UUID.randomUUID().toString() },
) {
    /** Stores what the user typed at once (so the screen can show it) without asking anyone yet. */
    fun enqueue(text: String): ChatLine {
        val cid = newCid()
        return store.append(ChatLine("$cid:$deviceId", clock(), deviceId, text, cid, false))
    }

    fun send(text: String): SendResult = answer(enqueue(text))

    /** Asks again for what was typed and never answered (a model came back, or the laptop did). */
    fun retryPending(): List<SendResult> = store.pending().map { answer(it) }

    /** Gets an answer for a line already in the thread: the laptop first, then this phone's own brains. */
    fun answer(user: ChatLine): SendResult {
        val cid = user.cid ?: return SendResult.Pending("sin cid")
        store.answerFor(cid)?.let { return SendResult.Answered(it, Via.Laptop) }
        laptop?.let { api ->
            try {
                val done = api.send(user.text, cid, deviceId)
                val line = store.append(ChatLine("$cid:jinx", clock(), "jinx", done.text, cid, done.mirrored))
                if (done.mirrored) store.markMirrored(setOf(user.id, line.id))
                return SendResult.Answered(line, Via.Laptop)
            } catch (_: ChatException) {
                // falls through to this phone's own brains
            }
        }
        var reason = "sin modelo disponible"
        for (b in brains) {
            try {
                val text = b.reply(store.tail(CONTEXT))
                val line = store.append(ChatLine("$cid:jinx", clock(), "jinx", text, cid, false))
                flushMirror()
                return SendResult.Answered(line, b.via)
            } catch (e: ChatException) {
                reason = e.message ?: reason
            }
        }
        return SendResult.Pending(reason)
    }

    /** Repeats, in order, the lines Telegram has not seen; stops at the first failure. */
    fun flushMirror() {
        val m = mirror ?: return
        val done = mutableSetOf<String>()
        for (l in store.unmirrored()) {
            val prefix = if (l.fromJinx) "🟠 " else "📱 "
            if (!m.deliver((prefix + l.text).take(TG_MAX))) break
            done += l.id
        }
        if (done.isNotEmpty()) store.markMirrored(done)
    }

    /** Hands our lines to the laptop and takes the ones we lack. Returns how many were added here. */
    fun sync(): Int {
        val api = laptop ?: return 0
        return try {
            val ours = store.all()
            val res = api.merge(ours.filter { !it.mirrored || it.src != "tg" }, ours.map { it.id })
            store.markMirrored(res.mirroredIds.toSet())
            store.merge(res.items)
        } catch (_: ChatException) {
            0
        }
    }

    private companion object {
        const val CONTEXT = 20
        const val TG_MAX = 4000
    }
}
