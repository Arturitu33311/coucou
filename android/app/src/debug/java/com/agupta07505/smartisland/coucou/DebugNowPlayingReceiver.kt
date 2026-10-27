/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader

/**
 * Debug builds only. `adb shell am broadcast -n <pkg>/.coucou.DebugNowPlayingReceiver --es title X --ez playing true`
 * publishes a generated cover to [CoverBus], so the wallpaper can be tested without a player. Also creates the folder
 * the car photo is read from.
 */
class DebugNowPlayingReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        context.getExternalFilesDir("car")
        val title = intent.getStringExtra("title") ?: "Test song"
        val playing = intent.getBooleanExtra("playing", true)
        val hue = intent.getFloatExtra("hue", (title.hashCode() and 0x7fffffff) % 360.toFloat())
        CoverBus.publish(cover(hue), playing, "debug|$title|artist")
    }

    private fun cover(hue: Float): Bitmap {
        val b = Bitmap.createBitmap(640, 640, Bitmap.Config.ARGB_8888)
        val c = Canvas(b)
        val p = Paint(Paint.ANTI_ALIAS_FLAG)
        p.shader = LinearGradient(
            0f, 0f, 640f, 640f,
            Color.HSVToColor(floatArrayOf(hue, 0.85f, 0.95f)), Color.HSVToColor(floatArrayOf((hue + 70f) % 360f, 0.9f, 0.45f)),
            Shader.TileMode.CLAMP,
        )
        c.drawRect(0f, 0f, 640f, 640f, p)
        p.shader = null
        p.color = Color.argb(200, 255, 255, 255)
        c.drawCircle(320f, 320f, 150f, p)
        p.color = Color.argb(255, 20, 20, 20)
        c.drawRect(250f, 290f, 390f, 350f, p)
        return b
    }
}
