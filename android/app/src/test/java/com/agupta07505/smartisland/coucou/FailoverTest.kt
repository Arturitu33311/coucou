package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.Failover
import com.agupta07505.smartisland.coucou.assist.GatewayUnreachable
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class FailoverTest {
    private var t = 0L
    private val f = Failover { t }
    private var serverCalls = 0
    private var laptopCalls = 0
    private val ok = """{"ev":"done","mode":"hermes","text":"hola"}"""

    private fun server(result: () -> String): () -> String = { serverCalls++; result() }
    private val laptop: () -> String = { laptopCalls++; """{"ev":"done","mode":"or","text":"desde la laptop"}""" }

    @Test fun `the server answers, the laptop is not asked`() {
        assertEquals(ok, f.ask("send", server { ok }, laptop))
        assertEquals(0, laptopCalls)
        assertFalse(f.onFallback)
    }

    @Test fun `a server out of reach sends to the laptop and is skipped for a while`() {
        f.ask("send", server { throw GatewayUnreachable("no route") }, laptop)
        assertEquals(1, laptopCalls)
        assertTrue(f.onFallback)
        t += Failover.DOWN_MS - 1
        f.ask("history", server { ok }, laptop)
        assertEquals(1, serverCalls)  // not asked again yet
        t += 1
        assertEquals(ok, f.ask("history", server { ok }, laptop))
        assertEquals(2, serverCalls)
    }

    @Test fun `a server without the gateway yet is skipped longer`() {
        f.ask("history", server { """{"ok":false,"error":"op desconocida"}""" }, laptop)
        assertEquals(1, laptopCalls)
        t += Failover.DOWN_MS
        f.ask("history", server { ok }, laptop)
        assertEquals(1, serverCalls)
        t += Failover.NO_GATEWAY_MS
        f.ask("history", server { ok }, laptop)
        assertEquals(2, serverCalls)
    }

    @Test fun `the gateway's own error is the answer`() {
        val err = """{"ev":"error","msg":"src desconocido"}"""
        assertEquals(err, f.ask("send", server { err }, laptop))
        assertEquals(0, laptopCalls)
    }

    @Test fun `a send that broke after reaching the server is not sent again to the laptop`() {
        try {
            f.ask("send", server { throw ChatException("timeout") }, laptop)
            fail("expected the error")
        } catch (e: ChatException) {
            assertEquals("timeout", e.message)
        }
        assertEquals(0, laptopCalls)
        // A read is safe to ask elsewhere.
        f.ask("history", server { throw ChatException("timeout") }, laptop)
        assertEquals(1, laptopCalls)
    }

    @Test fun `what counts as the gateway's output`() {
        assertTrue(Failover.isGatewayOutput("""{"ev":"ack"}""" + "\n" + ok))
        assertFalse(Failover.isGatewayOutput("""{"ok":true,"tasks":[]}"""))
        assertFalse(Failover.isGatewayOutput("bash: jinx-chat: command not found"))
        assertFalse(Failover.isGatewayOutput(""))
    }
}
