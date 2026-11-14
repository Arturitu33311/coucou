package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.agents.AgentModel
import com.agupta07505.smartisland.coucou.agents.AgentUi
import com.agupta07505.smartisland.coucou.agents.RemoteMsg
import com.agupta07505.smartisland.coucou.agents.RemoteProtocol
import com.agupta07505.smartisland.coucou.ui.ApprovalReview
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Jinx's permission requests from the server (`jinx-aprobaciones`), in the lines coucou_aprobaciones.py writes. */
class JinxLinkTest {
    private fun parse(line: String, now: Long = 1_000) = RemoteProtocol.parse(line, now)

    private val approval = """{"t":"approval","request_id":"3108430c6b554afd8600abdb65dc3ca1","session":"jinx-366e2a1f","project":"Jinx","agent":"hermes","tool":"recursive delete","detail":"rm -rf /tmp/a","timeout_s":114}"""

    @Test
    fun a_request_from_jinx_is_read_and_tagged_hermes() {
        val p = (parse(approval) as RemoteMsg.Request).pending
        assertEquals("3108430c6b554afd8600abdb65dc3ca1", p.id)
        assertEquals("hermes", p.agent)
        assertEquals("Jinx", p.project)
        assertEquals(114, p.timeoutS)
        assertFalse(p.isQuestion)
    }

    @Test
    fun hello_replaces_the_list_and_resolved_removes_one() {
        val m = AgentModel()
        m.apply(parse("""{"t":"hello","pending":[$approval]}""")!!, 1_000)
        assertEquals(1, m.pending.size)
        // a new link starts from what the server says, not from what the phone remembered
        m.apply(parse("""{"t":"hello","pending":[]}""")!!, 2_000)
        assertEquals(0, m.pending.size)
        m.apply(parse(approval)!!, 3_000)
        m.apply(parse("""{"t":"resolved","request_id":"3108430c6b554afd8600abdb65dc3ca1","decision":"gateway"}""")!!, 3_500)
        assertEquals(0, m.pending.size)
    }

    @Test
    fun a_request_nobody_resolved_is_dropped_by_the_phone_after_its_own_limit() {
        val m = AgentModel()
        m.apply(parse(approval, now = 0)!!, 0)
        assertEquals(1, m.view(100_000).let { m.pending.size })
        m.view((114 + AgentModel.PENDING_GRACE_S) * 1000L + 1)
        assertEquals("never shown beyond the time the server allows", 0, m.pending.size)
    }

    @Test
    fun the_server_cut_command_is_recognised_so_it_cannot_be_allowed_here() {
        val cut = "echo " + "x".repeat(494) + "…"           // 500 characters ending in the cut mark, as the server sends
        assertEquals(500, cut.length)
        assertTrue(ApprovalReview.shortenedByLaptop(cut))
        assertFalse("a long command that was not cut can be read in full and allowed", ApprovalReview.shortenedByLaptop("echo " + "x".repeat(450)))
    }

    @Test
    fun the_island_ignores_an_allow_tap_right_after_a_request_drops() {
        assertTrue(ApprovalReview.tooSoon(shownAt = 10_000, now = 10_599))
        assertFalse(ApprovalReview.tooSoon(shownAt = 10_000, now = 10_600))
        assertTrue("a clock that went backwards is still 'too soon', never an allow", ApprovalReview.tooSoon(shownAt = 10_000, now = 9_000))
    }

    @Test
    fun a_system_notification_is_wanted_only_for_jinx_requests_while_the_phone_sleeps() {
        val j = (parse(approval) as RemoteMsg.Request).pending
        val laptop = (parse(approval.replace("\"agent\":\"hermes\",", ""), 1_000) as RemoteMsg.Request).pending
        assertTrue(com.agupta07505.smartisland.coucou.agents.RequestNotifier.wanted(j, asleep = true))
        assertFalse("the island is enough while the phone is in use", com.agupta07505.smartisland.coucou.agents.RequestNotifier.wanted(j, asleep = false))
        assertFalse("Claude Code's requests keep their old behaviour", com.agupta07505.smartisland.coucou.agents.RequestNotifier.wanted(laptop, asleep = true))
    }

    @Test
    fun both_links_share_one_view_and_the_laptop_is_the_headline() {
        val j = (parse(approval, 5_000) as RemoteMsg.Request).pending
        val l = (parse(approval.replace("3108430c6b554afd8600abdb65dc3ca1", "9-1").replace("\"agent\":\"hermes\",", ""), 1_000) as RemoteMsg.Request).pending
        val laptop = AgentUi(AgentUi.Status.Connected, "", emptyList(), listOf(l))
        val jinx = AgentUi(AgentUi.Status.Connected, "", emptyList(), listOf(j))
        val merged = CoucouRuntime.mergeUi(laptop, jinx)
        assertEquals(listOf("9-1", j.id), merged.pending.map { it.id })       // oldest first: its time runs out first
        assertEquals(AgentUi.Status.Connected, merged.status)
        // the laptop not paired must not hide Jinx's link
        val onlyJinx = CoucouRuntime.mergeUi(AgentUi(AgentUi.Status.Off, "Pair the laptop"), jinx)
        assertEquals(AgentUi.Status.Connected, onlyJinx.status)
        assertEquals(1, onlyJinx.pending.size)
        // and Jinx's link being down must not hide the laptop's
        assertEquals(AgentUi.Status.Connected, CoucouRuntime.mergeUi(laptop, AgentUi(AgentUi.Status.Reconnecting)).status)
        assertNull(parse("""{"t":"approval","agent":"hermes"}"""))
    }
}
