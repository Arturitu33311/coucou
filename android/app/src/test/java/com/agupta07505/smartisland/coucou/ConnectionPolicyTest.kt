package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ConnectionPolicy
import com.agupta07505.smartisland.coucou.phone.SshProfile
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ConnectionPolicyTest {
    private val p = SshProfile(host = "100.102.136.23", port = 22, user = "alan", hostKey = "SHA256:abc")
    private val t0 = 1_000_000L

    @Test fun freshAndSameProfileIsReusable() {
        assertTrue(ConnectionPolicy.reusable(p, p, openedAt = t0, lastUsed = t0, now = t0 + 10_000))
    }

    @Test fun anyChangeOfWhoWeTrustOrWhereDropsTheConnection() {
        for (other in listOf(
            p.copy(host = "100.1.1.1"), p.copy(port = 2222), p.copy(user = "root"), p.copy(hostKey = "SHA256:other"),
        )) {
            assertFalse(ConnectionPolicy.reusable(p, other, t0, t0, t0 + 1_000))
        }
    }

    @Test fun unpairedProfileIsNeverReused() {
        val unpaired = p.copy(hostKey = "")
        assertFalse(ConnectionPolicy.reusable(unpaired, unpaired, t0, t0, t0 + 1_000))
    }

    @Test fun idleLongerThanTheLimitDropsIt() {
        assertFalse(ConnectionPolicy.reusable(p, p, t0, lastUsed = t0, now = t0 + ConnectionPolicy.IDLE_MS + 1))
        assertTrue(ConnectionPolicy.reusable(p, p, t0, lastUsed = t0, now = t0 + ConnectionPolicy.IDLE_MS - 1))
    }

    @Test fun aClockThatWentBackwardsNeverKeepsAConnectionAlive() {
        // now is before it was opened / last used: the age cannot be trusted, so it is not reusable
        assertFalse(ConnectionPolicy.reusable(p, p, openedAt = t0, lastUsed = t0, now = t0 - 1))
        assertFalse(ConnectionPolicy.reusable(p, p, openedAt = t0 - 500, lastUsed = t0 + 10_000, now = t0))
    }

    @Test fun olderThanTheMaximumAgeDropsItEvenIfBusy() {
        val now = t0 + ConnectionPolicy.MAX_AGE_MS + 1
        assertFalse(ConnectionPolicy.reusable(p, p, openedAt = t0, lastUsed = now - 1, now = now))
    }
}
