package com.agupta07505.smartisland.coucou

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class TaskLogicTest {
    private val now = 100 * 86_400L + 10 * 3600

    private fun shared(title: String, jinxId: String? = null) =
        TaskLogic.add(emptyList(), title, null, now, "t-$title", jinx = true).single().copy(jinxId = jinxId)

    private fun row(id: String, status: String) = JinxRow(id = id, titulo = "x", source = "coucou", status = status)

    private fun fails(f: () -> Unit) = try { f(); fail("should have failed") } catch (_: IllegalArgumentException) {}

    @Test
    fun tasks_are_added_completed_snoozed_and_deleted() {
        fails { TaskLogic.add(emptyList(), "   ", null, now, "a") }
        var l = TaskLogic.add(emptyList(), "uno", now + 60, now, "a")
        l = TaskLogic.add(l, "dos", null, now, "b")
        l = TaskLogic.markNotified(l, "a")
        assertTrue(l[0].notified)
        l = TaskLogic.setDone(l, "a", true, now + 5)
        assertEquals(now + 5, l[0].completedAt)
        l = TaskLogic.snooze(l, "a", now + 600)
        assertTrue(!l[0].done && !l[0].notified)
        assertEquals(now + 600, l[0].remindAt)
        l = TaskLogic.setDone(l, "b", true, now)
        l = TaskLogic.clearDone(l, now + 8)
        assertEquals(1, TaskLogic.visible(l).size)
        assertTrue("a cleared one is a tombstone", l.any { it.id == "b" && it.deleted && it.updatedAt == now + 8 })
        l = TaskLogic.delete(l, "a", now + 9)
        assertTrue(TaskLogic.visible(l).isEmpty())
        fails { TaskLogic.delete(l, "a") }
    }

    @Test
    fun the_list_and_a_title_are_bounded() {
        val long = TaskLogic.add(emptyList(), "x".repeat(TaskLogic.MAX_TITLE + 50), null, now, "a")
        assertEquals(TaskLogic.MAX_TITLE, long[0].title.length)
        var l = emptyList<CoucouTask>()
        repeat(TaskLogic.MAX_TASKS) { l = TaskLogic.add(l, "t", null, now, "id$it") }
        fails { TaskLogic.add(l, "una más", null, now, "x") }
    }

    @Test
    fun the_next_reminder_is_the_earliest_one_not_yet_announced() {
        var l = TaskLogic.add(emptyList(), "tarde", now + 600, now, "a")
        l = TaskLogic.add(l, "pronto", now + 60, now, "b")
        l = TaskLogic.add(l, "sin hora", null, now, "c")
        assertEquals("b", TaskLogic.nextReminder(l)?.id)
        l = TaskLogic.markNotified(l, "b")
        assertEquals("a", TaskLogic.nextReminder(l)?.id)
        l = TaskLogic.setDone(l, "a", true, now)
        assertNull(TaskLogic.nextReminder(l))
    }

    @Test
    fun a_shared_task_not_yet_hers_is_handed_over_while_it_is_open() {
        val (_, plan) = TaskLogic.reconcile(listOf(shared("a")), emptyList(), now)
        assertEquals(listOf("t-a"), plan.toAdd)
        val finished = shared("a").copy(done = true)
        assertTrue(TaskLogic.reconcile(listOf(finished), emptyList(), now).second.toAdd.isEmpty())
        val private = shared("b").copy(jinx = false)
        assertEquals(SyncPlan(), TaskLogic.reconcile(listOf(private), emptyList(), now).second)
    }

    @Test
    fun finishing_follows_in_both_directions() {
        // She closed it: it is done here.
        val (l1, p1) = TaskLogic.reconcile(listOf(shared("a", "j1")), listOf(row("j1", "hecho")), now)
        assertTrue(l1[0].done && l1[0].completedAt == now && p1.changed && p1.toResolve.isEmpty())
        // We closed it, she still has it open: asked again until it is there.
        val (_, p2) = TaskLogic.reconcile(listOf(shared("a", "j1").copy(done = true)), listOf(row("j1", "pendiente")), now)
        assertEquals(listOf("j1" to "hecho"), p2.toResolve)
        assertFalse(p2.changed)
        // Both open, or both closed: nothing to do on her side (it only remembers that they agree).
        val open = TaskLogic.reconcile(listOf(shared("a", "j1")), listOf(row("j1", "pendiente")), now)
        assertTrue(open.second.toAdd.isEmpty() && open.second.toResolve.isEmpty())
        assertEquals("pendiente", open.first[0].jinxStatus)
        val closed = TaskLogic.reconcile(listOf(shared("a", "j1").copy(done = true)), listOf(row("j1", "hecho")), now)
        assertTrue(closed.second.toAdd.isEmpty() && closed.second.toResolve.isEmpty())
    }

    @Test
    fun unsharing_dismisses_her_copy_and_a_vanished_one_breaks_the_link() {
        val (l, p) = TaskLogic.reconcile(listOf(shared("a", "j1").copy(jinx = false)), listOf(row("j1", "pendiente")), now)
        assertEquals(listOf("j1" to "descartado"), p.toResolve)
        assertNull(l[0].jinxId)
        val (l2, p2) = TaskLogic.reconcile(listOf(shared("a", "gone")), listOf(row("j1", "pendiente")), now)
        assertTrue(p2.changed)
        assertNull(l2[0].jinxId)
    }

    @Test
    fun only_her_open_items_that_are_not_ours_are_listed() {
        val l = listOf(shared("a", "j1"))
        val rows = listOf(row("j1", "pendiente"), row("j2", "pendiente"), row("j3", "hecho"))
        assertEquals(listOf("j2"), TaskLogic.unlinkedOpen(l, rows).map { it.id })
    }

    @Test
    fun a_day_number_is_a_calendar_date() {
        assertEquals("1970-01-01", TaskLogic.civilDate(0))
        assertEquals("1970-04-11", TaskLogic.civilDate(100))
        assertEquals("2000-02-29", TaskLogic.civilDate(11_016))
        assertEquals("2026-10-06", TaskLogic.civilDate(20_732))
        assertEquals("1969-12-31", TaskLogic.civilDate(-1))
    }

    private val day = 86_400L

    private fun pend(id: String, source: String, due: String?, status: String = "pendiente") =
        JinxRow(id, "pend $id", due, null, source, status)

    @Test
    fun a_day_becomes_nine_in_the_morning_in_this_zone() {
        assertEquals(0L, TaskLogic.daysFromCivil(1970, 1, 1))
        assertEquals(11_016L, TaskLogic.daysFromCivil(2000, 2, 29))
        assertEquals(20_732L, TaskLogic.daysFromCivil(2026, 10, 6))
        assertEquals(-1L, TaskLogic.daysFromCivil(1969, 12, 31))
        assertEquals(20_732 * day + 9 * 3600, TaskLogic.dueReminder("2026-10-06") { 0L })
        assertEquals(20_732 * day + 15 * 3600, TaskLogic.dueReminder("2026-10-06") { -6 * 3600L })
        for (bad in listOf("mañana", "2026-13-01", "2026-10-00", "2026/10/06", "", "2026-10-061")) assertNull(bad, TaskLogic.dueReminder(bad) { 0L })
    }

    @Test
    fun what_jinx_made_becomes_ordinary_tasks_with_a_mark() {
        val today = 20_732 * day + 10 * 3600
        val rows = listOf(
            pend("a", "hook", "2026-10-08"),
            pend("b", "teams", "2026-10-06"),
            pend("c", "portal", null),
            pend("d", "coucou", null),
            pend("e", "hook", null, "hecho"),
        )
        val (l, taken) = TaskLogic.importUnlinked(emptyList(), rows, today) { 0L }
        assertEquals(4, taken)
        fun by(id: String) = l.single { it.jinxId == id }
        assertEquals(Triple(20_734 * day + 9 * 3600, false, "hook"), Triple(by("a").remindAt, by("a").notified, by("a").origin))
        assertEquals(Triple(20_732 * day + 9 * 3600, true, "teams"), Triple(by("b").remindAt, by("b").notified, by("b").origin))
        assertEquals(null to "portal", by("c").remindAt to by("c").origin)
        assertNull(by("d").origin)
        assertTrue(l.all { it.jinx && !it.done })
        // Taken in once; the next sync leaves them alone.
        assertEquals(0, TaskLogic.importUnlinked(l, rows, today) { 0L }.second)
        TaskLogic.reconcile(l, rows, today).second.let { assertTrue(it.toResolve.isEmpty() && it.toAdd.isEmpty()) }
    }

    @Test
    fun nothing_is_taken_in_beyond_the_limit() {
        var l = emptyList<CoucouTask>()
        for (i in 0 until TaskLogic.MAX_TASKS) l = TaskLogic.add(l, "t$i", null, now, "id$i")
        assertEquals(0, TaskLogic.importUnlinked(l, listOf(pend("z", "hook", null)), now) { 0L }.second)
    }

    /** The same file the PC's tasks.rs reads: a pendiente's `due` rings at the same moment on both. */
    @Test fun `due reminders follow the shared vectors`() {
        val doc = org.json.JSONObject(javaClass.classLoader!!.getResourceAsStream("due-vectors.json")!!.bufferedReader().readText())
        val off = doc.getLong("offset")
        val cases = doc.getJSONArray("cases")
        for (i in 0 until cases.length()) {
            val c = cases.getJSONObject(i)
            val want = if (c.isNull("expect")) null else c.getLong("expect")
            assertEquals(c.getString("due") + ": " + c.getString("note"), want, TaskLogic.dueReminder(c.getString("due")) { off })
        }
    }
}
