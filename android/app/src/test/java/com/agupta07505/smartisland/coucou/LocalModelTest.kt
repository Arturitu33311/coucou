package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.DownloadOutcome
import com.agupta07505.smartisland.coucou.assist.IdlePolicy
import com.agupta07505.smartisland.coucou.assist.runDownloadAttempts
import com.agupta07505.smartisland.coucou.assist.LlmRuntime
import com.agupta07505.smartisland.coucou.assist.LocalModelBrain
import com.agupta07505.smartisland.coucou.assist.LocalPrompt
import com.agupta07505.smartisland.coucou.assist.ModelSpec
import com.agupta07505.smartisland.coucou.assist.ModelState
import com.agupta07505.smartisland.coucou.assist.ModelStore
import com.agupta07505.smartisland.coucou.assist.Persona
import com.agupta07505.smartisland.coucou.assist.Via
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayInputStream
import java.io.File
import java.security.MessageDigest

private fun sha256(b: ByteArray) = MessageDigest.getInstance("SHA-256").digest(b).joinToString("") { "%02x".format(it) }

class ModelStoreTest {
    @get:Rule val tmp = TemporaryFolder()
    private val data = ByteArray(1000) { (it % 251).toByte() }
    private fun spec(sha: String = sha256(data), size: Long = data.size.toLong()) = ModelSpec("http://x/m.litertlm", "m.litertlm", size, sha)
    private fun store(spec: ModelSpec = spec()) = ModelStore(File(tmp.root, "models"), spec)

    @Test fun nothingOnDiskIsNotDownloaded() {
        assertEquals(ModelState.NotDownloaded, store().state())
    }

    @Test fun aPartialFileReportsItsProgress() {
        val s = store()
        File(tmp.root, "models").mkdirs()
        File(tmp.root, "models/m.litertlm.part").writeBytes(data.copyOf(400))
        assertEquals(ModelState.Partial(400, 1000), s.state())
    }

    @Test fun downloadWritesThePartThenPromotesItOnlyWhenTheHashMatches() {
        val s = store()
        val seen = mutableListOf<Long>()
        s.download(open = { from -> assertEquals(0L, from); ByteArrayInputStream(data) }, onProgress = { seen += it })
        assertEquals(ModelState.Ready, s.state())
        assertTrue(File(tmp.root, "models/m.litertlm").exists())
        assertFalse(File(tmp.root, "models/m.litertlm.part").exists())
        assertEquals(1000L, seen.last())
    }

    @Test fun aWrongHashIsRejectedAndNothingIsPromoted() {
        val s = store(spec(sha = "00".repeat(32)))
        try {
            s.download(open = { ByteArrayInputStream(data) }, onProgress = {})
            fail("should reject")
        } catch (e: ChatException) {
            assertTrue(e.message!!.contains("huella", ignoreCase = true))
        }
        assertFalse(File(tmp.root, "models/m.litertlm").exists())
        assertFalse(File(tmp.root, "models/m.litertlm.part").exists())   // a bad file is not resumed again
    }

    @Test fun aConnectionThatEndsEarlyKeepsTheBytesAndTheNextTryContinues() {
        val s = store()
        try {
            s.download(open = { ByteArrayInputStream(data.copyOf(700)) }, onProgress = {})
            fail("an incomplete download must not look finished")
        } catch (e: java.io.IOException) {
            assertTrue(e.message!!.contains("incompleta", ignoreCase = true))
        }
        assertEquals(ModelState.Partial(700, 1000), s.state())       // nothing was thrown away
        var asked = -1L
        s.download(open = { from -> asked = from; ByteArrayInputStream(data.copyOfRange(from.toInt(), data.size)) }, onProgress = {})
        assertEquals(700L, asked)
        assertEquals(ModelState.Ready, s.state())
    }

    @Test fun anErrorMidStreamKeepsWhatWasWritten() {
        val s = store()
        val flaky = object : java.io.InputStream() {
            var n = 0
            override fun read(): Int = throw UnsupportedOperationException()
            override fun read(b: ByteArray, off: Int, len: Int): Int {
                if (n >= 400) throw java.io.IOException("Connection reset")
                val k = minOf(len, 400 - n)
                System.arraycopy(data, n, b, off, k)
                n += k
                return k
            }
        }
        try {
            s.download(open = { flaky }, onProgress = {})
            fail("should propagate")
        } catch (e: java.io.IOException) {
            assertTrue(e.message!!.contains("reset", ignoreCase = true))
        }
        assertEquals(ModelState.Partial(400, 1000), s.state())
    }

    @Test fun aSecondDownloadWhileOneRunsIsRefusedAndNeverTouchesThePart() {
        val s = store()
        val firstReading = java.util.concurrent.CountDownLatch(1)
        val letFirstFinish = java.util.concurrent.CountDownLatch(1)
        val slow = object : java.io.InputStream() {
            var sent = 0
            override fun read(): Int = throw UnsupportedOperationException()
            override fun read(b: ByteArray, off: Int, len: Int): Int {
                if (sent == 0) {
                    val k = minOf(len, 500)
                    System.arraycopy(data, 0, b, off, k)
                    sent = k
                    firstReading.countDown()          // the first download now holds the file
                    letFirstFinish.await()
                    return k
                }
                if (sent >= data.size) return -1
                val k = minOf(len, data.size - sent)
                System.arraycopy(data, sent, b, off, k)
                sent += k
                return k
            }
        }
        var firstError: Throwable? = null
        val t = Thread {
            try { s.download(open = { slow }, onProgress = {}) } catch (e: Throwable) { firstError = e }
        }
        t.start()
        assertTrue(firstReading.await(5, java.util.concurrent.TimeUnit.SECONDS))
        try {
            s.download(open = { ByteArrayInputStream(data) }, onProgress = {})
            fail("a second concurrent download must be refused")
        } catch (e: ChatException) {
            assertTrue(e.message!!.contains("en curso", ignoreCase = true))
        }
        letFirstFinish.countDown()
        t.join(5_000)
        assertEquals(null, firstError)
        assertEquals(ModelState.Ready, s.state())          // the file is the first one's, uncorrupted
    }

    @Test fun resumesFromWhatIsAlreadyThere() {
        val s = store()
        File(tmp.root, "models").mkdirs()
        File(tmp.root, "models/m.litertlm.part").writeBytes(data.copyOf(300))
        var asked = -1L
        s.download(open = { from -> asked = from; ByteArrayInputStream(data.copyOfRange(from.toInt(), data.size)) }, onProgress = {})
        assertEquals(300L, asked)
        assertEquals(ModelState.Ready, s.state())
    }

    @Test fun readyFileWithTheWrongSizeIsNotReady() {
        File(tmp.root, "models").mkdirs()
        File(tmp.root, "models/m.litertlm").writeBytes(data.copyOf(10))
        assertEquals(ModelState.NotDownloaded, store().state())
    }
}

class DownloadAttemptsTest {
    @Test fun finishesOnTheFirstSuccess() {
        var calls = 0
        val r = kotlinx.coroutines.runBlocking {
            runDownloadAttempts(5, networkOk = { true }, attempt = { calls++ }, onRetry = { _, _ -> }, pause = {})
        }
        assertEquals(DownloadOutcome.Done, r.first)
        assertEquals(1, calls)
    }

    @Test fun retriesAfterInterruptionsAndThenGivesUpWithTheLastReason() {
        var calls = 0
        val r = kotlinx.coroutines.runBlocking {
            runDownloadAttempts(3, networkOk = { true }, attempt = { calls++; throw java.io.IOException("reset") }, onRetry = { _, _ -> }, pause = {})
        }
        assertEquals(DownloadOutcome.Interrupted, r.first)
        assertEquals(3, calls)
        assertTrue(r.second!!.contains("reset"))
    }

    @Test fun theNetworkIsCheckedBeforeEveryAttemptNotOnlyTheFirst() {
        var calls = 0
        var onWifi = true
        val r = kotlinx.coroutines.runBlocking {
            runDownloadAttempts(
                10,
                networkOk = { onWifi },
                attempt = { calls++; onWifi = false; throw java.io.IOException("cut") },   // the phone leaves Wi-Fi
                onRetry = { _, _ -> },
                pause = {},
            )
        }
        assertEquals(DownloadOutcome.Paused, r.first)
        assertEquals(1, calls)      // it did not spend mobile data on a second attempt
    }

    @Test fun aRejectedFileStopsTheLoop() {
        var calls = 0
        val r = kotlinx.coroutines.runBlocking {
            runDownloadAttempts(5, networkOk = { true }, attempt = { calls++; throw ChatException("huella") }, onRetry = { _, _ -> }, pause = {})
        }
        assertEquals(DownloadOutcome.Rejected, r.first)
        assertEquals(1, calls)
    }

    @Test fun noNetworkAtAllNeverStartsAnAttempt() {
        var calls = 0
        val r = kotlinx.coroutines.runBlocking {
            runDownloadAttempts(5, networkOk = { false }, attempt = { calls++ }, onRetry = { _, _ -> }, pause = {})
        }
        assertEquals(DownloadOutcome.Paused, r.first)
        assertEquals(0, calls)
    }
}

class LocalPromptTest {
    private fun line(i: Int, jinx: Boolean) = ChatLine("id$i", i.toDouble(), if (jinx) "jinx" else "s22", "m$i", null, true)

    @Test fun systemKeepsTheCharacterAndCapsTheMemoryToProtectPrefillTime() {
        val p = Persona(core = "C".repeat(1500), memory = "M".repeat(5000))
        val built = LocalPrompt.build(p, listOf(line(1, false)))
        assertTrue(built.system.startsWith("C"))
        assertTrue(built.system.length <= LocalPrompt.MAX_SYSTEM_CHARS)
        assertTrue(built.system.contains("M"))        // some memory fits
    }

    @Test fun historyIsTheLastTurnsEndingWithTheUsersMessage() {
        val ctx = (1..12).map { line(it, it % 2 == 0) } + line(13, false)
        val built = LocalPrompt.build(Persona("core", ""), ctx)
        assertEquals("m13", built.user)
        assertEquals(LocalPrompt.MAX_HISTORY, built.history.size)
        assertEquals("m12", built.history.last().text)
        assertTrue(built.history.last().fromJinx)
    }

    @Test fun anAnswerOfJinxAtTheEndIsNotTreatedAsTheUsersTurn() {
        val built = LocalPrompt.build(Persona("core", ""), listOf(line(1, false), line(2, true)))
        assertEquals("m1", built.user)         // the last *user* line is what is answered
        assertTrue(built.history.isEmpty())
    }

    @Test fun noUserLineAtAllIsAnError() {
        try {
            LocalPrompt.build(Persona("core", ""), listOf(line(1, true)))
            fail("should throw")
        } catch (e: ChatException) {
            assertTrue(e.message!!.isNotBlank())
        }
    }
}

class IdlePolicyTest {
    @Test fun keepsTheEngineWhileUsedRecentlyAndDropsItWhenIdle() {
        assertFalse(IdlePolicy.shouldRelease(lastUsed = 1_000, now = 1_000 + IdlePolicy.IDLE_MS - 1))
        assertTrue(IdlePolicy.shouldRelease(lastUsed = 1_000, now = 1_000 + IdlePolicy.IDLE_MS + 1))
    }

    @Test fun aClockGoingBackwardsReleasesRatherThanKeeps() {
        assertTrue(IdlePolicy.shouldRelease(lastUsed = 5_000, now = 1_000))
    }
}

class LocalModelBrainTest {
    private class FakeRuntime(val out: String = "respuesta local", val fail: Boolean = false) : LlmRuntime {
        var calls = 0
        var closed = false
        var seenSystem = ""
        var seenUser = ""
        override fun generate(system: String, history: List<Pair<Boolean, String>>, user: String, maxTokens: Int): String {
            calls++
            seenSystem = system
            seenUser = user
            if (fail) throw IllegalStateException("fallo del motor")
            return out
        }
        override fun close() { closed = true }
    }

    private val ctx = listOf(ChatLine("a", 1.0, "s22", "hola", "c", false))

    @Test fun answersThroughTheRuntimeAndReportsLocalVia() {
        val rt = FakeRuntime()
        val brain = LocalModelBrain(modelReady = { true }, persona = { Persona("SOY JINX", "") }, factory = { rt }, now = { 0L })
        assertEquals(Via.Local, brain.via)
        assertEquals("respuesta local", brain.reply(ctx))
        assertTrue(rt.seenSystem.contains("SOY JINX"))
        assertEquals("hola", rt.seenUser)
    }

    @Test fun noModelFileIsAChatExceptionAndTheRuntimeIsNeverCreated() {
        var created = false
        val brain = LocalModelBrain(modelReady = { false }, persona = { Persona("c", "") }, factory = { created = true; FakeRuntime() }, now = { 0L })
        try { brain.reply(ctx); fail() } catch (e: ChatException) { assertTrue(e.message!!.contains("modelo", ignoreCase = true)) }
        assertFalse(created)
    }

    @Test fun engineFailureBecomesAChatExceptionAndTheRuntimeIsReleased() {
        val rt = FakeRuntime(fail = true)
        val brain = LocalModelBrain(modelReady = { true }, persona = { Persona("c", "") }, factory = { rt }, now = { 0L })
        try { brain.reply(ctx); fail() } catch (e: ChatException) { assertTrue(e.message!!.isNotBlank()) }
        assertTrue(rt.closed)
    }

    @Test fun reusesTheRuntimeWhileFreshAndRebuildsItAfterBeingIdle() {
        var now = 0L
        var built = 0
        val brain = LocalModelBrain(modelReady = { true }, persona = { Persona("c", "") }, factory = { built++; FakeRuntime() }, now = { now })
        brain.reply(ctx); now += 1_000; brain.reply(ctx)
        assertEquals(1, built)
        now += IdlePolicy.IDLE_MS + 10
        brain.reply(ctx)
        assertEquals(2, built)
    }

    @Test fun blankOutputIsAnErrorNotAnEmptyBubble() {
        val brain = LocalModelBrain(modelReady = { true }, persona = { Persona("c", "") }, factory = { FakeRuntime(out = "  ") }, now = { 0L })
        try { brain.reply(ctx); fail() } catch (e: ChatException) { /* ok */ }
    }
}
