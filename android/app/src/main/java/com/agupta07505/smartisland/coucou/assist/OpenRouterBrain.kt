/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/** POST [body] as JSON with a bearer [key]; returns the response body or throws. */
fun httpPost(url: String, key: String, body: String): String {
    val c = URL(url).openConnection() as HttpURLConnection
    try {
        c.requestMethod = "POST"
        c.connectTimeout = 8_000
        c.readTimeout = 60_000
        c.doOutput = true
        c.setRequestProperty("Content-Type", "application/json")
        if (key.isNotEmpty()) c.setRequestProperty("Authorization", "Bearer $key")
        c.outputStream.use { it.write(body.toByteArray()) }
        if (c.responseCode !in 200..299) throw java.io.IOException("HTTP ${c.responseCode}")
        return c.inputStream.bufferedReader().readText()
    } finally {
        c.disconnect()
    }
}

/** Jinx answered by OpenRouter straight from the phone: same models the laptop's fallback uses. */
class OpenRouterBrain(
    private val secrets: Secrets,
    private val persona: () -> Persona,
    private val post: (url: String, key: String, body: String) -> String = ::httpPost,
) : LocalBrain {
    override val via = Via.Cloud

    override fun reply(context: List<ChatLine>): String {
        val key = secrets.get(SecretNames.OPENROUTER)?.takeIf { it.isNotBlank() }
            ?: throw ChatException("sin llave de OpenRouter en este teléfono")
        var last = "OpenRouter no respondió"
        for (model in MODELS) {
            val body = JSONObject()
                .put("model", model)
                .put("max_tokens", MAX_TOKENS)
                .put("messages", JSONArray().apply {
                    put(JSONObject().put("role", "system").put("content", persona().systemPrompt()))
                    context.forEach { put(JSONObject().put("role", if (it.fromJinx) "assistant" else "user").put("content", it.text)) }
                })
                .toString()
            try {
                val text = JSONObject(post(URL, key, body))
                    .getJSONArray("choices").getJSONObject(0).getJSONObject("message").getString("content").trim()
                if (text.isNotEmpty()) return text
                last = "respuesta vacía"
            } catch (e: Exception) {
                last = e.message ?: last
            }
        }
        throw ChatException(last)
    }

    private companion object {
        const val URL = "https://openrouter.ai/api/v1/chat/completions"
        const val MAX_TOKENS = 400
        val MODELS = listOf("deepseek/deepseek-v4-flash", "google/gemini-2.5-flash-lite")
    }
}
