package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.BuiltInPersona
import com.agupta07505.smartisland.coucou.assist.Persona
import com.agupta07505.smartisland.coucou.assist.PersonaStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class PersonaStoreTest {
    @get:Rule val tmp = TemporaryFolder()

    @Test fun neverSyncedGivesTheBuiltInPersona() {
        val p = PersonaStore(File(tmp.root, "p.json")).load()
        assertEquals(BuiltInPersona.persona, p)
        assertTrue(p.systemPrompt().contains("Jinx"))
    }

    @Test fun savedPersonaIsReadBackAndSurvivesCorruption() {
        val f = File(tmp.root, "p.json")
        val s = PersonaStore(f)
        s.save(Persona("CORE ñ\n\"x\"", "MEM"))
        assertEquals(Persona("CORE ñ\n\"x\"", "MEM"), s.load())
        f.writeText("{broken")
        assertEquals(BuiltInPersona.persona, s.load())
    }

    @Test fun emptyCoreFromTheLaptopDoesNotReplaceTheCachedOne() {
        val s = PersonaStore(File(tmp.root, "p.json"))
        s.save(Persona("BUENA", "m"))
        s.save(Persona("   ", "otra"))
        assertEquals("BUENA", s.load().core)
    }
}
