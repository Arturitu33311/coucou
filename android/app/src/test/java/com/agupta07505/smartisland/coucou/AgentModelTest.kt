package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.agents.AgentModel
import com.agupta07505.smartisland.coucou.agents.RemoteMsg
import com.agupta07505.smartisland.coucou.agents.RemoteProtocol
import com.agupta07505.smartisland.coucou.agents.SessionState
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The same lines remote.rs writes, so the two ends stay in step. */
class AgentModelTest {
    private fun parse(line: String, now: Long = 1_000) = RemoteProtocol.parse(line, now)

    private fun ev(event: String, tool: String = "", detail: String = "", session: String = "s1", project: String = "coucou", message: String = "") =
        RemoteMsg.Event(event, session, project, tool, detail, message)

    @Test
    fun the_lines_the_laptop_writes_are_understood() {
        assertEquals(RemoteMsg.Pong, parse("""{"t":"pong"}"""))
        assertEquals(RemoteMsg.Offline, parse("""{"t":"offline"}"""))
        assertEquals(RemoteMsg.Disabled, parse("""{"t":"disabled"}"""))
        assertEquals(RemoteMsg.Resolved("9-1", "allow"), parse("""{"t":"resolved","request_id":"9-1","decision":"allow"}"""))
        val ack = parse("""{"t":"ack","ok":false,"why":"no such permission request is waiting"}""") as RemoteMsg.Ack
        assertTrue(!ack.ok && ack.id == null && ack.why!!.startsWith("no such"))
        val e = parse("""{"t":"event","event":"PreToolUse","session":"s1","project":"coucou","tool":"Bash","detail":"ls"}""") as RemoteMsg.Event
        assertEquals(listOf("PreToolUse", "s1", "coucou", "Bash", "ls"), listOf(e.event, e.session, e.project, e.tool, e.detail))
        val a = (parse("""{"t":"approval","request_id":"9-2","session":"s1","project":"p","tool":"Bash","detail":"rm -rf x","timeout_s":108}""") as RemoteMsg.Request).pending
        assertEquals(listOf("9-2", "Bash", "rm -rf x"), listOf(a.id, a.tool, a.detail))
        assertTrue(!a.isQuestion && a.timeoutS == 108)
        assertNull("a request without an id is no use", parse("""{"t":"approval","tool":"Bash"}"""))
        assertNull(parse("not json"))
        assertNull(parse("""{"t":"somethingnew"}"""))
    }

    @Test
    fun a_question_arrives_with_its_options() {
        val q = (parse(
            """{"t":"question","request_id":"9-3","session":"s","project":"p","timeout_s":120,"questions":[
              {"question":"¿Cuál?","header":"Plan","multi":true,"options":[{"label":"A","description":"primera"},{"label":"B","description":""}]}]}""",
        ) as RemoteMsg.Request).pending
        assertTrue(q.isQuestion)
        assertEquals("¿Cuál?", q.questions[0].question)
        assertTrue(q.questions[0].multi)
        assertEquals(listOf("A", "B"), q.questions[0].options.map { it.label })
    }

    @Test
    fun a_session_goes_from_working_to_idle_and_asks_for_you() {
        val m = AgentModel()
        m.apply(ev("SessionStart"), 1_000)
        assertEquals(SessionState.Idle, m.view(1_000).single().state)
        m.apply(ev("PreToolUse", "Bash", "cargo test"), 2_000)
        val w = m.view(2_000).single()
        assertEquals(listOf(SessionState.Working, "Bash", "cargo test"), listOf(w.state, w.tool, w.detail))
        m.apply(ev("Notification", message = "Claude needs your permission"), 3_000)
        assertEquals(SessionState.NeedsYou, m.view(3_000).single().state)
        m.apply(ev("Stop"), 4_000)
        assertEquals(SessionState.Idle, m.view(4_000).single().state)
        m.apply(ev("SessionEnd"), 5_000)
        assertTrue(m.view(5_000).isEmpty())
    }

    @Test
    fun waiting_for_input_is_not_asking_but_a_permission_is() {
        val m = AgentModel()
        m.apply(ev("PreToolUse", "Bash", "x"), 1_000)
        m.apply(ev("Notification", message = "Claude is waiting for your input"), 2_000)
        assertEquals(SessionState.Idle, m.view(2_000).single().state)
        m.apply(ev("Notification", message = "Claude needs your permission to use Bash"), 3_000)
        assertEquals(SessionState.NeedsYou, m.view(3_000).single().state)
    }

    @Test
    fun a_needs_you_nothing_backs_fades_instead_of_asking_for_ever() {
        val m = AgentModel()
        m.apply(ev("Notification", message = "Claude needs your permission to use Bash"), 0)
        assertEquals(SessionState.NeedsYou, m.view(AgentModel.ATTENTION_MS - 1).single().state)
        assertEquals(SessionState.Idle, m.view(AgentModel.ATTENTION_MS + 1).single().state)
        // …but one a waiting request backs stays until it is resolved.
        val backed = AgentModel()
        val req = parse("""{"t":"approval","request_id":"r1","session":"s1","project":"p","tool":"Bash","detail":"x","timeout_s":100000}""", 0)!!
        backed.apply(req, 0)
        assertEquals(SessionState.NeedsYou, backed.view(AgentModel.ATTENTION_MS + 1).single().state)
    }

    @Test
    fun a_request_the_laptop_gave_up_on_is_dropped_even_if_its_resolved_line_was_lost() {
        val m = AgentModel()
        val req = parse("""{"t":"approval","request_id":"r2","session":"s2","project":"p","tool":"Bash","detail":"x","timeout_s":108}""", 0)!!
        m.apply(req, 0)
        assertNotNull(m.nextPending())
        m.view((108 + AgentModel.PENDING_GRACE_S) * 1000L + 1)
        assertNull(m.nextPending())
    }

    @Test
    fun a_quiet_session_stops_looking_busy_and_an_old_one_is_forgotten() {
        val m = AgentModel()
        m.apply(ev("PreToolUse", "Edit", "a.kt"), 0)
        assertEquals(SessionState.Working, m.view(10_000).single().state)
        assertEquals(SessionState.Idle, m.view(AgentModel.STALE_MS + 1).single().state)
        assertTrue(m.view(AgentModel.FORGET_MS + 1).isEmpty())
    }

    @Test
    fun a_waiting_request_keeps_its_session_asking_until_it_is_resolved_anywhere() {
        val m = AgentModel()
        val req = parse("""{"t":"approval","request_id":"9-4","session":"s9","project":"p","tool":"Bash","detail":"rm x","timeout_s":108}""", 5_000)!!
        m.apply(req, 5_000)
        assertNotNull(m.nextPending())
        assertEquals(SessionState.NeedsYou, m.view(5_000).single { it.session == "s9" }.state)
        m.apply(ev("PreToolUse", "Bash", "ls", session = "s9"), 6_000)
        assertEquals(SessionState.NeedsYou, m.view(6_000).single { it.session == "s9" }.state)
        m.apply(RemoteMsg.Resolved("9-4", "deny"), 7_000)
        assertNull(m.nextPending())
    }

    @Test
    fun a_late_phone_is_told_what_is_still_waiting_and_the_oldest_comes_first() {
        val m = AgentModel()
        val h = parse(
            """{"t":"hello","v":1,"pending":[{"t":"approval","request_id":"a","session":"s","project":"p","tool":"Bash","detail":"x","timeout_s":108}]}""",
            9_000,
        )!!
        m.apply(h, 9_000)
        assertEquals("a", m.nextPending()?.id)
        // A new hello replaces the list (the laptop is the source of truth).
        m.apply(RemoteMsg.Hello(emptyList()), 9_500)
        assertNull(m.nextPending())
        m.apply(RemoteMsg.Request(parse("""{"t":"approval","request_id":"x","session":"s","tool":"T"}""", 10)!!.let { (it as RemoteMsg.Request).pending }), 10)
        m.apply(RemoteMsg.Request(parse("""{"t":"approval","request_id":"y","session":"s","tool":"T"}""", 20)!!.let { (it as RemoteMsg.Request).pending }), 20)
        assertEquals("x", m.nextPending()?.id)
    }

    @Test
    fun what_the_phone_says_back_has_the_shape_the_laptop_wants() {
        val a = JSONObject(RemoteProtocol.answer("9-1", "allow"))
        assertEquals(setOf("op", "request_id", "decision"), a.keys().asSequence().toSet())
        assertEquals("answer", a.getString("op"))
        val q = JSONObject(RemoteProtocol.answerQuestion("9-3", mapOf("¿Cuál?" to listOf("A", "B"), "¿Y?" to listOf("Sí")), setOf("¿Cuál?")))
        assertEquals("answer_question", q.getString("op"))
        assertEquals(listOf("A", "B"), (0 until 2).map { q.getJSONObject("answers").getJSONArray("¿Cuál?").getString(it) })
        assertEquals("Sí", q.getJSONObject("answers").getString("¿Y?"))
        assertEquals("decline", JSONObject(RemoteProtocol.decline("z")).getString("op"))
    }
}
