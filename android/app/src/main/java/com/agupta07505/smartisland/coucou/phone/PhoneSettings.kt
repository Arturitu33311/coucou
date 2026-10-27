/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.phone

import android.content.Context

/** The two hosts the phone talks to, and the two switches. Plain preferences: no secret is kept here. */
object PhoneSettings {
    const val SERVER = "server"
    const val LAPTOP = "laptop"

    private fun prefs(c: Context) = c.applicationContext.getSharedPreferences("coucou_phone", Context.MODE_PRIVATE)

    fun profile(c: Context, name: String) = SshProfile(
        host = prefs(c).getString("$name.host", "") ?: "",
        port = prefs(c).getInt("$name.port", 22),
        user = prefs(c).getString("$name.user", "") ?: "",
        hostKey = prefs(c).getString("$name.hostKey", "") ?: "",
    )

    fun save(c: Context, name: String, p: SshProfile) {
        prefs(c).edit()
            .putString("$name.host", p.host.trim())
            .putInt("$name.port", p.port)
            .putString("$name.user", p.user.trim())
            .putString("$name.hostKey", p.hostKey)
            .apply()
    }

    /** Anything about the host changed: what was trusted is no longer known to be this host. */
    fun changedHost(old: SshProfile, new: SshProfile) = old.host != new.host.trim() || old.port != new.port

    /** Share the tasks with Jinx's pendientes (over the server profile). */
    var Context.jinxSync: Boolean
        get() = prefs(this).getBoolean("jinxSync", false)
        set(v) = prefs(this).edit().putBoolean("jinxSync", v).apply()

    /**
     * Which phone this is (s21, s22…): what it calls itself in the beats the laptop relays and in the
     * presence server's heartbeat. By default it comes from the model, so each phone is right without
     * being told; Settings can fix it.
     */
    fun deviceId(c: Context): String {
        val saved = prefs(c).getString("deviceId", null)
        return saved?.takeIf { it in com.agupta07505.smartisland.coucou.presence.Arbiter.PHONES }
            ?: com.agupta07505.smartisland.coucou.presence.Arbiter.idForModel(android.os.Build.MODEL)
    }

    fun saveDeviceId(c: Context, id: String) {
        prefs(c).edit().putString("deviceId", id).apply()
    }

    /** Put the cover of what is playing on the lock screen (off by default: it replaces the lock wallpaper while music plays). */
    var Context.lockCover: Boolean
        get() = prefs(this).getBoolean("lockCover", false)
        set(v) = prefs(this).edit().putBoolean("lockCover", v).apply()

    /** Show synced lyrics beside the record in the car wallpaper (off by default: it asks lrclib.net for them). */
    var Context.lyricsOn: Boolean
        get() = prefs(this).getBoolean("lyricsOn", false)
        set(v) = prefs(this).edit().putBoolean("lyricsOn", v).apply()

    /** What Mochi wears here: "auto" (the seasons, then what he is doing), "none" or an outfit id. */
    var Context.mochiOutfit: String
        get() = prefs(this).getString("mochiOutfit", "auto") ?: "auto"
        set(v) = prefs(this).edit().putString("mochiOutfit", v).apply()

    /** Keep Mochi in the island, with the tasks (on by default). */
    var Context.showDesk: Boolean
        get() = prefs(this).getBoolean("showDesk", true)
        set(v) = prefs(this).edit().putBoolean("showDesk", v).apply()

    /** Follow Claude Code on the laptop and answer its requests (over the laptop profile). */
    var Context.followClaude: Boolean
        get() = prefs(this).getBoolean("followClaude", false)
        set(v) = prefs(this).edit().putBoolean("followClaude", v).apply()

    /** Hear Jinx's texts on this phone through presence (text only, no voice). */
    var Context.followJinx: Boolean
        get() = prefs(this).getBoolean("followJinx", false)
        set(v) = prefs(this).edit().putBoolean("followJinx", v).apply()

    /** Where presence lives (Tailscale base URL, no trailing slash). */
    fun presenceUrl(c: Context): String =
        prefs(c).getString("presenceUrl", "http://100.92.254.125:8897")
            ?: "http://100.92.254.125:8897"

    fun savePresenceUrl(c: Context, v: String) {
        prefs(c).edit().putString("presenceUrl", v.trim().trimEnd('/')).apply()
    }

    /** Key for the presence inbox (same as ~/.config/mochi-presence/key on the server). */
    fun mochiKey(c: Context): String = prefs(c).getString("mochiKey", "") ?: ""

    fun saveMochiKey(c: Context, v: String) {
        prefs(c).edit().putString("mochiKey", v.trim()).apply()
    }
}
