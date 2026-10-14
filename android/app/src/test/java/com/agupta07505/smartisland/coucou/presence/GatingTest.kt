package com.agupta07505.smartisland.coucou.presence

import com.agupta07505.smartisland.model.IslandMode
import com.agupta07505.smartisland.model.IslandNotification
import com.agupta07505.smartisland.model.isCoucou
import com.agupta07505.smartisland.ui.shownOnThisDevice
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Only the painting follows who is in use: the other device keeps every card, it just does not draw Coucou's. */
class GatingTest {
    private fun n(key: String, mode: IslandMode) =
        IslandNotification(key = key, packageName = "p", appName = "a", title = key, text = "", timeMillis = 1L, mode = mode)

    private val all = listOf(
        n("desk", IslandMode.Desk), n("ask", IslandMode.Approval), n("q", IslandMode.Question),
        n("agent", IslandMode.Agent), n("jinx", IslandMode.JinxMsg), n("rem", IslandMode.Reminder),
        n("music", IslandMode.Music), n("sms", IslandMode.Notification), n("bat", IslandMode.Battery),
    )

    @Test
    fun the_device_in_use_draws_everything() {
        assertEquals(all, shownOnThisDevice(true, all))
    }

    @Test
    fun the_other_device_draws_everything_but_coucou() {
        val shown = shownOnThisDevice(false, all)
        assertEquals(listOf("music", "sms", "bat"), shown.map { it.key })
        assertTrue(shown.none { it.mode.isCoucou })
    }

    @Test
    fun gating_a_list_does_not_alter_the_notifications() {
        val before = all.map { it.copy() }
        shownOnThisDevice(false, all)
        assertEquals(before, all)
    }
}
