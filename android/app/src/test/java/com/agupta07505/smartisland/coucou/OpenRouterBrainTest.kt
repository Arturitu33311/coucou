package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.OpenRouterBrain
import com.agupta07505.smartisland.coucou.assist.Persona
import com.agupta07505.smartisland.coucou.assist.SecretNames
import com.agupta07505.smartisland.coucou.assist.Secrets
import com.agupta07505.smartisland.coucou.assist.TelegramMirror
import com.agupta07505.smartisland.coucou.assist.Via
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

private class MemSecrets(vararg pairs: Pair<String, String>) : Secrets {
    private val m = pairs.toMap().toMutableMap()
    override fun get(name: String) = m[name]
    override fun put(name: String, value: String) { m[name] = value }
}

class OpenRouterBrainTest {
    private val persona = Persona("SOY JINX", "MEMORIA")
    private val ctx = listOf(
        ChatLine("a:tg", 1.0, "tg", "hola", null, true),
        ChatLine("b:jinx", 2.0, "jinx", "qué onda", null, true),
        ChatLine("c:s21", 3.0, "s21", "cuéntame", "c", false),
    )

    @Test fun buildsSystemPlusRoleMappedMessagesAndParsesTheAnswer() {
        var body = ""; var key = ""
        val brain = OpenRouterBrain(MemSecrets(SecretNames.OPENROUTER to "KEY"), { persona }) { _, k, b ->
            key = k; body = b
            """{"choices":[{"message":{"content":"  aquí estoy  "}}]}"""
        }
        assertEquals("aquí estoy", brain.reply(ctx))
        assertEquals("KEY", key)
        val msgs = JSONObject(body).getJSONArray("messages")
        assertEquals("system", msgs.getJSONObject(0).getString("role"))
        assertTrue(msgs.getJSONObject(0).getString("content").contains("SOY JINX"))
        assertEquals(listOf("user", "assistant", "user"), (1..3).map { msgs.getJSONObject(it).getString("role") })
        assertEquals(400, JSONObject(body).getInt("max_tokens"))
        assertEquals(Via.Cloud, brain.via)
    }

    @Test fun noKeyIsAChatException() {
        val brain = OpenRouterBrain(MemSecrets(), { persona }) { _, _, _ -> fail("no request without a key"); "" }
        try { brain.reply(ctx); fail() } catch (e: ChatException) { assertTrue(e.message!!.isNotBlank()) }
    }

    @Test fun networkOrBadJsonIsAChatExceptionNotACrash() {
        val down = OpenRouterBrain(MemSecrets(SecretNames.OPENROUTER to "K"), { persona }) { _, _, _ -> throw java.io.IOException("sin red") }
        try { down.reply(ctx); fail() } catch (e: ChatException) { /* ok */ }
        val bad = OpenRouterBrain(MemSecrets(SecretNames.OPENROUTER to "K"), { persona }) { _, _, _ -> "<html>" }
        try { bad.reply(ctx); fail() } catch (e: ChatException) { /* ok */ }
    }

    @Test fun triesTheFallbackModelWhenTheFirstFails() {
        val models = mutableListOf<String>()
        val brain = OpenRouterBrain(MemSecrets(SecretNames.OPENROUTER to "K"), { persona }) { _, _, b ->
            val m = JSONObject(b).getString("model"); models += m
            if (models.size == 1) throw java.io.IOException("429") else """{"choices":[{"message":{"content":"ok"}}]}"""
        }
        assertEquals("ok", brain.reply(ctx))
        assertEquals(2, models.size)
        assertFalse(models[0] == models[1])
    }
}

class TelegramMirrorTest {
    @Test fun postsToTheBotWithTheChat() {
        var url = ""; var body = ""
        val m = TelegramMirror(MemSecrets(SecretNames.TG_TOKEN to "TOK", SecretNames.TG_CHAT to "42")) { u, _, b -> url = u; body = b; """{"ok":true}""" }
        assertTrue(m.deliver("📱 hola"))
        assertEquals("https://api.telegram.org/botTOK/sendMessage", url)
        assertEquals("42", JSONObject(body).getString("chat_id"))
    }

    @Test fun noSecretsOrNoNetworkIsFalse() {
        assertFalse(TelegramMirror(MemSecrets()) { _, _, _ -> """{"ok":true}""" }.deliver("x"))
        assertFalse(TelegramMirror(MemSecrets(SecretNames.TG_TOKEN to "T", SecretNames.TG_CHAT to "1")) { _, _, _ -> throw java.io.IOException() }.deliver("x"))
        assertFalse(TelegramMirror(MemSecrets(SecretNames.TG_TOKEN to "T", SecretNames.TG_CHAT to "1")) { _, _, _ -> """{"ok":false}""" }.deliver("x"))
    }
}
