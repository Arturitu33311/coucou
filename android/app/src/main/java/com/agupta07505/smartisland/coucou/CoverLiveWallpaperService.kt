/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RadialGradient
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Shader
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.service.wallpaper.WallpaperService
import android.view.SurfaceHolder
import kotlin.math.cos
import kotlin.math.sin

/**
 * PROTOTYPE of the animated cover wallpaper: the cover of what is playing, sharp and breathing, over a blurred copy that
 * drifts slowly, with two soft lights in the cover's own colours moving behind it. Draws only while it is visible, ~30 fps,
 * and does nothing else: no network, no sensors. With nothing playing it shows the dark teal base with the same lights.
 */
class CoverLiveWallpaperService : WallpaperService() {
    override fun onCreateEngine(): Engine = CoverEngine()

    private inner class CoverEngine : Engine() {
        private val main = Handler(Looper.getMainLooper())
        private val t0 = SystemClock.uptimeMillis()
        private var visible = false
        private var w = 0
        private var h = 0

        private var shownKey: String? = null
        private var square: Bitmap? = null      // the cover, square, ready to draw
        private var backdrop: Bitmap? = null    // tiny copy, upscaled = blur
        private var c1 = Color.rgb(20, 120, 110)
        private var c2 = Color.rgb(30, 90, 70)
        private var accent = Color.argb(150, 94, 234, 212)

        private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
        private val blob1 = Paint(Paint.ANTI_ALIAS_FLAG)
        private val blob2 = Paint(Paint.ANTI_ALIAS_FLAG)
        private val clip = Path()

        private val tick = object : Runnable {
            override fun run() {
                draw()
                if (visible) main.postDelayed(this, 33)
            }
        }

        override fun onSurfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
            w = width; h = height
            val r = width * 0.75f
            blob1.shader = RadialGradient(0f, 0f, r, intArrayOf(withAlpha(c1, 150), withAlpha(c1, 0)), null, Shader.TileMode.CLAMP)
            blob2.shader = RadialGradient(0f, 0f, r, intArrayOf(withAlpha(c2, 130), withAlpha(c2, 0)), null, Shader.TileMode.CLAMP)
            draw()
        }

        override fun onVisibilityChanged(v: Boolean) {
            visible = v
            main.removeCallbacks(tick)
            if (v) main.post(tick)
        }

        override fun onSurfaceDestroyed(holder: SurfaceHolder) {
            visible = false
            main.removeCallbacks(tick)
        }

        private fun refresh() {
            val now = CoverBus.current()
            // paused for a while: back to the dark base, like the still wallpaper puts the old one back
            val idle = !now.playing && System.currentTimeMillis() - now.at > 90_000L
            val cover = if (idle) null else now.cover
            val id = if (idle) "idle" else now.key + "#" + CoverBus.signature(cover)
            if (id == shownKey) return
            shownKey = id
            if (cover == null || cover.isRecycled || cover.width < 32) {
                square = null; backdrop = null
                // nothing to show: the lights go back to Mochi's teal
                c1 = Color.rgb(20, 120, 110); c2 = Color.rgb(30, 90, 70)
                accent = Color.argb(150, 94, 234, 212)
                if (w > 0) onSurfaceChanged(surfaceHolder, 0, w, h)
                return
            }
            val sq = minOf(cover.width, cover.height)
            val cropped = Bitmap.createBitmap(cover, (cover.width - sq) / 2, (cover.height - sq) / 2, sq, sq)
            square = Bitmap.createScaledBitmap(cropped, 720, 720, true)
            backdrop = Bitmap.createScaledBitmap(cropped, 24, 24, true)
            val left = avg(backdrop!!, 0, 12)
            val right = avg(backdrop!!, 12, 24)
            c1 = boost(left); c2 = boost(right)
            accent = withAlpha(boost(avg(backdrop!!, 0, 24)), 150)
            if (w > 0) onSurfaceChanged(surfaceHolder, 0, w, h)
        }

        private fun draw() {
            val holder = surfaceHolder ?: return
            refresh()
            val c = try { holder.lockCanvas() } catch (_: Throwable) { null } ?: return
            try {
                paint2(c)
            } finally {
                holder.unlockCanvasAndPost(c)
            }
        }

        private fun paint2(c: Canvas) {
            if (w == 0 || h == 0) return
            val t = (SystemClock.uptimeMillis() - t0) / 1000f
            c.drawColor(Color.rgb(7, 19, 17))

            // blurred copy drifting slowly (Ken Burns)
            backdrop?.let { b ->
                val k = maxOf(w.toFloat() / b.width, h.toFloat() / b.height) * (1.18f + 0.05f * sin(t / 9f))
                c.save()
                c.translate(w / 2f + 40f * sin(t / 13f), h / 2f + 40f * cos(t / 11f))
                c.scale(k, k)
                c.drawBitmap(b, -b.width / 2f, -b.height / 2f, paint)
                c.restore()
            }
            c.drawColor(Color.argb(if (backdrop != null) 168 else 0, 7, 19, 17))

            // two soft lights in the cover's colours
            c.save()
            c.translate(w * (0.30f + 0.22f * sin(t / 7f)), h * (0.28f + 0.12f * cos(t / 9f)))
            c.drawCircle(0f, 0f, w * 0.75f, blob1)
            c.restore()
            c.save()
            c.translate(w * (0.72f + 0.20f * cos(t / 8f)), h * (0.78f + 0.10f * sin(t / 10f)))
            c.drawCircle(0f, 0f, w * 0.75f, blob2)
            c.restore()

            // the cover itself, breathing a little
            val sq = square ?: return
            val side = w * 0.80f * (1f + 0.012f * sin(t / 3f))
            val cx = w / 2f
            val cy = h * 0.60f + 6f * sin(t / 4f)
            val box = RectF(cx - side / 2, cy - side / 2, cx + side / 2, cy + side / 2)
            val r = side * 0.07f
            c.save()
            c.rotate(1.0f * sin(t / 7f), cx, cy)
            paint.style = Paint.Style.FILL
            paint.color = Color.argb(90, 0, 0, 0)
            c.drawRoundRect(RectF(box.left, box.top + side * 0.025f, box.right, box.bottom + side * 0.025f), r, r, paint)
            clip.reset(); clip.addRoundRect(box, r, r, Path.Direction.CW)
            c.save(); c.clipPath(clip)
            c.drawBitmap(sq, Rect(0, 0, sq.width, sq.height), box, paint)
            c.restore()
            paint.style = Paint.Style.STROKE
            paint.strokeWidth = 2f
            paint.color = accent
            c.drawRoundRect(box, r, r, paint)
            paint.style = Paint.Style.FILL
            c.restore()
        }

        private fun avg(b: Bitmap, x0: Int, x1: Int): Int {
            var r = 0L; var g = 0L; var bl = 0L; var n = 0
            for (y in 0 until b.height) for (x in x0 until x1) {
                val p = b.getPixel(x, y); r += Color.red(p); g += Color.green(p); bl += Color.blue(p); n++
            }
            n = n.coerceAtLeast(1)
            return Color.rgb((r / n).toInt(), (g / n).toInt(), (bl / n).toInt())
        }

        private fun boost(color: Int): Int {
            val hsv = FloatArray(3)
            Color.colorToHSV(color, hsv)
            hsv[1] = (hsv[1] * 1.25f).coerceIn(0.4f, 1f)
            hsv[2] = hsv[2].coerceIn(0.55f, 0.95f)
            return Color.HSVToColor(hsv)
        }

        private fun withAlpha(color: Int, a: Int) = Color.argb(a, Color.red(color), Color.green(color), Color.blue(color))
    }
}
