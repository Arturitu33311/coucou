package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.agents.AgentKinds
import com.agupta07505.smartisland.coucou.agents.AgentModel
import com.agupta07505.smartisland.coucou.agents.RemoteMsg
import com.agupta07505.smartisland.coucou.agents.RemoteProtocol
import org.junit.Assert.assertEquals
import org.junit.Test

class AgentKindsTest {
    @Test fun `the phone reads which agent each event comes from, with the laptop's rule`() {
        val e = RemoteProtocol.parse("""{"t":"event","event":"PreToolUse","session":"s","project":"p","tool":"bash","agent":"opencode"}""", 0) as RemoteMsg.Event
        assertEquals("opencode", e.agent)
        val c = RemoteProtocol.parse("""{"t":"event","event":"Stop","session":"s","project":"p"}""", 0) as RemoteMsg.Event
        assertEquals("", c.agent)
        for (bad in listOf("claude", "Bad Name", "Antigravity", "a/b", "abcdefghijklmnopqrstuvwxyz")) assertEquals(bad, "", AgentKinds.valid(bad))
    }

    @Test fun `names and colours as on the PC, and the header for a mix`() {
        assertEquals("Claude Code", AgentKinds.of("").name)
        assertEquals("Antigravity", AgentKinds.of("antigravity").name)
        assertEquals("OpenCode", AgentKinds.of("opencode").name)
        assertEquals("Foo", AgentKinds.of("foo").name)
        assertEquals("OpenCode", AgentKinds.title(listOf("opencode", "opencode")))
        assertEquals("Agentes", AgentKinds.title(listOf("", "antigravity")))
        assertEquals(AgentKinds.of("antigravity").color, AgentKinds.byName("Antigravity").color)
        assertEquals(AgentKinds.of("").color, AgentKinds.byName("Something else").color)
    }

    @Test fun `a session keeps its agent through its events`() {
        val m = AgentModel()
        m.apply(RemoteProtocol.parse("""{"t":"event","event":"UserPromptSubmit","session":"s","project":"p","agent":"antigravity"}""", 0)!!, 0)
        m.apply(RemoteProtocol.parse("""{"t":"event","event":"PreToolUse","session":"s","project":"p","tool":"run"}""", 1)!!, 1)
        assertEquals("antigravity", m.view(2).single().agent)
    }
}
