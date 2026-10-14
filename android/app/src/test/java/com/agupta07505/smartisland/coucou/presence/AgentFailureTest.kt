package com.agupta07505.smartisland.coucou.presence

import com.agupta07505.smartisland.coucou.agents.AgentModel
import com.agupta07505.smartisland.coucou.agents.RemoteMsg
import org.junit.Assert.assertEquals
import org.junit.Test

/** A failed tool is what Mochi winces at; other events leave it alone. */
class AgentFailureTest {
    private fun event(name: String) = RemoteMsg.Event(name, "s1", "coucou", "Bash", "ls", "")

    @Test
    fun a_new_prompt_is_thinking_until_its_first_tool() {
        val m = AgentModel()
        m.apply(event("PreToolUse"), 100L)
        assertEquals("Bash", m.sessions["s1"]?.tool)
        // The prompt clears the last tool: no tool yet means it is thinking (violet dots, as on the laptop).
        m.apply(RemoteMsg.Event("UserPromptSubmit", "s1", "coucou", "", "", ""), 200L)
        assertEquals("", m.sessions["s1"]?.tool)
        assertEquals(com.agupta07505.smartisland.coucou.agents.SessionState.Working, m.sessions["s1"]?.state)
        m.apply(event("PreToolUse"), 300L)
        assertEquals("Bash", m.sessions["s1"]?.tool)
    }

    @Test
    fun only_a_failed_tool_stamps_the_failure() {
        val m = AgentModel()
        m.apply(event("PreToolUse"), 100L)
        m.apply(event("PostToolUse"), 200L)
        assertEquals(0L, m.lastFailureAt)
        m.apply(event("PostToolUseFailure"), 300L)
        assertEquals(300L, m.lastFailureAt)
        m.apply(event("Stop"), 400L)
        assertEquals(300L, m.lastFailureAt)
    }
}
