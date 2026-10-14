/*
 * Smart Island (2026)
 * © Animesh Gupta — github.com/agupta07505
 * Licensed under the GNU GPL v3 License
 * Do not remove or alter this notice. - Per GPL-3.0 Section 4 & Section 5
 */

package com.agupta07505.smartisland.model

enum class IslandMode {
    Empty,
    Notification,
    IncomingCall,
    Music,
    Battery,
    LiveActivity,
    Navigation,
    DownloadUpload,
    Hotspot,
    Bluetooth,
    Flashlight,
    ScreenRecording,
    Timer,
    Stopwatch,
    /** Coucou: a task's reminder has gone off. */
    Reminder,
    /** Coucou: Claude Code is working on the laptop. */
    Agent,
    /** Coucou: Claude Code asks for permission. */
    Approval,
    /** Coucou: Claude Code asks a question. */
    Question,
    /** Coucou: a text of Jinx's on the island. */
    JinxMsg,
    /** Coucou: Mochi, always there, with the tasks (it opens on his desk). */
    Desk
}

/** Coucou's own modes: they have their own life (Mochi's desk, a reminder, Claude Code) and are never torn down as "an app's notifications". */
val IslandMode.isCoucou: Boolean
    get() = this == IslandMode.Reminder || this == IslandMode.Agent || this == IslandMode.Approval ||
        this == IslandMode.Question || this == IslandMode.JinxMsg || this == IslandMode.Desk

/**
 * How much each mode matters, so the island rests on the most important one and the rest wait beside it:
 * what needs you now, then what is happening, then what is just there. Mochi sits above the battery ring.
 */
val IslandMode.priority: Int
    get() = when (this) {
        IslandMode.IncomingCall -> 110
        IslandMode.Approval, IslandMode.Question -> 100
        IslandMode.Reminder -> 90
        IslandMode.Timer, IslandMode.Stopwatch -> 80
        IslandMode.Navigation -> 75
        IslandMode.ScreenRecording, IslandMode.LiveActivity -> 70
        IslandMode.DownloadUpload -> 60
        IslandMode.Hotspot, IslandMode.Bluetooth, IslandMode.Flashlight -> 55
        // Claude Code merely working is background: it shows only when nothing else has anything to
        // say (above Mochi's desk, below music, news and the rest). What asks for the person is raised
        // by effectivePriority (ATTENTION_PRIORITY), and real requests are Approval / Question.
        IslandMode.Agent -> 30
        IslandMode.JinxMsg -> 95
        IslandMode.Music -> 50
        IslandMode.Notification -> 40
        IslandMode.Desk -> 20
        IslandMode.Battery -> 10
        IslandMode.Empty -> 0
    }

/** Music that yields the throne but keeps its card: paused players and remote mirrors (KDE's
 * PC player). Below news and tasks, above the battery ring. Only music actually sounding on this
 * phone keeps full priority. */
const val YIELDED_MUSIC_PRIORITY = 15

/** From here up, the island is waiting for the person (a call, a permission, a question, a reminder). */
const val WAITING_PRIORITY = 90

/** The category of the Claude Code card when a session needs the person (see IslandAgents). */
const val AGENT_NEEDS_YOU = "needs_you"

/** A session that needs the person, whose request cannot be answered from the phone: just under a real question. */
const val ATTENTION_PRIORITY = 99
