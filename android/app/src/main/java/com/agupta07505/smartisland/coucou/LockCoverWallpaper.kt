/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.app.WallpaperManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.os.Handler
import android.os.Looper
import android.util.Log
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.lockCover
import java.io.File
import kotlin.concurrent.thread

/**
 * The cover of the song that is playing, as the lock screen wallpaper: the cover itself, sharp and centred a little
 * low (the clock lives at the top), over its own blurred, darkened copy. Nothing leaves the phone: the cover is the one
 * the player already gave the media session.
 *
 * What was on the lock screen before is kept in `filesDir` the first time and put back ~90 s after the music stops.
 */
object LockCoverWallpaper {
    private const val TAG = "LockCover"
    private const val RESTORE_DELAY_MS = 90_000L
    private const val MIN_GAP_MS = 4_000L

    private val main = Handler(Looper.getMainLooper())
    private var lastKey: String? = null
    private var lastAt = 0L
    private var restoring: Runnable? = null
    @Volatile private var busy = false

    /** [key] changes with the track; [cover] may be null (nothing to show). */
    fun onPlayback(context: Context, cover: Bitmap?, playing: Boolean, key: String) {
        val app = context.applicationContext
        CoverBus.publish(cover, playing, key)
        if (!app.lockCover || liveActive(app)) return
        if (!playing) {
            scheduleRestore(app)
            return
        }
        cancelRestore()
        if (cover == null || cover.isRecycled || cover.width < 64) return
        val now = System.currentTimeMillis()
        val id = key + "#" + CoverBus.signature(cover)
        if (id == lastKey || busy || now - lastAt < MIN_GAP_MS) return
        busy = true
        lastKey = id
        lastAt = now
        val copy = cover.copy(Bitmap.Config.ARGB_8888, false) ?: run { busy = false; return }
        thread(name = "lock-cover") {
            try {
                val wm = WallpaperManager.getInstance(app)
                keepOriginal(app, wm)
                val dm = app.resources.displayMetrics
                wm.setBitmap(compose(copy, dm.widthPixels, dm.heightPixels), null, true, WallpaperManager.FLAG_LOCK)
            } catch (t: Throwable) {
                Log.w(TAG, "could not set the lock cover", t)
                lastKey = null
            } finally {
                busy = false
            }
        }
    }

    /** The animated cover wallpaper owns the lock screen: setting a still image would replace it. */
    private fun liveActive(app: Context): Boolean = runCatching {
        android.os.Build.VERSION.SDK_INT >= 34 &&
            WallpaperManager.getInstance(app).getWallpaperInfo(WallpaperManager.FLAG_LOCK)?.serviceName ==
            CoverLiveWallpaperService::class.java.name
    }.getOrDefault(false)

    /** Put back what was there. Called by the timer, and when the user turns the switch off. */
    fun restore(context: Context) {
        val app = context.applicationContext
        cancelRestore()
        lastKey = null
        if (liveActive(app)) return
        thread(name = "lock-cover-restore") {
            try {
                val wm = WallpaperManager.getInstance(app)
                val saved = originalFile(app)
                if (saved.exists()) {
                    android.graphics.BitmapFactory.decodeFile(saved.path)?.let {
                        wm.setBitmap(it, null, true, WallpaperManager.FLAG_LOCK)
                    }
                } else {
                    wm.clear(WallpaperManager.FLAG_LOCK)
                }
            } catch (t: Throwable) {
                Log.w(TAG, "could not restore the lock wallpaper", t)
            }
        }
    }

    private fun scheduleRestore(app: Context) {
        if (lastKey == null || restoring != null) return
        val r = Runnable { restoring = null; restore(app) }
        restoring = r
        main.postDelayed(r, RESTORE_DELAY_MS)
    }

    private fun cancelRestore() {
        restoring?.let { main.removeCallbacks(it) }
        restoring = null
    }

    private fun originalFile(app: Context) = File(app.filesDir, "lock-original.png")

    private fun keepOriginal(app: Context, wm: WallpaperManager) {
        val f = originalFile(app)
        if (f.exists()) return
        runCatching {
            wm.getWallpaperFile(WallpaperManager.FLAG_LOCK)?.use { pfd ->
                android.graphics.BitmapFactory.decodeFileDescriptor(pfd.fileDescriptor)?.let { b ->
                    f.outputStream().use { out -> b.compress(Bitmap.CompressFormat.PNG, 100, out) }
                }
            }
        }.onFailure { Log.i(TAG, "no readable lock wallpaper to keep: ${it.message}") }
    }

    private fun compose(cover: Bitmap, w: Int, h: Int): Bitmap {
        val out = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
        val c = Canvas(out)
        val p = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)

        // backdrop: the cover scaled to fill, blurred by going down to 1/28 and back up, then darkened
        val small = Bitmap.createScaledBitmap(cover, 28, (28f * h / w).toInt().coerceAtLeast(28), true)
        val k = maxOf(w.toFloat() / small.width, h.toFloat() / small.height)
        c.save()
        c.translate((w - small.width * k) / 2f, (h - small.height * k) / 2f)
        c.scale(k, k)
        c.drawBitmap(small, 0f, 0f, p)
        c.restore()
        c.drawColor(Color.argb(176, 7, 19, 17))

        // the cover, sharp, square, rounded, centred a little low
        val side = (w * 0.80f)
        val left = (w - side) / 2f
        val top = h * 0.60f - side / 2f
        val box = RectF(left, top, left + side, top + side)
        val r = side * 0.07f
        p.color = Color.argb(90, 0, 0, 0)
        c.drawRoundRect(RectF(box.left, box.top + side * 0.025f, box.right, box.bottom + side * 0.025f), r, r, p)
        val clip = Path().apply { addRoundRect(box, r, r, Path.Direction.CW) }
        c.save()
        c.clipPath(clip)
        val sq = minOf(cover.width, cover.height)
        c.drawBitmap(
            cover,
            android.graphics.Rect((cover.width - sq) / 2, (cover.height - sq) / 2, (cover.width + sq) / 2, (cover.height + sq) / 2),
            box, p,
        )
        c.restore()
        p.style = Paint.Style.STROKE
        p.strokeWidth = 2f
        p.color = accentOf(small)
        c.drawRoundRect(box, r, r, p)
        return out
    }

    /** The cover's own average colour, brightened, so the frame belongs to the song. */
    private fun accentOf(small: Bitmap): Int {
        var r = 0L; var g = 0L; var b = 0L
        val px = IntArray(small.width * small.height).also { small.getPixels(it, 0, small.width, 0, 0, small.width, small.height) }
        for (v in px) { r += Color.red(v); g += Color.green(v); b += Color.blue(v) }
        val n = px.size.coerceAtLeast(1)
        val hsv = FloatArray(3)
        Color.RGBToHSV((r / n).toInt(), (g / n).toInt(), (b / n).toInt(), hsv)
        hsv[1] = (hsv[1] * 1.2f).coerceIn(0.35f, 1f)
        hsv[2] = 0.9f
        return Color.HSVToColor(150, hsv)
    }
}
