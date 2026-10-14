/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import java.io.File
import java.io.InputStream
import java.security.MessageDigest

/** A model file this app may download: where from, how big, and the SHA-256 it must have. */
data class ModelSpec(val url: String, val fileName: String, val sizeBytes: Long, val sha256: String)

object GemmaModel {
    /** Gemma-4-E2B for LiteRT-LM (public repo, no licence gate). The size and hash are the Hugging Face LFS values. */
    val spec = ModelSpec(
        url = "https://huggingface.co/litert-community/gemma-4-E2B-it-litert-lm/resolve/main/gemma-4-E2B-it.litertlm",
        fileName = "gemma-4-E2B-it.litertlm",
        sizeBytes = 2_588_147_712L,
        sha256 = "181938105e0eefd105961417e8da75903eacda102c4fce9ce90f50b97139a63c",
    )
}

sealed interface ModelState {
    data object NotDownloaded : ModelState
    data class Partial(val bytes: Long, val total: Long) : ModelState
    data object Ready : ModelState
}

/**
 * The model file in the app's private folder. It is written as `<name>.part`, checked against the pinned
 * SHA-256 and only then renamed: a half or corrupt file is never loaded by the engine.
 */
class ModelStore(private val dir: File, private val spec: ModelSpec) {
    private val finalFile get() = File(dir, spec.fileName)
    private val partFile get() = File(dir, spec.fileName + ".part")

    fun modelFile(): File = finalFile

    fun state(): ModelState = when {
        finalFile.exists() && finalFile.length() == spec.sizeBytes -> ModelState.Ready
        partFile.exists() -> ModelState.Partial(partFile.length(), spec.sizeBytes)
        else -> ModelState.NotDownloaded
    }

    /**
     * Downloads (or continues) the model. [open] gives a stream that starts at the byte offset it is asked
     * for (an HTTP Range request); [onProgress] gets the bytes on disk so far. Throws [ChatException] when
     * the finished file does not have the pinned hash (the bad file is deleted).
     */
    fun download(open: (from: Long) -> InputStream, onProgress: (Long) -> Unit) {
        dir.mkdirs()
        // One writer at a time, across threads and processes: two downloaders appending to the same `.part`
        // interleave their bytes and the file never matches its hash. The second one is refused, not queued.
        java.io.RandomAccessFile(File(dir, spec.fileName + ".lock"), "rw").use { raf ->
            val lock = try {
                raf.channel.tryLock()
            } catch (e: java.nio.channels.OverlappingFileLockException) {
                null
            }
            if (lock == null) throw ChatException("Ya hay una descarga en curso")
            try {
                downloadLocked(open, onProgress)
            } finally {
                runCatching { lock.release() }
            }
        }
    }

    private fun downloadLocked(open: (from: Long) -> InputStream, onProgress: (Long) -> Unit) {
        dir.mkdirs()
        if (finalFile.exists() && finalFile.length() != spec.sizeBytes) finalFile.delete()
        var have = if (partFile.exists()) partFile.length() else 0L
        if (have > spec.sizeBytes) {
            partFile.delete()
            have = 0L
        }
        if (have < spec.sizeBytes) {
            open(have).use { input ->
                java.io.FileOutputStream(partFile, true).use { out ->
                    val buf = ByteArray(256 * 1024)
                    var total = have
                    while (true) {
                        val n = input.read(buf)
                        if (n < 0) break
                        out.write(buf, 0, n)
                        total += n
                        onProgress(total)
                    }
                }
            }
            // A connection that is closed early reads as "end of file", not as an error. It is not a finished
            // download: the bytes stay, and the next try continues from them (never hash a short file).
            val got = partFile.length()
            if (got < spec.sizeBytes) {
                throw java.io.IOException("Descarga incompleta ($got de ${spec.sizeBytes} bytes)")
            }
        }
        val digest = MessageDigest.getInstance("SHA-256")
        partFile.inputStream().use { s ->
            val buf = ByteArray(1024 * 1024)
            while (true) {
                val n = s.read(buf)
                if (n < 0) break
                digest.update(buf, 0, n)
            }
        }
        val actual = digest.digest().joinToString("") { "%02x".format(it) }
        if (!actual.equals(spec.sha256, ignoreCase = true)) {
            val size = partFile.length()
            partFile.delete()
            throw ChatException(
                "La huella del modelo no coincide (obtenida ${actual.take(12)}…, ${size} bytes de ${spec.sizeBytes}); descarga descartada",
            )
        }
        if (finalFile.exists()) finalFile.delete()
        check(partFile.renameTo(finalFile)) { "No se pudo guardar el modelo" }
    }

    fun delete() {
        finalFile.delete()
        partFile.delete()
    }
}

enum class DownloadOutcome { Done, Interrupted, Paused, Rejected }

/**
 * Runs download attempts until one finishes. The network gate is asked before EVERY attempt, not once:
 * a retry loop that only checked at the start would keep pulling 2.6 GB after the phone left Wi-Fi.
 * Returns the outcome and a human reason when it is not [DownloadOutcome.Done].
 */
suspend fun runDownloadAttempts(
    maxTries: Int,
    networkOk: () -> Boolean,
    attempt: () -> Unit,
    onRetry: (attemptNumber: Int, cause: Exception) -> Unit,
    pause: suspend (Long) -> Unit,
): Pair<DownloadOutcome, String?> {
    var last: String? = null
    for (n in 1..maxTries) {
        if (!networkOk()) return DownloadOutcome.Paused to "Descarga en pausa: sin Wi-Fi. Continúa donde quedó al volver."
        try {
            attempt()
            return DownloadOutcome.Done to null
        } catch (e: ChatException) {
            return DownloadOutcome.Rejected to (e.message ?: "El archivo del modelo no es válido.")
        } catch (e: Exception) {
            last = "Descarga interrumpida (${e.message ?: e.javaClass.simpleName})"
            onRetry(n, e)
            if (n < maxTries) pause(RETRY_PAUSE_MS)
        }
    }
    return DownloadOutcome.Interrupted to "${last ?: "Descarga interrumpida"}; vuelve a tocar Descargar para continuar."
}

private const val RETRY_PAUSE_MS = 3_000L

/** What the on-device model is given: a short system text, the last few turns, and the user's line. */
data class LocalPromptBuilt(val system: String, val history: List<ChatLine>, val user: String)

/**
 * A 2B model on a phone reads its prompt at about a hundred tokens a second, so the persona is cut to a
 * few thousand characters and only the last turns are sent: a long prompt would cost more than the answer.
 */
object LocalPrompt {
    const val MAX_SYSTEM_CHARS = 3_000
    const val MAX_HISTORY = 6
    private const val SEP = "\n\n---\n\n"

    fun build(persona: Persona, context: List<ChatLine>): LocalPromptBuilt {
        val idx = context.indexOfLast { !it.fromJinx }
        if (idx < 0) throw ChatException("No hay mensaje tuyo al que contestar")
        val user = context[idx].text
        val history = context.subList(0, idx).takeLast(MAX_HISTORY)
        val core = persona.core.trim().take(MAX_SYSTEM_CHARS)
        val room = MAX_SYSTEM_CHARS - core.length - SEP.length
        val system = if (persona.memory.isNotBlank() && room > 0) core + SEP + persona.memory.trim().take(room) else core
        return LocalPromptBuilt(system, history, user)
    }
}

/** When the loaded engine (about 3 GB of RAM) is given back. */
object IdlePolicy {
    const val IDLE_MS = 5 * 60_000L
    fun shouldRelease(lastUsed: Long, now: Long): Boolean = now < lastUsed || now - lastUsed > IDLE_MS
}

/** The on-device engine, behind an interface so the brain is tried without the native library. */
interface LlmRuntime {
    /** [history] pairs are (isJinx, text). Returns the model's full answer. */
    fun generate(system: String, history: List<Pair<Boolean, String>>, user: String, maxTokens: Int): String
    fun close()
}

/**
 * Jinx answered by the model that lives in this phone. Slower and plainer than the cloud, but it works with no
 * internet and no other device. The engine is created at the first use and released after being idle.
 */
class LocalModelBrain(
    private val modelReady: () -> Boolean,
    private val persona: () -> Persona,
    private val factory: () -> LlmRuntime,
    private val now: () -> Long = { android.os.SystemClock.elapsedRealtime() },
) : LocalBrain {
    override val via = Via.Local

    private var runtime: LlmRuntime? = null
    private var lastUsed = 0L

    @Synchronized
    override fun reply(context: List<ChatLine>): String {
        if (!modelReady()) throw ChatException("El modelo local no está descargado en este teléfono")
        val built = LocalPrompt.build(persona(), context)
        val t = now()
        var rt = runtime
        if (rt != null && IdlePolicy.shouldRelease(lastUsed, t)) {
            release()
            rt = null
        }
        try {
            if (rt == null) {
                rt = factory()
                runtime = rt
            }
            val text = rt.generate(
                built.system,
                built.history.map { it.fromJinx to it.text },
                built.user,
                MAX_TOKENS,
            ).trim()
            lastUsed = now()
            if (text.isEmpty()) throw ChatException("El modelo local no devolvió texto")
            return text
        } catch (e: ChatException) {
            throw e
        } catch (e: Exception) {
            release()
            throw ChatException("Falló el modelo local: ${e.message ?: e.javaClass.simpleName}")
        }
    }

    @Synchronized
    fun release() {
        runCatching { runtime?.close() }
        runtime = null
    }

    /** Called from time to time (panel closed, app idle): gives the memory back if it has not been used. */
    @Synchronized
    fun releaseIfIdle() {
        if (runtime != null && IdlePolicy.shouldRelease(lastUsed, now())) release()
    }

    private companion object {
        const val MAX_TOKENS = 400
    }
}
