package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatEvent
import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.ChatProtocol
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class ChatProtocolTest {
    @Test fun sendRequestCarriesTextCidAndSrc() {
        val o = JSONObject(ChatProtocol.send("hola\n\"x\" 😀", "c1", "s22"))
        assertEquals("send", o.getString("op"))
        assertEquals("hola\n\"x\" 😀", o.getString("text"))
        assertEquals("c1", o.getString("cid"))
        assertEquals("s22", o.getString("src"))
    }

    @Test fun parseReadsEventsAndSkipsGarbageLines() {
        val raw = """
            {"ev":"ack"}
            not json
            {"ev":"done","mode":"or","text":"qué onda"}
        """.trimIndent()
        val ev = ChatProtocol.parse(raw)
        assertEquals(listOf(ChatEvent.Ack, ChatEvent.Done("or", "qué onda")), ev)
    }

    @Test fun doneCarriesTheMirrorConfirmationAndDefaultsToTrueForOldGateways() {
        val withFlag = ChatProtocol.parse("""{"ev":"done","mode":"or","text":"x","mirrored":false}""").single()
        assertEquals(ChatEvent.Done("or", "x", false), withFlag)
        val old = ChatProtocol.parse("""{"ev":"done","mode":"or","text":"x"}""").single()
        assertEquals(ChatEvent.Done("or", "x", true), old)
    }

    @Test fun doneCarriesJinxsMoodOnlyWhenItIsOneOfMochisEmotes() {
        val ok = ChatProtocol.parse("""{"ev":"done","mode":"hermes","text":"x","mood":"laughing"}""").single()
        assertEquals(ChatEvent.Done("hermes", "x", true, "laughing"), ok)
        for (bad in listOf("\"mood\":\"furious\"", "\"mood\":\"Happy\"", "\"mood\":3", "\"mood\":null", "\"mood\":\"\"")) {
            val ev = ChatProtocol.parse("""{"ev":"done","mode":"hermes","text":"x",$bad}""").single() as ChatEvent.Done
            assertEquals(bad, null, ev.mood)
        }
        assertEquals(null, (ChatProtocol.parse("""{"ev":"done","mode":"or","text":"x"}""").single() as ChatEvent.Done).mood)
    }

    @Test fun aThreadLineKeepsItsMoodThroughTheWireAndNeverInventsOne() {
        val line = ChatLine("c:jinx", 1.0, "jinx", "hola", "c", true, mood = "love")
        val back = ChatProtocol.lineFromJson(ChatProtocol.lineToJson(line))
        assertEquals("love", back!!.mood)
        assertEquals(line, back)
        // No mood: the key is not even written (the wire format is the old one).
        val plain = ChatLine("d:jinx", 2.0, "jinx", "adiós", "d", true)
        assertEquals(false, ChatProtocol.lineToJson(plain).has("mood"))
        assertEquals(null, ChatProtocol.lineFromJson(JSONObject("""{"id":"e","ts":3,"src":"jinx","text":"x","mood":"nope"}"""))!!.mood)
        assertEquals(null, ChatProtocol.lineFromJson(JSONObject("""{"id":"e","ts":3,"src":"jinx","text":"x","mood":null}"""))!!.mood)
    }

    @Test fun expectDoneReturnsTheAnswer() {
        val done = ChatProtocol.expectDone("{\"ev\":\"ack\"}\n{\"ev\":\"done\",\"mode\":\"cached\",\"text\":\"x\"}")
        assertEquals("x", done.text)
    }

    @Test fun expectDoneThrowsTheGatewaysError() {
        try {
            ChatProtocol.expectDone("{\"ev\":\"error\",\"msg\":\"modelo caído\"}")
            fail("should throw")
        } catch (e: ChatException) {
            assertTrue(e.message!!.contains("modelo caído"))
        }
    }

    @Test fun expectDoneThrowsOnEmptyOutput() {
        try {
            ChatProtocol.expectDone("")
            fail("should throw")
        } catch (e: ChatException) {
            assertTrue(e.message!!.isNotBlank())
        }
    }

    @Test fun lineRoundTrips() {
        val l = ChatLine("c1:s21", 12.5, "s21", "hola", "c1", false)
        assertEquals(l, ChatProtocol.lineFromJson(ChatProtocol.lineToJson(l)))
    }

    @Test fun mergedAndSecretsParse() {
        val m = ChatProtocol.parse(
            """{"ev":"merged","added":1,"items":[{"id":"a:tg","ts":1,"src":"tg","text":"t","cid":null,"mirrored":true}],"mirrored_ids":["z:s21"]}""",
        ).single() as ChatEvent.Merged
        assertEquals(1, m.added)
        assertEquals("t", m.items.single().text)
        assertEquals(listOf("z:s21"), m.mirroredIds)
        val s = ChatProtocol.parse("""{"ev":"secrets","openrouter_key":"k","telegram_token":"t","chat":"42"}""").single()
        assertEquals(ChatEvent.Secrets("k", "t", "42"), s)
    }
}
