package com.agupta07505.smartisland.coucou.audiolink

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The laptop's test (`audiolink_logic.rs`) reads the same file. */
class AudioLinkLogicTest {
    private val doc = JSONObject(
        javaClass.classLoader!!.getResourceAsStream("audiolink-vectors.json")!!.bufferedReader().readText(),
    )

    private fun event(v: JSONObject): Event = when (val t = v.getString("t")) {
        "enabled" -> Event.Enabled(v.getBoolean("on"))
        "headphones" -> Event.Headphones(
            when (val at = v.getString("at")) {
                "phone" -> Where.Phone
                "pc" -> Where.Pc
                "none" -> Where.None
                else -> error("unknown place $at")
            },
        )
        "link" -> Event.Link(v.getBoolean("up"))
        "call" -> Event.Call(v.getBoolean("on"))
        "phone_media" -> Event.PhoneMedia(v.getBoolean("playing"))
        "pc_media" -> Event.PcMedia(v.getBoolean("playing"))
        "grace_expired" -> Event.GraceExpired
        else -> error("unknown event $t")
    }

    private fun actionName(a: Action) = when (a) {
        Action.SinkVirtual -> "sink_virtual"
        Action.SinkRestore -> "sink_restore"
        Action.RelayStart -> "relay_start"
        Action.RelayStop -> "relay_stop"
        Action.PausePc -> "pause_pc"
        Action.PausePhone -> "pause_phone"
    }

    @Test
    fun every_shared_state_case_holds() {
        val cases = doc.getJSONArray("state_cases")
        assertTrue("the vector file lost its cases", cases.length() >= 10)
        for (c in 0 until cases.length()) {
            val case = cases.getJSONObject(c)
            val name = case.getString("name")
            var state = State()
            val steps = case.getJSONArray("steps")
            for (i in 0 until steps.length()) {
                val s = steps.getJSONObject(i)
                val (next, actions) = AudioLinkLogic.step(state, event(s.getJSONObject("event")))
                state = next
                val want = (0 until s.getJSONArray("actions").length()).map { s.getJSONArray("actions").getString(it) }
                assertEquals("$name, step $i: actions", want, actions.map(::actionName))
                assertEquals("$name, step $i: mode", s.getString("mode"), state.mode.name.lowercase())
            }
        }
    }
}
