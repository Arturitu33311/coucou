package com.agupta07505.smartisland.coucou.notas

import org.json.JSONObject
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

data class ClaseAhora(val materia: String, val fin: String?, val salon: String?)

/** HTTP client of the notes server (Tailscale). Blocking: call it from a background thread. */
class NotasApi(private val base: String = BASE) : NotasUploader {

    fun materias(): List<String> {
        val arr = JSONObject(getText("$base/api/config")).optJSONArray("materias") ?: return emptyList()
        return (0 until arr.length()).mapNotNull { i ->
            // the list may hold plain names or objects carrying a name
            when (val v = arr.get(i)) {
                is String -> v
                is JSONObject -> v.optString("nombre", v.optString("materia", "")).ifBlank { null }
                else -> null
            }
        }
    }

    fun claseAhora(): ClaseAhora? {
        val o = JSONObject(getText("$base/api/clase-ahora"))
        val m = o.optString("materia", "")
        if (o.isNull("materia") || m.isBlank()) return null
        return ClaseAhora(m, o.optString("fin").ifBlank { null }, o.optString("salon").ifBlank { null })
    }

    override fun crearSesion(materia: String, titulo: String): String? {
        val body = JSONObject().put("materia", materia).put("titulo", titulo).toString().toByteArray()
        val (code, text) = post("$base/api/sesiones", body, "application/json")
        return if (code == 200 || code == 201) JSONObject(text).optString("id").ifBlank { null } else null
    }

    override fun subirChunk(sid: String, i: Int, pista: Int, file: File): Int =
        post("$base/api/sesiones/$sid/chunk?i=$i&pista=$pista", file.readBytes(), "application/octet-stream").first

    override fun marcar(sid: String, t: Int): Int =
        post("$base/api/sesiones/$sid/marcar", JSONObject().put("t", t).toString().toByteArray(), "application/json").first

    override fun terminar(sid: String): Int =
        post("$base/api/sesiones/$sid/terminar", ByteArray(0), "application/json").first

    private fun getText(url: String): String {
        val c = URL(url).openConnection() as HttpURLConnection
        try {
            c.connectTimeout = 8000
            c.readTimeout = 8000
            if (c.responseCode != 200) throw IOException("HTTP " + c.responseCode)
            return c.inputStream.bufferedReader().use { it.readText() }
        } finally {
            c.disconnect()
        }
    }

    private fun post(url: String, body: ByteArray, type: String): Pair<Int, String> {
        val c = URL(url).openConnection() as HttpURLConnection
        try {
            c.requestMethod = "POST"
            c.connectTimeout = 10000
            c.readTimeout = 30000
            c.doOutput = true
            c.setRequestProperty("Content-Type", type)
            c.setFixedLengthStreamingMode(body.size)
            c.outputStream.use { it.write(body) }
            val code = c.responseCode
            val stream = if (code in 200..299) c.inputStream else c.errorStream
            return code to (stream?.bufferedReader()?.use { it.readText() } ?: "")
        } finally {
            c.disconnect()
        }
    }

    companion object {
        const val BASE = "https://server.tail3dac6b.ts.net:5281"
    }
}
