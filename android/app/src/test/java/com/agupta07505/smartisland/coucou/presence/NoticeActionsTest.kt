package com.agupta07505.smartisland.coucou.presence

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class NoticeActionsTest {
    private fun arr(json: String) = JSONArray(json)

    @Test
    fun valid_buttons_unique_and_at_most_three() {
        val got = NoticeActions.parse(
            arr(
                """[{"id":"done","label":"Hecho"},{"id":"Done","label":"mal id"},{"id":"done","label":"repetido"},
                    {"id":"snooze","label":"  "},{"id":"later","label":"${"L".repeat(60)}"},
                    {"id":"a","label":"uno"},{"id":"b","label":"dos"}]""",
            ),
        )
        assertEquals(listOf("done", "later", "a"), got.map { it.id })
        assertEquals(NoticeActions.MAX_LABEL, got[1].label.length)
    }

    @Test
    fun nothing_or_garbage_gives_no_buttons() {
        assertTrue(NoticeActions.parse(null).isEmpty())
        assertTrue(NoticeActions.parse(arr("[]")).isEmpty())
        assertTrue(NoticeActions.parse(arr("""["texto", 3, {"id":"x"}]""")).isEmpty())
    }

    private fun msg(actions: List<NoticeAction>, acted: String? = null) =
        PresenceMsg(7L, "Renovar la tarjeta", null, "info", "pendientes", 1L, actions, "pend:abc", acted)

    @Test
    fun the_card_shows_buttons_only_for_a_notice_nobody_has_acted_on() {
        val buttons = listOf(NoticeAction("done", "Hecho"))
        assertEquals(NoticeButtons(7L, buttons), NoticeActions.forCard(msg(buttons)))
        assertNull(NoticeActions.forCard(msg(buttons, acted = "done")))   // another device already pressed one
        assertNull(NoticeActions.forCard(msg(emptyList())))
    }

    @Test
    fun the_inbox_parser_reads_buttons_ref_and_acted() {
        val body = """{"messages":[
            {"id":1,"text":"Renovar la tarjeta","kind":"info","source":"pendientes","ts":5,
             "actions":[{"id":"done","label":"Hecho"},{"id":"tomorrow","label":"Mañana"}],"ref":"pend:abc"},
            {"id":2,"text":"Ya hecho","kind":"info","source":"x","ts":6,"actions":[{"id":"done","label":"Hecho"}],
             "acted":{"action":"done","by":"pc","ts":7}},
            {"id":3,"text":"Sin botones","kind":"warn","source":"x","ts":8}]}"""
        val got = MochiPresence.parseInbox(body)
        assertEquals(listOf("done", "tomorrow"), got[0].actions.map { it.id })
        assertEquals("pend:abc", got[0].ref)
        assertNull(got[0].acted)
        assertEquals("done", got[1].acted)
        assertTrue(got[2].actions.isEmpty())
        assertNull(got[2].ref)
    }
}
