package com.agupta07505.smartisland.coucou.presence

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A reply of Jinx's goes to the island of the device in use, unless her chat is open there already. */
class IslandEchoTest {
    private fun msg(source: String) = PresenceMsg(1, "hola", null, "info", source, 0)

    @Test fun aReplyIsNotRepeatedOnTheIslandWhileHerChatIsOpen() {
        assertTrue(MochiPresence.alreadyInChat(msg("jinx-chat"), chatVisible = true))
        assertFalse(MochiPresence.alreadyInChat(msg("jinx-chat"), chatVisible = false))
    }

    @Test fun otherNoticesAlwaysShow() {
        assertFalse(MochiPresence.alreadyInChat(msg("agentes"), chatVisible = true))
        assertFalse(MochiPresence.alreadyInChat(msg("jinx"), chatVisible = true))
    }
}
