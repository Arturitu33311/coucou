package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.phone.JinxException
import com.agupta07505.smartisland.coucou.phone.JinxProtocol
import com.agupta07505.smartisland.coucou.phone.JinxSync
import com.agupta07505.smartisland.coucou.phone.PendientesApi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import java.io.File

/** A Jinx made of a list in memory, with the same rules as her store (add upserts, resolve by id). */
private class FakeJinx(val rows: MutableList<JinxRow> = mutableListOf()) : PendientesApi {
    var down = false
    var refuseAdd = false
    val calls = mutableListOf<String>()
    private var n = 0

    override fun list(): List<JinxRow> {
        if (down) throw JinxException("Jinx is not reachable")
        calls += "list"
        return rows.toList()
    }

    override fun add(task: CoucouTask): String {
        if (down) throw JinxException("Jinx is not reachable")
        if (refuseAdd) throw JinxException("bad title")
        calls += "add:${task.id}"
        val existing = rows.firstOrNull { it.source == "coucou" && it.titulo == task.title && it.open }
        if (existing != null) return existing.id
        val id = "j%x".format(++n)
        rows += JinxRow(id, task.title, null, "personal", "coucou", "pendiente")
        return id
    }

    override fun resolve(id: String, action: String) {
        if (down) throw JinxException("Jinx is not reachable")
        calls += "resolve:$id:$action"
        val i = rows.indexOfFirst { it.id == id }
        if (i < 0) throw JinxException("no such pendiente")
        rows[i] = rows[i].copy(status = if (action == "reabrir") "pendiente" else action)
    }
}

class JinxSyncTest {
    private val now = 100 * 86_400L + 10 * 3600

    private fun store(): TaskStore {
        val f = File.createTempFile("coucou-tasks", ".json")
        f.delete()
        f.deleteOnExit()
        return TaskStore(f)
    }

    private fun TaskStore.addShared(title: String, id: String = "t-$title") =
        update { TaskLogic.add(it, title, null, now, id, jinx = true) }

    @Test
    fun a_shared_task_reaches_jinx_once_and_remembers_her_id() {
        val s = store()
        val j = FakeJinx()
        s.addShared("llamar a mamá")
        val out = JinxSync.run(j, s, now)
        assertNull(out.error)
        assertEquals("j1", s.load()[0].jinxId)
        assertEquals(1, j.rows.size)
        // A second round changes nothing and asks nothing more.
        j.calls.clear()
        JinxSync.run(j, s, now)
        assertEquals(listOf("list"), j.calls)
        assertEquals(1, j.rows.size)
    }

    @Test
    fun finishing_follows_in_both_directions() {
        val s = store()
        val j = FakeJinx()
        s.addShared("a")
        JinxSync.run(j, s, now)
        // She closes it: it is done here.
        j.resolve("j1", "hecho")
        JinxSync.run(j, s, now)
        assertTrue(s.load()[0].done)
        // We close another, she still has it open: she is told.
        s.addShared("b")
        JinxSync.run(j, s, now)
        s.update { TaskLogic.setDone(it, "t-b", true, now) }
        JinxSync.run(j, s, now)
        assertEquals("hecho", j.rows.first { it.titulo == "b" }.status)
    }

    @Test
    fun what_jinx_made_becomes_a_task_of_ours_and_what_is_ours_does_not_repeat() {
        val s = store()
        val j = FakeJinx(mutableListOf(JinxRow("h1", "pagar la renta", "2026-10-07", "admin", "hook", "pendiente")))
        s.addShared("mía")
        val out = JinxSync.run(j, s, now)
        assertTrue(out.fromJinx.isEmpty())
        val mine = out.tasks.single { it.jinxId == "h1" }
        assertEquals("pagar la renta", mine.title)
        assertEquals("hook", mine.origin)
        assertEquals(2, out.tasks.size)
        // Marking it done here closes it there, like any other.
        s.update { TaskLogic.setDone(it, mine.id, true, now) }
        JinxSync.run(j, s, now)
        assertEquals("hecho", j.rows.first { it.id == "h1" }.status)
    }

    @Test
    fun an_unreachable_jinx_changes_nothing_and_the_difference_is_asked_again_later() {
        val s = store()
        val j = FakeJinx()
        s.addShared("a")
        j.down = true
        val out = JinxSync.run(j, s, now)
        assertEquals("Jinx is not reachable", out.error)
        assertNull(s.load()[0].jinxId)
        j.down = false
        assertNull(JinxSync.run(j, s, now).error)
        assertNotNull(s.load()[0].jinxId)
    }

    @Test
    fun one_refused_task_does_not_stop_the_others_or_the_round() {
        val s = store()
        val j = FakeJinx()
        s.addShared("a")
        j.refuseAdd = true
        val out = JinxSync.run(j, s, now)
        assertEquals("bad title", out.error)
        assertEquals(1, s.load().size)
    }

    @Test
    fun every_task_is_shared_so_a_private_one_is_shared_again() {
        val s = store()
        val j = FakeJinx()
        s.addShared("a")
        JinxSync.run(j, s, now)
        s.update { TaskLogic.setJinx(it, "t-a", false) }
        JinxSync.run(j, s, now)
        assertEquals("pendiente", j.rows[0].status)
        assertTrue(s.load().single().jinx)
    }

    // ── The wire format ─────────────────────────────────────────────────────────

    @Test
    fun a_reminder_becomes_her_date_and_a_note_with_the_time() {
        val t = 20_732 * 86_400L + 17 * 3600 + 30 * 60 // 2026-10-06 17:30 UTC
        assertEquals("2026-10-06" to "Coucou · 17:30", JinxProtocol.dueAndNote(t, 0))
        // At UTC-6 the same instant is 11:30 the same day; 22:00 UTC-6 on the 6th is 04:00 UTC on the 7th.
        assertEquals("2026-10-06" to "Coucou · 11:30", JinxProtocol.dueAndNote(t, -6 * 3600))
        assertEquals("2026-10-06" to "Coucou · 22:00", JinxProtocol.dueAndNote(20_733 * 86_400L + 4 * 3600, -6 * 3600))
        assertEquals(null to "Coucou", JinxProtocol.dueAndNote(null, 0))
    }

    @Test
    fun the_request_for_a_task_carries_only_what_the_gateway_takes() {
        val task = CoucouTask("abc-1", "comprar \"pila\" ✓", created = 1, remindAt = null)
        val o = org.json.JSONObject(JinxProtocol.add(task, 0))
        assertEquals(setOf("op", "titulo", "due", "nota", "source_id"), o.keys().asSequence().toSet())
        assertEquals("comprar \"pila\" ✓", o.getString("titulo"))
        assertTrue(o.isNull("due"))
        assertEquals("abc-1", o.getString("source_id"))
    }

    @Test
    fun her_answers_are_read_strictly() {
        val rows = JinxProtocol.parseList(
            """{"ok":true,"items":[{"id":"a1","titulo":"x","due":null,"categoria":"admin","source":"hook","status":"pendiente"},{"titulo":"sin id"}]}""",
        )
        assertEquals(1, rows.size)
        assertNull(rows[0].due)
        assertEquals("a1", JinxProtocol.parseId("""{"ok":true,"id":"a1","status":"creado"}"""))
        for (bad in listOf("""{"ok":false,"error":"bad title"}""", "not json", "")) {
            try {
                JinxProtocol.parseList(bad)
                fail("accepted: $bad")
            } catch (_: JinxException) {
            }
        }
        // Noise before the JSON line (a login banner) does not matter: the last line is the answer.
        assertEquals("a1", JinxProtocol.parseId("welcome\n{\"ok\":true,\"id\":\"a1\"}\n"))
        assertFalse(rows.isEmpty())
    }
}
