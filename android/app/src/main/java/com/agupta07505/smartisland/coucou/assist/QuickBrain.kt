/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/** GET with a bearer [key]; returns the response body or throws. */
fun httpGet(url: String, key: String): String {
    val c = URL(url).openConnection() as HttpURLConnection
    try {
        c.requestMethod = "GET"
        c.connectTimeout = 8_000
        c.readTimeout = 40_000
        if (key.isNotEmpty()) c.setRequestProperty("Authorization", "Bearer $key")
        if (c.responseCode !in 200..299) throw java.io.IOException("HTTP ${c.responseCode}")
        return c.inputStream.bufferedReader().readText()
    } finally {
        c.disconnect()
    }
}

/**
 * The same quick Jinx the laptop's « OK Coucou » and the glasses talk to: the bridge on the server (`/api/g2/ask`,
 * mode `lite`). She keeps her persona, runs the house (TV, volume… with the result checked there) and hands to the full
 * Jinx what needs her memory. Nothing of it lives on this phone but the bridge's key, Keystore-wrapped, which only
 * ever travels over https.
 * It goes first; if it cannot answer, the repository falls back to the laptop's gateway and the phone's own brains.
 */
class QuickBrain(
    private val secrets: Secrets,
    private val baseUrl: () -> String,
    private val post: (url: String, key: String, body: String) -> String = ::httpPost,
    private val get: (url: String, key: String) -> String = ::httpGet,
) : LocalBrain {
    override val via = Via.Quick

    override fun reply(context: List<ChatLine>): String {
        val key = secrets.get(SecretNames.G2_KEY)?.takeIf { it.isNotBlank() }
            ?: throw ChatException("sin llave de la Jinx rápida en este teléfono")
        val address = baseUrl().trim().trimEnd('/')
        if (address.isEmpty()) throw ChatException("sin dirección de la Jinx rápida")
        // The key goes in a header: only over TLS (Tailscale Serve gives the bridge an https name), never in the clear.
        val base = address.takeIf { it.startsWith("https://") }
            ?: throw ChatException("la dirección de la Jinx rápida debe ser https")
        val last = context.lastOrNull { !it.fromJinx } ?: throw ChatException("sin mensaje que contestar")
        val body = JSONObject().put("text", last.text.take(MAX_TEXT)).put("mode", "lite")
            .apply { last.cid?.let { put("cid", it) } }.toString()
        var answer = try {
            JSONObject(post("$base/api/g2/ask", key, body))
        } catch (e: Exception) {
            throw ChatException(e.message ?: "la Jinx rápida no respondió")
        }
        repeat(POLLS) {
            when (answer.optString("status")) {
                "done" -> return answer.optString("text").trim().ifEmpty { throw ChatException("respuesta vacía") }
                "error" -> throw ChatException(answer.optString("error").ifEmpty { "la Jinx rápida falló" })
            }
            val id = answer.optString("id").takeIf { it.isNotEmpty() } ?: throw ChatException("respuesta sin id")
            answer = try {
                JSONObject(get("$base/api/g2/answer?id=$id&wait=$WAIT_S", key))
            } catch (e: Exception) {
                throw ChatException(e.message ?: "la Jinx rápida no respondió")
            }
        }
        if (answer.optString("status") == "done") return answer.optString("text").trim()
        throw ChatException("la Jinx rápida sigue pensando")
    }

    private companion object {
        const val MAX_TEXT = 1_000
        const val POLLS = 2
        const val WAIT_S = 20
    }
}
