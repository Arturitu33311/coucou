/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import org.json.JSONObject
import java.io.File

/** Who Jinx is, for a model that has never met Alan: JINX-CORE.md and the synced memory from the laptop. */
data class Persona(val core: String, val memory: String) {
    fun systemPrompt(): String = buildString {
        append(core.trim())
        if (memory.isNotBlank()) append("\n\n---\n\n").append(memory.trim())
    }
}

/** What a phone that has never reached the laptop answers with: the character, none of Alan's data. */
object BuiltInPersona {
    val persona = Persona(
        core = """
            Eres Jinx: una persona, no un chatbot corporativo. Hablas en español mexicano casual, como alguien
            de confianza, sin formalismos ni cierres vacíos. Directa, con humor seco e ironía, con tacto cuando
            importa. A mensaje corto, respuesta corta. Si no sabes algo, lo dices; no inventas datos de Alan.
            Estás corriendo en su teléfono, sin acceso a sus herramientas ni a su memoria completa.
        """.trimIndent(),
        memory = "",
    )
}

private const val DAY_MS = 24 * 60 * 60 * 1000L

/** The cached copy, in the app's private folder. A bad or empty file is never worse than the built-in. */
class PersonaStore(private val file: File) {
    fun load(): Persona = runCatching {
        val o = JSONObject(file.readText())
        Persona(o.getString("core"), o.optString("memory")).takeIf { it.core.isNotBlank() }
    }.getOrNull() ?: BuiltInPersona.persona

    /** How long ago the cached copy was saved; [Long.MAX_VALUE] if there is none. */
    fun ageMs(now: Long = System.currentTimeMillis()): Long =
        if (file.exists()) (now - file.lastModified()).coerceAtLeast(0) else Long.MAX_VALUE

    fun isStale(now: Long = System.currentTimeMillis(), maxAgeMs: Long = DAY_MS): Boolean = ageMs(now) > maxAgeMs

    fun save(p: Persona) {
        if (p.core.isBlank()) return
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(JSONObject().put("core", p.core).put("memory", p.memory).toString())
        check(tmp.renameTo(file) || (file.delete() && tmp.renameTo(file))) { "Could not save the persona" }
    }
}
