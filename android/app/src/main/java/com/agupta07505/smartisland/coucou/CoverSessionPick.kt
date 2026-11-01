/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

/** What the cover wallpapers need to know about a media session to choose between them. */
data class SessionView(val packageName: String, val hasTrack: Boolean, val playing: Boolean)

/**
 * Sessions that are not what this phone plays itself. KDE Connect hands the laptop's players back to the phone as
 * sessions, and the laptop lists the phone's own player among them ("Spotify (Phone)"), so what arrives is an echo of
 * the phone's earlier track, late and often still marked as playing when nothing sounds. The audio link's quiet
 * notification session is ours. Neither is ever followed for the cover, the title or the lyrics.
 */
val MIRROR_PACKAGES = setOf("org.kde.kdeconnect_tp", "com.agupta07505.smartisland")

/**
 * Which session the cover, title and lyrics follow (an index into `sessions`, or -1): one only, so they never come
 * from two players at once. A player actually sounding on this phone comes first, then a real player that is paused
 * (its last track). A mirror (see `MIRROR_PACKAGES`) and a session with no track are never picked. Among equals the
 * system's own order wins, which puts the most recently active first.
 */
fun pickSession(sessions: List<SessionView>): Int {
    val real = sessions.indices.filter { sessions[it].hasTrack && sessions[it].packageName !in MIRROR_PACKAGES }
    return real.firstOrNull { sessions[it].playing } ?: real.firstOrNull() ?: -1
}
