/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Context
import org.json.JSONObject
import java.io.File

/**
 * This phone's copy of the conversation: one JSON line per message, the same format as the laptop's
 * `thread.jsonl`, written through a temporary file so a crash never leaves half of it.
 */
class ThreadStore(private val file: File) {
    @Synchronized
    fun all(): List<ChatLine> {
        if (!file.exists()) return emptyList()
        return file.readLines().mapNotNull { l ->
            runCatching { JSONObject(l) }.getOrNull()?.let(ChatProtocol::lineFromJson)
        }
    }

    @Synchronized
    private fun write(lines: List<ChatLine>) {
        val kept = if (lines.size > MAX_LINES) lines.takeLast(MAX_LINES) else lines
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(kept.joinToString("\n", postfix = "\n") { ChatProtocol.lineToJson(it).toString() })
        check(tmp.renameTo(file) || (file.delete() && tmp.renameTo(file))) { "Could not save the conversation" }
    }

    @Synchronized
    fun append(line: ChatLine): ChatLine {
        val lines = all()
        lines.firstOrNull { it.id == line.id }?.let { return it }
        write(lines + line)
        return line
    }

    /** Adds the lines this copy lacks; returns how many were added. */
    @Synchronized
    fun merge(incoming: List<ChatLine>): Int {
        val lines = all()
        val known = lines.map { it.id }.toMutableSet()
        val added = incoming.filter { known.add(it.id) }
        if (added.isNotEmpty()) write((lines + added).sortedBy { it.ts })
        return added.size
    }

    @Synchronized
    fun markMirrored(ids: Set<String>) {
        val lines = all()
        if (lines.none { it.id in ids && !it.mirrored }) return
        write(lines.map { if (it.id in ids) it.copy(mirrored = true) else it })
    }

    fun unmirrored(): List<ChatLine> = all().filter { !it.mirrored }
    fun tail(n: Int): List<ChatLine> = all().takeLast(n)
    fun answerFor(cid: String): ChatLine? = all().firstOrNull { it.id == "$cid:jinx" }

    /** What was typed here and has no answer yet. */
    fun pending(): List<ChatLine> {
        val lines = all()
        val answered = lines.filter { it.fromJinx }.mapNotNull { it.cid }.toSet()
        return lines.filter { !it.fromJinx && it.src != "tg" && it.cid != null && it.cid !in answered }
    }

    companion object {
        const val MAX_LINES = 2000
        fun of(context: Context) = ThreadStore(File(context.applicationContext.filesDir, "jinx-thread.jsonl"))
    }
}
