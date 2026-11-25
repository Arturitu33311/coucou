package com.agupta07505.smartisland.coucou.notas

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.io.IOException

class NotasUploadQueueTest {
    @get:Rule val tmp = TemporaryFolder()

    private class Fake : NotasUploader {
        val log = mutableListOf<String>()
        var failChunks = 0          // how many chunk calls fail before the server answers
        var chunkCode = 200
        var down = false
        var crear: String? = "S1"
        override fun crearSesion(materia: String, titulo: String): String? { if (down) throw IOException("down"); log += "crear"; return crear }
        override fun subirChunk(sid: String, i: Int, pista: Int, file: File): Int {
            if (down) throw IOException("down")
            if (failChunks > 0) { failChunks--; throw IOException("flaky") }
            log += "chunk:$sid:$i:$pista"; return chunkCode
        }
        override fun marcar(sid: String, t: Int): Int { if (down) throw IOException("down"); log += "marca:$sid:$t"; return 200 }
        override fun terminar(sid: String): Int { if (down) throw IOException("down"); log += "terminar:$sid"; return 200 }
    }

    private fun queue(f: Fake, sleeps: MutableList<Long> = mutableListOf()) =
        UploadQueue(tmp.newFolder("cola"), f, sleeper = { sleeps += it })

    private fun session(q: UploadQueue, sid: String = "S1", vararg names: String): File {
        val d = q.sessionDir(sid)
        names.forEach { File(d, it).writeText("x") }
        return d
    }

    @Test fun uploadsInOrderAndDeletesOnlyOn200() {
        val f = Fake(); val q = queue(f)
        val d = session(q, "S1", UploadQueue.chunkName(1, 1), UploadQueue.chunkName(0, 1), UploadQueue.chunkName(0, 2))
        File(d, "000001.mark").writeText("42")
        File(d, UploadQueue.TERMINAR).writeText("1")
        assertEquals(5, q.pendingCount())
        assertTrue(q.drain())
        assertEquals(listOf("chunk:S1:0:1", "chunk:S1:0:2", "chunk:S1:1:1", "marca:S1:42", "terminar:S1"), f.log)
        assertEquals(0, q.pendingCount())
        assertFalse(d.exists())
    }

    @Test fun retriesWithExponentialBackoffThenSucceeds() {
        val f = Fake().apply { failChunks = 3 }; val sleeps = mutableListOf<Long>()
        val q = queue(f, sleeps)
        session(q, "S1", UploadQueue.chunkName(0, 1))
        assertTrue(q.drain())
        assertEquals(listOf(1000L, 2000L, 4000L), sleeps)
        assertEquals(0, q.pendingCount())
    }

    @Test fun sixAttemptsThenKeepsTheFileOnDisk() {
        val f = Fake().apply { down = true }; val sleeps = mutableListOf<Long>()
        val q = queue(f, sleeps)
        val d = session(q, "S1", UploadQueue.chunkName(0, 1))
        assertFalse(q.drain())
        assertEquals(listOf(1000L, 2000L, 4000L, 8000L, 16000L), sleeps) // 6 attempts = 5 waits
        assertTrue(File(d, UploadQueue.chunkName(0, 1)).exists())
        assertEquals(1, q.pendingCount())
        // the network is back: the same queue finishes the job
        f.down = false
        assertTrue(q.drain())
        assertEquals(0, q.pendingCount())
    }

    @Test fun nonOkStatusIsNotDeleted() {
        val f = Fake().apply { chunkCode = 500 }; val q = queue(f)
        val d = session(q, "S1", UploadQueue.chunkName(0, 1))
        assertFalse(q.drain())
        assertTrue(File(d, UploadQueue.chunkName(0, 1)).exists())
    }

    @Test fun doesNotCloseASessionThatIsStillRecording() {
        val f = Fake(); val q = queue(f)
        val d = session(q, "S1", UploadQueue.chunkName(0, 1), UploadQueue.TERMINAR, UploadQueue.ACTIVA)
        assertTrue(q.drain())
        assertEquals(listOf("chunk:S1:0:1"), f.log) // no terminar yet
        File(d, UploadQueue.ACTIVA).delete()
        assertTrue(q.drain())
        assertEquals("terminar:S1", f.log.last())
    }

    @Test fun sessionCreatedLaterWhenRecordingStartedOffline() {
        val f = Fake().apply { down = true }; val q = queue(f)
        val d = q.newSessionDir()
        File(d, "meta.txt").writeText("Fisica\nClase\n")
        File(d, UploadQueue.chunkName(0, 1)).writeText("x")
        assertFalse(q.drain())
        assertTrue(q.pendingCount() > 0)
        f.down = false
        assertTrue(q.drain())
        assertEquals(listOf("crear", "chunk:S1:0:1"), f.log)
        assertEquals("S1", File(d, UploadQueue.SID).readText())
    }

    @Test fun parsesChunkNames() {
        assertEquals(12 to 2, UploadQueue.parseChunk(UploadQueue.chunkName(12, 2)))
        assertEquals(null, UploadQueue.parseChunk("000001-1.m4a.part"))
    }
}
