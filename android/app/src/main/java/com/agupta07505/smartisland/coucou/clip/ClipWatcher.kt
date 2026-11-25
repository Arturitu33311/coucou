package com.agupta07505.smartisland.coucou.clip

import android.content.Context
import android.content.pm.PackageManager
import android.os.SystemClock
import android.util.Log
import com.agupta07505.smartisland.coucou.CoucouRuntime
import com.agupta07505.smartisland.service.SmartIslandOverlayService
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Sends what is copied on the phone to the laptop, without pressing anything.
 *
 * Android 10+ refuses the clipboard to an app that is not focused, and does not even tell it that something was copied.
 * But the system logs a line every time it refuses (see [ClipLogTrigger]). With the READ_LOGS permission, granted once
 * over adb, that line is the signal: the accessibility service then holds the input focus for a moment, reads the
 * text, and it travels on the phone link to the laptop. Experimental and off by default; the text is never logged
 * (only its length), a clip marked sensitive is never read, and what arrives from the laptop is not sent back.
 */
object ClipWatcher {
    private const val TAG = "ClipWatcher"
    const val READ_LOGS = "android.permission.READ_LOGS"
    /** Several lines for one copy (one per refused listener) count as one. */
    private const val DEBOUNCE_MS = 300L

    private val _status = MutableStateFlow("apagado")
    /** What to show in Settings: off, waiting for the permission, watching, and the last result. */
    val status: StateFlow<String> = _status.asStateFlow()

    @Volatile private var running = false
    @Volatile private var process: Process? = null
    private var thread: Thread? = null
    private val guard = LoopGuard()
    private var lastTriggerAt = 0L

    fun hasLogPermission(context: Context): Boolean =
        context.checkSelfPermission(READ_LOGS) == PackageManager.PERMISSION_GRANTED

    @Synchronized
    fun start(app: Context) {
        if (running) return
        if (!hasLogPermission(app)) {
            _status.value = "falta el permiso: adb shell pm grant ${app.packageName} $READ_LOGS"
            return
        }
        running = true
        _status.value = "vigilando copias"
        thread = Thread({ loop(app.applicationContext) }, "clip-log").also {
            it.isDaemon = true
            it.start()
        }
    }

    @Synchronized
    fun stop() {
        running = false
        process?.destroy()
        process = null
        thread?.interrupt()
        thread = null
        _status.value = "apagado"
    }

    private fun loop(app: Context) {
        val pkg = app.packageName
        var wait = 2_000L
        while (running) {
            try {
                // `-T 1`: from now on, not the history. `-e`: only lines that mention the clipboard; `*:W`: warnings and up.
                val p = ProcessBuilder("logcat", "-v", "brief", "-T", "1", "-e", "lipboard", "*:W")
                    .redirectErrorStream(true)
                    .start()
                process = p
                p.inputStream.bufferedReader().use { reader ->
                    wait = 2_000L
                    while (running) {
                        val line = reader.readLine() ?: break
                        if (ClipLogTrigger.isDenialFor(line, pkg)) onCopied(app)
                    }
                }
                p.destroy()
            } catch (e: Exception) {
                Log.w(TAG, "logcat reader: ${e.message}")
            }
            if (!running) break
            try {
                Thread.sleep(wait)
            } catch (_: InterruptedException) {
                break
            }
            wait = minOf(wait * 2, 30_000L)
        }
    }

    private fun onCopied(app: Context) {
        val now = SystemClock.elapsedRealtime()
        if (now - lastTriggerAt < DEBOUNCE_MS) return
        lastTriggerAt = now
        val service = SmartIslandOverlayService.instance
        if (service == null) {
            Log.d(TAG, "something was copied but the accessibility service is not connected")
            return
        }
        service.readClipboardWithFocus { raw ->
            val text = ClipSync.clean(raw)
            if (text == null) {
                Log.d(TAG, "copied, nothing usable to send (${if (raw == null) "not readable or sensitive" else "empty or too long"})")
                return@readClipboardWithFocus
            }
            val at = System.currentTimeMillis()
            if (!guard.shouldSend(text, at)) {
                Log.d(TAG, "copied, not sent (a repeat or an echo)")
                return@readClipboardWithFocus
            }
            val sent = CoucouRuntime.sendToLaptop(ClipSync.line(text, at))
            // Length only: what was copied never goes in a log.
            Log.d(TAG, "copied ${text.codePointCount(0, text.length)} characters, ${if (sent) "sent to the laptop" else "NOT sent: no link to the laptop"}")
            _status.value = if (sent) "última copia enviada" else "copia sin enviar: sin enlace con el laptop"
        }
    }
}
