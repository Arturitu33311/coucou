/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.audiolink

/** What the laptop needs to be told about the audio relay's own ssh key. */
object AudioLinkSetup {
    /** The name of the second key (see `PhoneKey`). */
    const val KEY = "audio"

    /**
     * The line to paste in the laptop's `~/.ssh/authorized_keys`: this key may run exactly one command,
     * `coucou-hook --audio`, which only pipes to the audio relay's socket.
     */
    fun authorizedLine(user: String, publicLine: String): String =
        "restrict,command=\"/home/$user/.local/share/coucou/bin/coucou-hook --audio\" $publicLine"
}
