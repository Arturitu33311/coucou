package com.agupta07505.smartisland.coucou.notas

import java.io.File
import java.io.IOException

/** What the queue needs from the notes server. Calls return the HTTP status; a network failure throws IOException. */
interface NotasUploader {
    /** POST /api/sesiones. Returns the new session id, or null if the server did not accept it. */
    fun crearSesion(materia: String, titulo: String): String?
    fun subirChunk(sid: String, i: Int, pista: Int, file: File): Int
    fun marcar(sid: String, t: Int): Int
    fun terminar(sid: String): Int
}

/**
 * Disk queue under `cola/`. One folder per session; inside it:
 *  - `NNNNNN-P.m4a`   a finished 10 s segment (index N, track P). `.part` files are still being recorded and are never touched.
 *  - `NNNNNN.mark`    a bookmark; the file holds the second.
 *  - `.terminar`      the recording ended: call /terminar once everything else is up.
 *  - `.activa`        the recorder is still writing here.
 *  - `meta.txt`/`sid.txt` (folders named `pend-*`) materia + titulo to create the session, then the id it got.
 * A file is deleted only after the server answered 200, so nothing is lost if the phone is offline or the process dies.
 */
class UploadQueue(
    val root: File,
    private val uploader: NotasUploader,
    private val maxAttempts: Int = 6,
    private val baseDelayMs: Long = 1000,
    private val sleeper: (Long) -> Unit = { Thread.sleep(it) },
) {
    private val lock = Any()

    /** Everything still to send: segments, bookmarks, pending /terminar calls and sessions not created yet. */
    fun pendingCount(): Int = sessionDirs().sumOf { d ->
        val n = d.listFiles().orEmpty().count { it.name.endsWith(".m4a") || it.name.endsWith(".mark") || it.name == TERMINAR }
        if (n == 0 && sidOf(d) == null) 1 else n
    }

    /** The folder of a session that already has its server id. */
    fun sessionDir(sid: String): File = File(root, sid).also { it.mkdirs() }

    fun newSessionDir(): File = File(root, PEND + System.currentTimeMillis()).also { it.mkdirs() }

    /** After a crash nobody is recording any more: close the sessions that were left "active" so they get sent. */
    fun clearStale() {
        for (d in sessionDirs()) {
            val a = File(d, ACTIVA)
            if (a.exists()) { a.delete(); File(d, TERMINAR).writeText("1") }
            d.listFiles().orEmpty().filter { it.name.endsWith(".part") }.forEach { it.delete() }
        }
    }

    /** True when the whole drain finished. Each request is tried up to maxAttempts times with exponential backoff. */
    fun drain(onProgress: () -> Unit = {}): Boolean = synchronized(lock) {
        for (dir in sessionDirs()) {
            val sid = sidOf(dir) ?: createRemote(dir) ?: return false
            if (!drainSession(dir, sid, onProgress)) return false
        }
        true
    }

    /** The server id of a session: the folder name, or `sid.txt` inside a `pend-*` folder once it was created. */
    private fun sidOf(dir: File): String? =
        if (!dir.name.startsWith(PEND)) dir.name else File(dir, SID).takeIf { it.exists() }?.readText()?.trim()?.ifBlank { null }

    // The folder keeps its name (the recorder is writing inside it); only sid.txt records the id.
    private fun createRemote(dir: File): String? {
        val meta = File(dir, "meta.txt").takeIf { it.exists() }?.readLines().orEmpty()
        val sid = withRetry { uploader.crearSesion(meta.getOrElse(0) { "" }, meta.getOrElse(1) { "" }) }
        if (sid.isNullOrBlank()) return null
        File(dir, SID).writeText(sid)
        return sid
    }

    private fun drainSession(dir: File, sid: String, onProgress: () -> Unit): Boolean {
        val files = dir.listFiles().orEmpty()
        for (f in files.filter { it.name.endsWith(".m4a") }.sortedBy { it.name }) {
            val parsed = parseChunk(f.name)
            if (parsed == null) { f.delete(); continue }
            if (withRetry { uploader.subirChunk(sid, parsed.first, parsed.second, f) } != 200) return false
            f.delete(); onProgress()
        }
        for (m in files.filter { it.name.endsWith(".mark") }.sortedBy { it.name }) {
            val t = m.readText().trim().toIntOrNull()
            if (t == null) { m.delete(); continue }
            if (withRetry { uploader.marcar(sid, t) } != 200) return false
            m.delete(); onProgress()
        }
        // .activa: the recorder is still writing here, do not close the session under it
        if (File(dir, TERMINAR).exists() && !File(dir, ACTIVA).exists()) {
            if (withRetry { uploader.terminar(sid) } != 200) return false
            onProgress()
            dir.deleteRecursively()
        }
        return true
    }

    /** First success (HTTP 200 or a session id) or the last failed result once every attempt is used. */
    private fun <T> withRetry(call: () -> T?): T? {
        var last: T? = null
        for (attempt in 0 until maxAttempts) {
            if (attempt > 0) sleeper(baseDelayMs shl (attempt - 1))
            last = try { call() } catch (_: IOException) { null }
            if (last == 200 || last is String) return last
        }
        return last
    }

    private fun sessionDirs(): List<File> =
        root.listFiles { f -> f.isDirectory }.orEmpty().sortedWith(compareBy({ it.lastModified() }, { it.name }))

    companion object {
        const val TERMINAR = ".terminar"
        const val PEND = "pend-"
        const val ACTIVA = ".activa"
        const val SID = "sid.txt"
        fun chunkName(i: Int, pista: Int) = "%06d-%d.m4a".format(i, pista)
        fun parseChunk(name: String): Pair<Int, Int>? {
            val m = Regex("""(\d+)-(\d)\.m4a""").matchEntire(name) ?: return null
            return m.groupValues[1].toInt() to m.groupValues[2].toInt()
        }
    }
}
