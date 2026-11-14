package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.ui.MochiReactions
import com.agupta07505.smartisland.coucou.ui.ReactionLedger
import com.agupta07505.smartisland.model.IslandMode
import com.agupta07505.smartisland.model.IslandNotification
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Mochi reacts once per notification: not again for a list change, not when a second face appears, not never for an update. */
class MochiNewsTest {
    private val t0 = 1_000_000_000L

    private fun note(key: String, at: Long = t0, text: String = "hola", mode: IslandMode = IslandMode.Notification) =
        IslandNotification(key = key, packageName = "com.example", appName = "Example", title = "t", text = text, timeMillis = at, mode = mode)

    @Test
    fun one_arrival_is_one_reaction_even_if_the_list_changes_while_it_is_still_recent() {
        val n = note("w1")
        var scan = MochiReactions.scanNews(listOf(n), t0 + 100, emptySet(), started = false)
        assertEquals(1, scan.news.size)
        // the surprise has closed; an unrelated change re-emits the list 3 s later: the old rule jumped again
        scan = MochiReactions.scanNews(listOf(n, note("music", mode = IslandMode.Music, at = t0 + 3_000)), t0 + 3_100, scan.seenAfter, started = true)
        assertEquals("only the music card is new, not the notification again", 1, scan.news.size)
        assertTrue(scan.news[0].startsWith("music@"))
        val again = MochiReactions.scanNews(listOf(n), t0 + 3_500, scan.seenAfter, started = true)
        assertTrue(again.news.isEmpty())
    }

    @Test
    fun a_card_that_was_already_there_when_he_appears_is_not_news() {
        val old = note("w1", at = t0 - 60_000)
        assertTrue(MochiReactions.scanNews(listOf(old), t0, emptySet(), started = false).news.isEmpty())
    }

    @Test
    fun an_update_with_new_words_is_news_again_even_if_the_card_keeps_its_old_time() {
        val first = note("chat", at = t0 - 600_000, text = "hola")
        var scan = MochiReactions.scanNews(listOf(first), t0, emptySet(), started = false)
        assertTrue("old at first look", scan.news.isEmpty())
        val updated = first.copy(text = "¿vienes?")          // same key, same arrival time, new message
        scan = MochiReactions.scanNews(listOf(updated), t0 + 1_000, scan.seenAfter, started = true)
        assertEquals("the old rule saw nothing: the time was not recent", 1, scan.news.size)
    }

    @Test
    fun a_second_face_that_appears_later_does_not_react_again_but_one_drawn_at_the_same_time_does() {
        val n = note("w1")
        val id = MochiReactions.newsId(n)
        ReactionLedger.note(id, t0)
        // the island unfolds 2 s later and a new face looks at the same recent card
        val late = MochiReactions.scanNews(listOf(n), t0 + 2_000, emptySet(), started = false, reactedEarlier = { ReactionLedger.reactedEarlier(it, t0 + 2_000) })
        assertTrue("not a second reaction", late.news.isEmpty())
        // another face drawn in the same instant (the bubble and the desk card) still reacts
        val same = MochiReactions.scanNews(listOf(n), t0 + 100, emptySet(), started = false, reactedEarlier = { ReactionLedger.reactedEarlier(it, t0 + 100) })
        assertEquals(1, same.news.size)
    }

    @Test
    fun his_own_cards_are_never_news_and_the_seen_set_stays_bounded() {
        assertTrue(MochiReactions.scanNews(listOf(note("coucou_approval_1", mode = IslandMode.Approval)), t0, emptySet(), true).news.isEmpty())
        var seen = emptySet<String>()
        for (i in 0 until 300) seen = MochiReactions.scanNews(listOf(note("k$i", at = t0 + i)), t0 + i, seen, true).seenAfter
        assertTrue(seen.size <= 100)
    }

    @Test
    fun the_pop_for_news_is_not_hidden_by_the_unlock_hello() {
        val scene = com.agupta07505.smartisland.coucou.ui.MochiScene(surprised = true, greeted = true)
        assertTrue(MochiReactions.surpriseOverGreeting(scene))
        assertFalse(MochiReactions.surpriseOverGreeting(com.agupta07505.smartisland.coucou.ui.MochiScene(greeted = true)))
    }
}
