package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.assist.ChatApi
import com.agupta07505.smartisland.coucou.assist.ChatEvent
import com.agupta07505.smartisland.coucou.assist.ChatException
import com.agupta07505.smartisland.coucou.assist.ChatLine
import com.agupta07505.smartisland.coucou.assist.ChatRepository
import com.agupta07505.smartisland.coucou.assist.LocalBrain
import com.agupta07505.smartisland.coucou.assist.Mirror
import com.agupta07505.smartisland.coucou.assist.SendResult
import com.agupta07505.smartisland.coucou.assist.ThreadStore
import com.agupta07505.smartisland.coucou.assist.Via
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

private class FakeLaptop : ChatApi {
    var down = false
    var answer = "desde la laptop"
    var answerMirrored = true
    val sent = mutableListOf<String>()
    var mergedWith: List<ChatLine> = emptyList()
    var toGive: List<ChatLine> = emptyList()
    var mirroredIds: List<String> = emptyList()
    override fun send(text: String, cid: String, src: String): ChatEvent.Done {
        if (down) throw ChatException("laptop no disponible")
        sent += "$cid|$text|$src"
        return ChatEvent.Done("or", answer, answerMirrored)
    }
    override fun merge(lines: List<ChatLine>, have: List<String>): ChatEvent.Merged {
        if (down) throw ChatException("laptop no disponible")
        mergedWith = lines
        return ChatEvent.Merged(lines.size, toGive, mirroredIds)
    }
    override fun core() = ChatEvent.Core("c", "m")
    override fun provision() = ChatEvent.Secrets("k", "t", "1")
}

private class FakeBrain(override val via: Via, var text: String? = "desde el teléfono") : LocalBrain {
    var calls = 0
    var seen: List<ChatLine> = emptyList()
    override fun reply(context: List<ChatLine>): String {
        calls++
        seen = context
        return text ?: throw ChatException("sin modelo")
    }
}

private class FakeMirror(var ok: Boolean = true) : Mirror {
    val sent = mutableListOf<String>()
    override fun deliver(text: String): Boolean { sent += text; return ok }
}

class ChatRepositoryTest {
    @get:Rule val tmp = TemporaryFolder()
    private var n = 0
    private fun store() = ThreadStore(File(tmp.root, "t.jsonl"))
    private fun repo(
        store: ThreadStore, laptop: ChatApi?, brains: List<LocalBrain>, mirror: Mirror? = null,
    ) = ChatRepository(store, "s21", laptop, brains, mirror, clock = { 100.0 + n }, newCid = { "c${++n}" })

    @Test fun laptopAnswersFirstAndTheLocalBrainIsNotUsed() {
        val s = store(); val lap = FakeLaptop(); val b = FakeBrain(Via.Cloud)
        val r = repo(s, lap, listOf(b)).send("hola") as SendResult.Answered
        assertEquals(Via.Laptop, r.via)
        assertEquals("desde la laptop", r.line.text)
        assertEquals(0, b.calls)
        assertEquals(listOf("c1|hola|s21"), lap.sent)
        assertTrue(s.unmirrored().isEmpty()) // the laptop mirrors by itself
    }

    @Test fun enqueueStoresTheUsersLineAtOnceAndAnswerComesLater() {
        val s = store(); val lap = FakeLaptop()
        val r = repo(s, lap, emptyList())
        val line = r.enqueue("hola")
        assertEquals(listOf(line.id), s.all().map { it.id })      // visible before any brain runs
        assertEquals(0, lap.sent.size)
        val res = r.answer(line) as SendResult.Answered
        assertEquals("desde la laptop", res.line.text)
        assertEquals(2, s.all().size)
    }

    @Test fun laptopAnswerNotConfirmedAsMirroredStaysQueuedHere() {
        val s = store(); val lap = FakeLaptop().apply { answerMirrored = false }
        repo(s, lap, emptyList()).send("hola")
        assertEquals(2, s.unmirrored().size)
    }

    @Test fun laptopDownFallsBackToCloudThenLocal() {
        val s = store(); val lap = FakeLaptop().apply { down = true }
        val cloud = FakeBrain(Via.Cloud, text = null)
        val local = FakeBrain(Via.Local, text = "sin conexión pero aquí")
        val r = repo(s, lap, listOf(cloud, local)).send("hola") as SendResult.Answered
        assertEquals(Via.Local, r.via)
        assertEquals(1, cloud.calls)
        assertEquals(listOf("hola", "sin conexión pero aquí"), s.all().map { it.text })
    }

    @Test fun contextGivenToTheBrainEndsWithTheUsersLine() {
        val s = store()
        s.append(ChatLine("t1:tg", 1.0, "tg", "de telegram", null, true))
        val b = FakeBrain(Via.Cloud)
        repo(s, null, listOf(b)).send("y ahora")
        assertEquals(listOf("de telegram", "y ahora"), b.seen.map { it.text })
    }

    @Test fun nothingAvailableLeavesThePendingLineAndInventsNothing() {
        val s = store()
        val r = repo(s, null, listOf(FakeBrain(Via.Cloud, text = null)))
        val res = r.send("hola")
        assertTrue(res is SendResult.Pending)
        assertEquals(listOf("c1:s21"), s.pending().map { it.id })
        assertTrue(s.all().none { it.fromJinx })
    }

    @Test fun retryPendingAnswersWhenAModelComesBack() {
        val s = store(); val b = FakeBrain(Via.Cloud, text = null)
        val r = repo(s, null, listOf(b))
        r.send("hola")
        b.text = "ya"
        val res = r.retryPending()
        assertEquals(1, res.size)
        assertTrue(res[0] is SendResult.Answered)
        assertTrue(s.pending().isEmpty())
    }

    @Test fun aPhoneAnswerStaysOnThePhoneNotTelegram() {
        val s = store(); val m = FakeMirror(ok = true)
        val r = repo(s, null, listOf(FakeBrain(Via.Cloud)), m)
        r.send("hola")
        r.flushMirror()
        assertEquals(emptyList<String>(), m.sent)
        assertTrue(s.unmirrored().isEmpty())
        assertEquals("or", s.all().last().via)
    }

    @Test fun leftoversGoToTelegramUnmarkedButABackupKeepsItsWarning() {
        val s = store(); val m = FakeMirror(ok = true)
        s.append(ChatLine("c1:s21", 1.0, "s21", "hola", "c1", false))
        s.append(ChatLine("c1:jinx", 2.0, "jinx", "ya", "c1", false, via = "or"))
        repo(s, null, emptyList(), m).flushMirror()
        assertEquals(listOf("hola", "⚠️ ya"), m.sent)
    }

    @Test fun syncSendsOurLinesAndTakesTheOnesWeLack() {
        val s = store(); val lap = FakeLaptop()
        val r = repo(s, null, listOf(FakeBrain(Via.Cloud)))
        r.send("sin laptop") // c1: user line + answer, both unmirrored
        lap.toGive = listOf(ChatLine("x:tg", 1.0, "tg", "mensaje de telegram", null, true))
        lap.mirroredIds = listOf("c1:s21", "c1:jinx")
        val sync = repo(s, lap, emptyList())
        assertEquals(1, sync.sync())
        assertEquals(2, lap.mergedWith.size)
        assertTrue(s.all().any { it.id == "x:tg" })
        assertTrue(s.unmirrored().isEmpty())
    }

    @Test fun syncWithTheLaptopDownDoesNotThrow() {
        val s = store(); val lap = FakeLaptop().apply { down = true }
        assertEquals(0, repo(s, lap, emptyList()).sync())
    }

    @Test fun answerAlreadyStoredForACidIsNotAskedAgain() {
        val s = store(); val lap = FakeLaptop(); val r = repo(s, lap, emptyList())
        val first = r.send("hola") as SendResult.Answered
        val again = r.retryPending()
        assertTrue(again.isEmpty())
        assertEquals(1, lap.sent.size)
        assertEquals("desde la laptop", first.line.text)
    }
}
