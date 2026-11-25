package com.agupta07505.smartisland.coucou.clip

import org.json.JSONObject

/**
 * What the phone sends to the laptop when something is copied (phone -> PC; the other direction is KDE Connect's).
 * Pure, so the rules can be tested; the laptop applies the same ones again (windows/src-tauri/src/clip_sync.rs).
 */
object ClipSync {
    /** Same limit as the laptop's clipboard history. */
    const val MAX_CHARS = 4_000

    /**
     * The text as it may travel: control characters other than newline and tab dropped, trimmed; null when what is
     * left is empty or longer than [MAX_CHARS] (a long text is not sent cut, it is not sent).
     */
    fun clean(text: String?): String? {
        if (text == null) return null
        val kept = StringBuilder(text.length)
        for (ch in text) {
            if (ch == '\n' || ch == '\t' || !Character.isISOControl(ch)) kept.append(ch)
        }
        val out = kept.toString().trim()
        return if (out.isEmpty() || out.codePointCount(0, out.length) > MAX_CHARS) null else out
    }

    /** FNV-1a 64 over the UTF-8 bytes: stable, and the same value on the laptop. Never reversible to the text in logs. */
    fun fingerprint(text: String): Long {
        var h = -3750763034362895579L // 0xcbf29ce484222325
        for (b in text.toByteArray(Charsets.UTF_8)) {
            h = (h xor (b.toLong() and 0xff)) * 1099511628211L
        }
        return h
    }

    /** The line the laptop's phone link understands (`op: "clip"`). */
    fun line(text: String, atMs: Long): String =
        JSONObject().put("op", "clip").put("text", text).put("at", atMs).toString()
}

/**
 * Stops copy loops. The laptop's clipboard comes back to the phone through KDE Connect, which sets the phone's own
 * clipboard, which would be read and sent again, forever. So the same text twice in a row is sent once, and a text
 * that just arrived from the other side is not sent back.
 */
class LoopGuard(private val windowMs: Long = 5_000) {
    private var lastSent: Long? = null
    private var receivedFp: Long? = null
    private var receivedAt = 0L

    fun shouldSend(text: String, nowMs: Long): Boolean {
        val fp = ClipSync.fingerprint(text)
        if (fp == receivedFp && nowMs - receivedAt < windowMs) return false
        if (fp == lastSent) return false
        lastSent = fp
        return true
    }

    fun noteReceived(text: String, nowMs: Long) {
        receivedFp = ClipSync.fingerprint(text)
        receivedAt = nowMs
    }
}

/**
 * Android 10+ does not let an app read the clipboard in the background, but it logs a line every time it refuses
 * one ("Denying clipboard access to <package>, application is not in focus nor is it a system service"). With the
 * READ_LOGS permission (granted once over adb) that line is the signal that something was copied.
 */
object ClipLogTrigger {
    /** Whether a log line is the system refusing the clipboard to [pkg]. Tolerant of wording differences between builds. */
    fun isDenialFor(line: String, pkg: String): Boolean {
        if (!line.contains(pkg)) return false
        if (!line.contains("clipboard", ignoreCase = true)) return false
        return line.contains("deny", ignoreCase = true) ||
            line.contains("denied", ignoreCase = true) ||
            line.contains("not in focus", ignoreCase = true)
    }
}
