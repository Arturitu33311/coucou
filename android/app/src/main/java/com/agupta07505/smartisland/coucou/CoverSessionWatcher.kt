/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.content.ComponentName
import android.content.Context
import android.media.MediaMetadata
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.os.Handler
import android.os.Looper
import android.util.Log

/**
 * Watches the media sessions themselves. A player's notification can arrive before the new track's cover is in the
 * session (it sends the title first, the picture a moment later), so the cover wallpapers also listen to the session's
 * own metadata changes and update when the real picture lands.
 */
class CoverSessionWatcher(context: Context, private val listener: ComponentName) {
    private val app = context.applicationContext
    private val handler = Handler(Looper.getMainLooper())
    private val mgr = app.getSystemService(Context.MEDIA_SESSION_SERVICE) as? MediaSessionManager
    private val attached = HashMap<String, Pair<MediaController, MediaController.Callback>>()

    private val sessionsChanged = MediaSessionManager.OnActiveSessionsChangedListener { list -> attach(list.orEmpty()) }

    fun start() {
        val m = mgr ?: return
        runCatching {
            m.addOnActiveSessionsChangedListener(sessionsChanged, listener, handler)
            attach(m.getActiveSessions(listener))
        }.onFailure { Log.w(TAG, "could not watch the media sessions", it) }
    }

    fun stop() {
        runCatching { mgr?.removeOnActiveSessionsChangedListener(sessionsChanged) }
        attached.values.forEach { (c, cb) -> runCatching { c.unregisterCallback(cb) } }
        attached.clear()
    }

    private fun attach(list: List<MediaController>) {
        val ids = list.map { id(it) }.toSet()
        attached.keys.filter { it !in ids }.forEach { k ->
            attached.remove(k)?.let { (c, cb) -> runCatching { c.unregisterCallback(cb) } }
        }
        for (c in list) {
            val k = id(c)
            if (k in attached) continue
            val cb = object : MediaController.Callback() {
                override fun onMetadataChanged(metadata: MediaMetadata?) = push(c)
                override fun onPlaybackStateChanged(state: PlaybackState?) = push(c)
            }
            runCatching { c.registerCallback(cb, handler) }.onSuccess { attached[k] = c to cb }
            push(c)
        }
    }

    private fun id(c: MediaController) = c.packageName + "#" + c.sessionToken.hashCode()

    private fun push(c: MediaController) {
        val md = c.metadata ?: return
        val art = md.getBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART)
            ?: md.getBitmap(MediaMetadata.METADATA_KEY_ART)
            ?: md.getBitmap(MediaMetadata.METADATA_KEY_DISPLAY_ICON)
        val title = md.getString(MediaMetadata.METADATA_KEY_TITLE).orEmpty()
        val artist = md.getString(MediaMetadata.METADATA_KEY_ARTIST).orEmpty()
        val ps = c.playbackState
        val playing = ps?.state == PlaybackState.STATE_PLAYING
        val key = "${c.packageName}|$title|$artist"
        val album = md.getString(MediaMetadata.METADATA_KEY_ALBUM).orEmpty()
        Lyrics.onTrack(app, key, artist, title, album, md.getLong(MediaMetadata.METADATA_KEY_DURATION))
        if (ps != null) Lyrics.onState(ps.position, ps.playbackSpeed, playing, ps.lastPositionUpdateTime)
        LockCoverWallpaper.onPlayback(app, art, playing, key)
    }

    private companion object { const val TAG = "CoverWatcher" }
}
