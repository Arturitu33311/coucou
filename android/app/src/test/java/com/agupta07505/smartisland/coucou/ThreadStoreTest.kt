package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.ThreadStore
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class ThreadStoreTest {
    @get:Rule val tmp = TemporaryFolder()
    private fun store() = ThreadStore(File(tmp.root, "t.jsonl"))
    private fun line(id: String, ts: Double, src: String = "s21", cid: String? = null, mirrored: Boolean = false) =
        ChatLine(id, ts, src, "txt-$id", cid, mirrored)

    @Test fun appendIsIdempotentById() {
        val s = store()
        s.append(line("c1:s21", 1.0, cid = "c1"))
        s.append(line("c1:s21", 2.0, cid = "c1"))
        assertEquals(1, s.all().size)
    }

    @Test fun answerForAndPending() {
        val s = store()
        s.append(line("c1:s21", 1.0, cid = "c1"))
        s.append(line("c2:s21", 2.0, cid = "c2"))
        s.append(line("c1:jinx", 3.0, src = "jinx", cid = "c1"))
        assertNotNull(s.answerFor("c1"))
        assertNull(s.answerFor("c2"))
        assertEquals(listOf("c2:s21"), s.pending().map { it.id })
    }

    @Test fun linesFromThePcOrTelegramAreNeverAnsweredHere() {
        val s = store()
        s.append(line("p1:pc", 1.0, src = "pc", cid = "p1"))
        s.append(line("t1:tg", 2.0, src = "tg", cid = "t1"))
        s.append(line("c3:s21", 3.0, cid = "c3"))
        assertEquals(listOf("c3:s21"), s.pending().map { it.id })
    }

    @Test fun eachSourceHasItsLabelAndItsTelegramPrefix() {
        assertEquals("PC", line("a", 1.0, src = "pc").sourceLabel)
        assertEquals("Telegram", line("a", 1.0, src = "tg").sourceLabel)
        assertNull(line("a", 1.0, src = "s22").sourceLabel)
        assertEquals("💻 ", line("a", 1.0, src = "pc").mirrorPrefix)
        assertEquals("📱 ", line("a", 1.0, src = "s21").mirrorPrefix)
        assertEquals("", line("a", 1.0, src = "jinx").mirrorPrefix)
        assertEquals("⚠️ Respaldo (Hermes no contestó): ", line("a", 1.0, src = "jinx").copy(via = "or").mirrorPrefix)
    }

    @Test fun mergeAddsOnlyUnknownAndOrdersByTs() {
        val s = store()
        s.append(line("b", 20.0))
        val added = s.merge(listOf(line("a", 10.0), line("b", 20.0)))
        assertEquals(1, added)
        assertEquals(listOf("a", "b"), s.all().map { it.id })
    }

    @Test fun unmirroredAndMark() {
        val s = store()
        s.append(line("a", 1.0))
        s.append(line("b", 2.0, mirrored = true))
        assertEquals(listOf("a"), s.unmirrored().map { it.id })
        s.markMirrored(setOf("a"))
        assertEquals(emptyList<String>(), s.unmirrored().map { it.id })
    }

    @Test fun tailReturnsTheLastN() {
        val s = store()
        for (i in 1..5) s.append(line("m$i", i.toDouble()))
        assertEquals(listOf("m4", "m5"), s.tail(2).map { it.id })
    }

    @Test fun survivesACorruptFileAndKeepsAtMost2000Lines() {
        val f = File(tmp.root, "t.jsonl")
        f.writeText("{bad\n")
        val s = ThreadStore(f)
        assertEquals(0, s.all().size)
        for (i in 1..2100) s.append(line("m$i", i.toDouble()))
        assertEquals(2000, s.all().size)
        assertEquals("m2100", s.all().last().id)
    }
}
