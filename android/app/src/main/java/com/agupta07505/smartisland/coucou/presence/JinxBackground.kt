/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.presence

import com.agupta07505.smartisland.coucou.assist.JinxMood
import com.agupta07505.smartisland.coucou.ui.MochiMood
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Jinx's background mood: what she feels between messages, from her emotion state on the server (the presence
 * server sends it with every heartbeat answer as `jinx_mood`). Her Mochi wears it whenever nothing else moves
 * him. Null: she is calm, or the server has not said (it keeps the last word while it is out of reach).
 */
object JinxBackground {
    private val _emote = MutableStateFlow<String?>(null)

    /** One of Mochi's emotes, or null. */
    val emote: StateFlow<String?> = _emote.asStateFlow()

    fun set(emote: String?) {
        _emote.value = JinxMood.validEmote(emote)
    }

    /** Her background mood as Mochi's face. */
    fun mood(): MochiMood? = JinxMood.moodOf(_emote.value)
}
