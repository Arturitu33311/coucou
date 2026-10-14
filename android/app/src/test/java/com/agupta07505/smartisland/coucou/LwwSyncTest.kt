package com.agupta07505.smartisland.coucou

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Each device works alone; when they meet, the newest change wins. The same rules as Coucou's tasks.rs. */
class LwwSyncTest {
    private val now = 1_000L

    private fun mk(id: String, title: String, updated: Long) =
        CoucouTask(id = id, title = title, created = 1, updatedAt = updated, jinx = true)

    /** Last agreed with her at stamp 100 (open on both sides). */
    private fun agreed(updated: Long) =
        mk("t-a", "a", updated).copy(jinxId = "j1", jinxStatus = "pendiente", jinxSynced = 100)

    private fun row(status: String, updatedAt: Long? = null) =
        JinxRow(id = "j1", titulo = "a", source = "coucou", status = status, updatedAt = updatedAt)

    // ── stamps and tombstones ──────────────────────────────────────────────────────

    @Test
    fun a_change_is_stamped_and_a_deletion_is_a_tombstone_the_screens_do_not_see() {
        var l = TaskLogic.add(emptyList(), "a", null, 100, "x")
        assertEquals(100L, l[0].updatedAt)
        l = TaskLogic.setDone(l, "x", true, 105)
        assertEquals(105L, l[0].updatedAt)
        l = TaskLogic.snooze(l, "x", 900, 107)
        assertEquals(107L, l[0].updatedAt)
        l = TaskLogic.delete(l, "x", 109)
        assertTrue(l[0].deleted && l[0].updatedAt == 109L)
        assertTrue(TaskLogic.visible(l).isEmpty())
        assertNull("a deleted task never rings", TaskLogic.nextReminder(l))
        assertEquals(1, TaskLogic.purgeTombstones(l, 109 + TaskLogic.TOMBSTONE_SECS - 1).size)
        assertTrue(TaskLogic.purgeTombstones(l, 109 + TaskLogic.TOMBSTONE_SECS + 1).isEmpty())
    }

    // ── meeting another device ─────────────────────────────────────────────────────

    @Test
    fun the_newest_change_wins_whichever_way_round_they_meet() {
        val phone = listOf(mk("x", "llamar", 100).copy(done = true, updatedAt = 200))
        val laptop = listOf(mk("x", "llamar", 100).copy(remindAt = 500, updatedAt = 150))
        val (a, aChanged) = TaskLogic.merge(phone, laptop, now)
        val (b, bChanged) = TaskLogic.merge(laptop, phone, now)
        assertFalse("the phone's own version is the newest", aChanged)
        assertTrue(bChanged)
        assertEquals(a, b)
        assertTrue(a[0].done && a[0].updatedAt == 200L)
        assertFalse("meeting again changes nothing", TaskLogic.merge(a, b, now).second)
    }

    @Test
    fun a_wrong_clock_does_not_win_for_ever() {
        val (l, _) = TaskLogic.merge(listOf(mk("x", "a", 1_000)), listOf(mk("x", "future", 9_999_999).copy(done = true)), 2_000)
        assertEquals(2_000 + TaskLogic.SKEW_SECS, l[0].updatedAt)
        val (l2, _) = TaskLogic.merge(l, listOf(mk("x", "real", 2_400)), 2_400)
        assertEquals("real", l2[0].title)
    }

    @Test
    fun a_reminder_still_to_come_rings_where_it_is_learnt_and_a_past_one_does_not() {
        val soon = mk("s", "pronto", 100).copy(remindAt = now + 600, notified = true)
        val past = mk("p", "ya paso", 100).copy(remindAt = now - 600)
        val (l, _) = TaskLogic.merge(emptyList(), listOf(soon, past), now)
        assertFalse(l.first { it.id == "s" }.notified)
        assertTrue(l.first { it.id == "p" }.notified)
    }

    @Test
    fun what_another_device_sends_is_bounded_before_it_is_merged() {
        val t = TaskLogic.sanitizeRemote(mk("abc", "  hola\u0007  ", 10).copy(jinxId = "not hex!", jinxStatus = "hecho"))!!
        assertEquals("hola", t.title)
        assertNull(t.jinxId)
        assertNull(t.jinxStatus)
        assertNull(TaskLogic.sanitizeRemote(mk("", "x", 1)))
        assertNull(TaskLogic.sanitizeRemote(mk("x".repeat(65), "x", 1)))
        assertNull(TaskLogic.sanitizeRemote(mk("a", "   ", 1)))
        assertEquals(TaskLogic.MAX_TITLE, TaskLogic.sanitizeRemote(mk("a", "y".repeat(1000), 1))!!.title.length)
        assertNotNull("a tombstone needs no title", TaskLogic.sanitizeRemote(mk("a", "", 1).copy(deleted = true)))
    }

    // ── with Jinx: a three-way ─────────────────────────────────────────────────────

    @Test
    fun with_jinx_only_one_side_changed_is_copied_to_the_other() {
        // She closed it while we did nothing: it is done here, stamped when we saw it.
        val (l1, p1) = TaskLogic.reconcile(listOf(agreed(100)), listOf(row("hecho")), now)
        assertTrue(l1[0].done && l1[0].updatedAt == now && p1.toResolve.isEmpty())
        // We closed it while she did nothing: she is told.
        val (_, p2) = TaskLogic.reconcile(listOf(agreed(150).copy(done = true)), listOf(row("pendiente")), now)
        assertEquals(listOf("j1" to "hecho"), p2.toResolve)
        // We deleted it: her copy is dismissed.
        val (_, p3) = TaskLogic.reconcile(listOf(agreed(150).copy(deleted = true)), listOf(row("pendiente")), now)
        assertEquals(listOf("j1" to "descartado"), p3.toResolve)
        // Closed on both sides they agree, and the agreement is remembered...
        val closed = agreed(150).copy(jinxStatus = "hecho", done = true)
        val (l4, p4) = TaskLogic.reconcile(listOf(closed), listOf(row("hecho")), now)
        assertTrue(p4.toResolve.isEmpty())
        assertEquals(150L, l4[0].jinxSynced)
        // ...and snoozed here later (open again, a later stamp) she is told to reopen it.
        val (_, p5) = TaskLogic.reconcile(listOf(closed.copy(done = false, updatedAt = 200)), listOf(row("hecho")), now)
        assertEquals(listOf("j1" to "reabrir"), p5.toResolve)
    }

    @Test
    fun with_jinx_when_both_changed_the_later_stamp_wins() {
        // She closed it (her gateway gives no stamp); we moved the reminder at 150: she wins.
        val (l1, p1) = TaskLogic.reconcile(listOf(agreed(150)), listOf(row("hecho")), now)
        assertTrue(l1[0].done && p1.toResolve.isEmpty())
        // Her stamp is older than ours: ours wins and she is told.
        val (l2, p2) = TaskLogic.reconcile(listOf(agreed(150)), listOf(row("hecho", 120)), now)
        assertFalse(l2[0].done)
        assertEquals(listOf("j1" to "reabrir"), p2.toResolve)
        // Her stamp is newer: hers wins, and the task carries her stamp.
        val (l3, _) = TaskLogic.reconcile(listOf(agreed(150)), listOf(row("hecho", 180)), now)
        assertTrue(l3[0].done && l3[0].updatedAt == 180L && l3[0].jinxSynced == 180L)
    }

    @Test
    fun a_task_taken_in_from_jinx_is_not_a_person_s_change() {
        val (l, _) = TaskLogic.importUnlinked(emptyList(), listOf(JinxRow("a", "pend a", source = "hook")), 5_000) { 0L }
        assertEquals(0L, l[0].updatedAt)
        assertEquals("pendiente", l[0].jinxStatus)
    }

    @Test
    fun her_timestamps_become_unix_seconds_in_any_zone() {
        val utc = 20_732L * 86_400 + 19 * 3600 + 30 * 60 + 42
        assertEquals(utc, TaskLogic.isoToUnix("2026-10-06T19:30:42.274785+00:00"))
        assertEquals(utc, TaskLogic.isoToUnix("2026-10-06T19:30:42Z"))
        assertEquals(utc, TaskLogic.isoToUnix("2026-10-06 19:30:42"))
        assertEquals(utc, TaskLogic.isoToUnix("2026-10-06T13:30:42-06:00"))
        assertEquals(utc, TaskLogic.isoToUnix("2026-10-07T01:30:42+06:00"))
        assertNull(TaskLogic.isoToUnix("mañana"))
    }

    // ── on disk ────────────────────────────────────────────────────────────────────

    @Test
    fun the_new_fields_survive_a_save_and_an_old_file_gets_its_stamp() {
        val t = mk("x", "a", 123).copy(deleted = true, jinxStatus = "hecho", jinxSynced = 120)
        assertEquals(listOf(t), TaskStore.decode(TaskStore.encode(listOf(t))))
        // A file from before the stamps: made here counts from when it was made or finished; from Jinx stays 0.
        val old = """[{"id":"x","title":"a","done":true,"created":50,"completedAt":80},{"id":"j-9","title":"b","done":false,"created":60}]"""
        val l = TaskStore.decode(old)
        assertEquals(80L, l[0].updatedAt)
        assertEquals(0L, l[1].updatedAt)
    }

    // ── the cases both platforms must satisfy ──────────────────────────────────────

    @Test
    fun the_merge_vectors_shared_with_the_laptop_hold() {
        val text = javaClass.classLoader!!.getResourceAsStream("merge-vectors.json")!!.bufferedReader().readText()
        val cases = JSONObject(text).getJSONArray("cases")
        fun project(l: List<CoucouTask>) = l.map { listOf(it.id, it.title, it.done, it.deleted, it.updatedAt).joinToString("|") }.sorted()
        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            val name = c.getString("name")
            val at = c.getLong("now")
            val local = TaskStore.decode(c.getJSONArray("local").toString())
            val remote = TaskStore.decode(c.getJSONArray("remote").toString())
            val want = TaskStore.decode(
                org.json.JSONArray().also { arr ->
                    val ex = c.getJSONArray("expect")
                    for (k in 0 until ex.length()) arr.put(ex.getJSONObject(k).put("created", 1))
                }.toString(),
            )
            // `decode` gives a stamp to a file without one; the vectors always have it, so the values are the cases'.
            assertEquals(name, project(want), project(TaskLogic.merge(local, remote, at).first))
            if (c.optBoolean("symmetric")) {
                assertEquals("$name (the other way round)", project(want), project(TaskLogic.merge(remote, local, at).first))
            }
        }
    }
}
