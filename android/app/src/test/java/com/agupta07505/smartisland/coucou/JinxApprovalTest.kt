package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatEvent
import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.ChatProtocol
import com.agupta07505.smartisland.coucou.assist.JinxApproval
import com.agupta07505.smartisland.coucou.assist.JinxApprovalReview
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** Jinx's questions (the server's `approvals` / `approve` ops) as the phone reads and answers them. */
class JinxApprovalTest {
    private val a = JinxApproval("run_abc", "r1", "rm -rf /tmp/x", "delete files", listOf("once", "session", "always", "deny"), 10.0)

    @Test fun approvalsAreReadWithWhatHermesOffers() {
        val raw = """{"ev":"approvals","items":[{"run_id":"run_abc","request_id":"r1","command":"rm -rf /tmp/x","description":"delete files","choices":["once","session","always","deny"],"ts":10.0},{"request_id":"no run"},{"run_id":"run_2","choices":["bogus"]}]}"""
        val ev = ChatProtocol.parse(raw).single() as ChatEvent.Approvals
        assertEquals(a, ev.items[0])
        // Without a run id it cannot be answered; with no known choice it is once/deny.
        assertEquals(2, ev.items.size)
        assertEquals(listOf("once", "deny"), ev.items[1].choices)
    }

    @Test fun anApprovalCarriesOnlyItsIdsAndTheChoice() {
        val o = JSONObject(ChatProtocol.approve(a, "once"))
        assertEquals("approve", o.getString("op"))
        assertEquals("run_abc", o.getString("run_id"))
        assertEquals("r1", o.getString("request_id"))
        assertEquals("once", o.getString("choice"))
        assertEquals(4, o.length())
    }

    @Test fun theServersRefusalIsAnError() {
        try {
            ChatProtocol.expect<ChatEvent.Approved>("""{"ev":"error","msg":"esa aprobación ya no está pendiente"}""")
            fail()
        } catch (e: ChatException) {
            assertEquals("esa aprobación ya no está pendiente", e.message)
        }
        assertEquals(ChatEvent.Approved("once"), ChatProtocol.expect<ChatEvent.Approved>("""{"ev":"approved","choice":"once"}"""))
    }

    @Test fun thePhoneNeverOffersAlways() {
        assertEquals(listOf("once", "session", "deny"), JinxApprovalReview.buttons(a))
        assertEquals(listOf("once", "deny"), JinxApprovalReview.buttons(a.copy(choices = listOf("once", "deny"))))
    }

    @Test fun aCommandTheServerCutCannotBeAllowedHere() {
        assertFalse(JinxApprovalReview.cut("ls"))
        assertFalse(JinxApprovalReview.cut("x".repeat(500) + "…"))
        assertTrue(JinxApprovalReview.cut("x".repeat(JinxApprovalReview.SERVER_LIMIT - 1) + "…"))
    }
}
