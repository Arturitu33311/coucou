/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.audiolink

/**
 * The audio relay's brain, with no I/O. The laptop runs the same function in Rust (`audiolink_logic.rs`);
 * both read `audiolink-vectors.json`, so they cannot drift apart.
 *
 * `Off`: the laptop plays on its own speakers. `Relay`: its sound goes to a silent sink and from there to
 * this phone. `Hold`: the headphones are on the phone but the stream cannot run (link lost, a call, a short
 * Bluetooth drop); the laptop stays silent.
 */
enum class Mode { Off, Relay, Hold }

/** Which device holds the headphones, as far as we know. */
enum class Where { Phone, Pc, None }

sealed interface Event {
    data class Enabled(val on: Boolean) : Event
    data class Headphones(val at: Where) : Event
    data class Link(val up: Boolean) : Event
    data class Call(val on: Boolean) : Event
    data class PhoneMedia(val playing: Boolean) : Event
    data class PcMedia(val playing: Boolean) : Event

    /** The short wait after the headphones disappeared from the phone is over. */
    data object GraceExpired : Event
}

enum class Action { SinkVirtual, SinkRestore, RelayStart, RelayStop, PausePc, PausePhone }

data class State(
    val mode: Mode = Mode.Off,
    val enabled: Boolean = false,
    val headphones: Where = Where.None,
    val linkUp: Boolean = false,
    val call: Boolean = false,
    val phonePlaying: Boolean = false,
    val pcPlaying: Boolean = false,
)

object AudioLinkLogic {
    private fun State.armed() = enabled && headphones == Where.Phone
    private fun State.ready() = armed() && linkUp && !call

    /** Takes one event in; returns the next state and what to do about it. */
    fun step(state: State, ev: Event): Pair<State, List<Action>> {
        var s = when (ev) {
            is Event.Enabled -> state.copy(enabled = ev.on)
            is Event.Headphones -> state.copy(headphones = ev.at)
            is Event.Link -> state.copy(linkUp = ev.up)
            is Event.Call -> state.copy(call = ev.on)
            is Event.PhoneMedia -> state.copy(phonePlaying = ev.playing)
            is Event.PcMedia -> state.copy(pcPlaying = ev.playing)
            Event.GraceExpired -> state
        }
        val out = mutableListOf<Action>()
        when (s.mode) {
            Mode.Off -> when {
                s.ready() -> {
                    s = s.copy(mode = Mode.Relay)
                    out += listOf(Action.SinkVirtual, Action.RelayStart)
                }
                s.armed() -> {
                    s = s.copy(mode = Mode.Hold)
                    out += listOf(Action.SinkVirtual, Action.PausePc)
                }
            }
            Mode.Relay -> when {
                !s.enabled || s.headphones == Where.Pc -> {
                    s = s.copy(mode = Mode.Off)
                    out += listOf(Action.RelayStop, Action.SinkRestore)
                }
                !s.ready() -> {
                    s = s.copy(mode = Mode.Hold)
                    out += listOf(Action.RelayStop, Action.PausePc)
                }
            }
            Mode.Hold -> {
                val gone = ev == Event.GraceExpired && s.headphones != Where.Phone
                when {
                    !s.enabled || s.headphones == Where.Pc || gone -> {
                        s = s.copy(mode = Mode.Off)
                        out += Action.SinkRestore
                    }
                    s.ready() -> {
                        s = s.copy(mode = Mode.Relay)
                        out += Action.RelayStart
                    }
                }
            }
        }
        // Last one to start wins: what starts on one device pauses the other.
        when {
            ev == Event.PhoneMedia(true) && s.mode == Mode.Relay && s.pcPlaying -> out += Action.PausePc
            ev == Event.PcMedia(true) && s.mode == Mode.Relay && s.phonePlaying -> out += Action.PausePhone
            ev == Event.PcMedia(true) && s.mode == Mode.Hold -> out += Action.PausePc
        }
        return s to out
    }
}
