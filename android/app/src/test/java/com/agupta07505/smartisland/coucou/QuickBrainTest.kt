package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.QuickBrain
import com.agupta07505.smartisland.coucou.assist.SecretNames
import com.agupta07505.smartisland.coucou.assist.Secrets
import com.agupta07505.smartisland.coucou.assist.Via
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

private class QuickSecrets(vararg pairs: Pair<String, String>) : Secrets {
    private val m = pairs.toMap().toMutableMap()
    override fun get(name: String) = m[name]
    override fun put(name: String, value: String) { m[name] = value }
}

class QuickBrainTest {
    private val ctx = listOf(
        ChatLine("a:jinx", 1.0, "jinx", "dime", null, true),
        ChatLine("c1:s22", 2.0, "s22", "apaga la tele", "c1", false),
    )
    private val key = SecretNames.G2_KEY

    private fun brain(
        secrets: Secrets = QuickSecrets(key to "K"),
        url: String = "https://server.example.ts.net:8898/",
        post: (String, String, String) -> String = { _, _, _ -> """{"status":"done","text":" Tele apagada. "}""" },
        get: (String, String) -> String = { _, _ -> error("no se esperaba un GET") },
    ) = QuickBrain(secrets, { url }, post, get)

    private fun failsWith(b: QuickBrain): String =
        try { b.reply(ctx); fail("debía fallar"); "" } catch (e: ChatException) { e.message.orEmpty() }

    @Test fun `it asks the bridge in lite mode with the last thing the user said and the key`() {
        var seen = Triple("", "", "")
        val text = brain(post = { u, k, b -> seen = Triple(u, k, b); """{"status":"done","text":"Listo."}""" }).reply(ctx)
        assertEquals("Listo.", text)
        assertEquals("https://server.example.ts.net:8898/api/g2/ask", seen.first)
        assertEquals("K", seen.second)
        val body = JSONObject(seen.third)
        assertEquals("apaga la tele", body.getString("text"))
        assertEquals("lite", body.getString("mode"))
        assertEquals("c1", body.getString("cid")) // the same line sent twice is answered once
    }

    @Test fun `the answer is trimmed and its via is the quick one`() {
        assertEquals("Tele apagada.", brain().reply(ctx))
        assertEquals(Via.Quick, brain().via)
    }

    @Test fun `a pending answer is asked again by its id`() {
        val gets = mutableListOf<String>()
        val text = brain(
            post = { _, _, _ -> """{"status":"pending","id":"c1"}""" },
            get = { u, _ -> gets += u; """{"status":"done","text":"Ya está."}""" },
        ).reply(ctx)
        assertEquals("Ya está.", text)
        assertEquals(listOf("https://server.example.ts.net:8898/api/g2/answer?id=c1&wait=20"), gets)
    }

    @Test fun `it gives up when she keeps thinking, so the next way answers`() {
        val msg = failsWith(brain(
            post = { _, _, _ -> """{"status":"pending","id":"c1"}""" },
            get = { _, _ -> """{"status":"pending","id":"c1"}""" },
        ))
        assertTrue(msg, msg.contains("sigue pensando"))
    }

    @Test fun `an error, an empty answer, a network failure, no key and no address all fall through`() {
        assertTrue(failsWith(brain(post = { _, _, _ -> """{"status":"error","error":"llave"}""" })).contains("llave"))
        assertTrue(failsWith(brain(post = { _, _, _ -> """{"status":"done","text":"  "}""" })).contains("vacía"))
        assertEquals("sin red", failsWith(brain(post = { _, _, _ -> throw java.io.IOException("sin red") })))
        assertTrue(failsWith(brain(secrets = QuickSecrets())).contains("sin llave"))
        assertTrue(failsWith(brain(url = "")).contains("sin dirección"))
        assertTrue(failsWith(brain(url = "ftp://x")).contains("https"))
    }

    @Test fun `the key is never sent in the clear`() {
        var called = false
        val msg = failsWith(brain(url = "http://100.1.1.1:8898", post = { _, _, _ -> called = true; "{}" }))
        assertTrue(msg, msg.contains("https"))
        assertTrue("nothing may be sent over http", !called)
    }

    @Test fun `with no line from the user there is nothing to answer`() {
        try { brain().reply(listOf(ChatLine("a:jinx", 1.0, "jinx", "hola", null, true))); fail("debía fallar") }
        catch (e: ChatException) { assertTrue(e.message!!.contains("sin mensaje")) }
    }
}
