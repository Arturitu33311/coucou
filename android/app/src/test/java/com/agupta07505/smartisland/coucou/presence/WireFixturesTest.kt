package com.agupta07505.smartisland.coucou.presence

import com.agupta07505.smartisland.coucou.CoucouTask
import com.agupta07505.smartisland.coucou.TaskStore
import com.agupta07505.smartisland.coucou.agents.RemoteMsg
import com.agupta07505.smartisland.coucou.agents.RemoteProtocol
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The lines both ends of the phone link write for state and tasks; the PC's test reads the same file. */
class WireFixturesTest {
    private val doc: JSONObject = JSONObject(
        javaClass.classLoader!!.getResourceAsStream("wire-fixtures.json")!!.bufferedReader().readText(),
    )

    private fun keys(o: JSONObject) = o.keys().asSequence().toSet()

    @Test
    fun the_laptops_lines_are_read_here() {
        val peer = RemoteProtocol.parse(doc.getJSONObject("laptop_to_phone").getJSONObject("peer").toString(), 0L)
        assertTrue(peer is RemoteMsg.Peer)
        assertEquals("laptop", (peer as RemoteMsg.Peer).beat.device)
        assertTrue(!peer.limit)
        // A rate-limited laptop says so: the phone's Mochi is tired with it.
        val limited = doc.getJSONObject("laptop_to_phone").getJSONObject("peer").put("limit", true)
        assertTrue((RemoteProtocol.parse(limited.toString(), 0L) as RemoteMsg.Peer).limit)

        val tasks = RemoteProtocol.parse(doc.getJSONObject("laptop_to_phone").getJSONObject("tasks").toString(), 0L)
        assertTrue(tasks is RemoteMsg.Tasks)
        val decoded = TaskStore.decode((tasks as RemoteMsg.Tasks).json)
        assertEquals(1, decoded.size)
        assertTrue(decoded[0].done)
        assertEquals(9L, decoded[0].updatedAt)
    }

    @Test
    fun the_phones_lines_have_the_keys_the_laptop_reads() {
        val want = doc.getJSONObject("phone_to_laptop")
        val beat = JSONObject(RemoteProtocol.peer(Beat("s21", 4, true, false, null), music = true, news = false))
        assertEquals(keys(want.getJSONObject("peer")), keys(beat))

        val task = CoucouTask(id = "t-1", title = "leche", created = 1, updatedAt = 5)
        val sync = JSONObject(RemoteProtocol.tasksSync(TaskStore.encode(listOf(task))))
        assertEquals("tasks_sync", sync.getString("op"))
        assertEquals(
            keys(want.getJSONObject("tasks_sync").getJSONArray("tasks").getJSONObject(0)),
            keys(sync.getJSONArray("tasks").getJSONObject(0)),
        )
        assertNotNull(sync.getJSONArray("tasks").getJSONObject(0).getString("id"))
    }
}
