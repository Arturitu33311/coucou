package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.Persona
import com.agupta07505.smartisland.coucou.assist.PersonaStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class PersonaFreshnessTest {
    @get:Rule val tmp = TemporaryFolder()

    @Test fun neverSavedIsStale() {
        val s = PersonaStore(File(tmp.root, "p.json"))
        assertTrue(s.isStale(now = 1_000_000L, maxAgeMs = 3_600_000L))
    }

    @Test fun recentlySavedIsFreshAndOldIsStale() {
        val f = File(tmp.root, "p.json")
        val s = PersonaStore(f)
        s.save(Persona("CORE", "m"))
        f.setLastModified(1_000_000L)
        assertFalse(s.isStale(now = 1_000_000L + 60_000L, maxAgeMs = 3_600_000L))
        assertTrue(s.isStale(now = 1_000_000L + 3_700_000L, maxAgeMs = 3_600_000L))
        assertEquals(60_000L, s.ageMs(now = 1_000_000L + 60_000L))
    }
}
