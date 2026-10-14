package com.agupta07505.smartisland.coucou.presence

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The inbox key is only sent where it cannot be read on the way. */
class KeyTransportTest {
    @Test
    fun private_networks_and_https_may_carry_it() {
        for (ok in listOf(
            "https://presence.example.com", "HTTPS://x.y", "http://100.92.254.125:8897", "http://100.64.0.1",
            "http://100.127.255.254", "http://10.0.2.2:8897", "http://192.168.1.20", "http://172.16.0.5",
            "http://172.31.9.9", "http://127.0.0.1:8897", "http://localhost:8897", "http://server.tail1234.ts.net",
        )) assertTrue(ok, PresenceNet.mayCarryKey(ok))
    }

    @Test
    fun plain_http_to_a_public_address_may_not() {
        for (no in listOf(
            "http://example.com", "http://8.8.8.8:80", "http://100.128.0.1", "http://100.63.255.1",
            "http://172.32.0.1", "http://192.169.0.1", "ftp://10.0.0.1", "10.0.0.1", "", "http://",
        )) assertFalse(no, PresenceNet.mayCarryKey(no))
    }
}
