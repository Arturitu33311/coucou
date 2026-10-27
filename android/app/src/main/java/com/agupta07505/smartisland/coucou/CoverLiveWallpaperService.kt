/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RadialGradient
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.SweepGradient
import android.os.SystemClock
import android.service.wallpaper.WallpaperService
import android.text.StaticLayout
import android.text.TextPaint
import android.util.Log
import android.view.Choreographer
import android.view.SurfaceHolder
import androidx.core.graphics.ColorUtils
import kotlin.math.cos
import kotlin.math.sin

/**
 * Animated cover wallpaper: the cover of what is playing, sharp and breathing, over a blurred copy that drifts slowly,
 * with two soft lights in the cover's own colours, a record that turns with the music and motes that drift up.
 * No network. The car variant reads the gravity sensor (no permission) only while visible.
 *
 * Drawing is done on the GPU (hardware canvas) once per vsync, capped at ~60 fps, and only while visible. Nothing
 * expensive is done per frame: the clock is rendered into two bitmaps once a minute, shaders are built once, and the
 * cover is only re-processed when it changes.
 */
open class CoverLiveWallpaperService : WallpaperService() {
    /** The "car" variant: a photo with the clock BEHIND the car (cut-out in front), the music effects on the floor. */
    protected open val carMode: Boolean = false

    /** The "home" variant: no cover or record, only the slow lights and motes, in the cover's colours and the hour's. */
    protected open val homeMode: Boolean = false

    override fun onCreateEngine(): Engine = CoverEngine()

    private inner class CoverEngine : Engine(), Choreographer.FrameCallback {
        private val choreographer: Choreographer by lazy { Choreographer.getInstance() }
        private val t0 = SystemClock.uptimeMillis()
        private var visible = false
        private var w = 0
        private var h = 0

        // frame pacing and a measurement of it (logged every 2 s as "CoverFps")
        private var lastFrameNs = 0L
        private var statFrames = 0
        private var statDrawNs = 0L
        private var statWorstNs = 0L
        private var statSince = 0L

        private var lastNow: CoverBus.Now? = null
        private var lastIdle = false
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
        private val dot = Paint(Paint.ANTI_ALIAS_FLAG)
        /** Only for bitmaps and never recoloured: a colour with alpha on [paint] would make the pictures translucent. */
        private val bmp = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
        private val rectA = RectF()
        private val rectB = RectF()
        private val srcA = Rect()

        // car variant: photo, cut-out of the car (rows [CUT_TOP, CUT_TOP + height) of the photo) and the big clock
        private var carBg: Bitmap? = null
        private var carCut: Bitmap? = null
        private val clockPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = android.graphics.Typeface.create("sans-serif-condensed", android.graphics.Typeface.BOLD)
            textAlign = Paint.Align.LEFT
            textSize = 420f
        }
        private val timeBounds = Rect()
        private val textPath = Path()
        private val mergedPath = Path()
        private val pathBounds = RectF()
        private val pathMatrix = android.graphics.Matrix()
        private val clockFill = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
        private val clockStroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            color = Color.rgb(94, 234, 212)
            strokeJoin = Paint.Join.ROUND
        }
        private val timeFmt = java.text.SimpleDateFormat("h:mm", java.util.Locale.US)
        private var clockKey = ""
        private var clockFillBmp: Bitmap? = null
        private var clockEchoBmp: Bitmap? = null
        private val clockBox = RectF()     // where the two bitmaps go, in the photo's units, relative to [clockOriginX]
        private var clockOriginX = 0f
        private var refW = 0f
        private var refH = 0f

        // vinyl: turns while music plays, slows down when it pauses
        private var spin = 0f
        private var spinSpeed = 0f
        private var lastT = 0f
        private val sheen = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            shader = SweepGradient(
                0f, 0f,
                intArrayOf(0x00FFFFFF, 0x34FFFFFF, 0x00FFFFFF, 0x00FFFFFF, 0x34FFFFFF, 0x00FFFFFF, 0x00FFFFFF),
                floatArrayOf(0f, 0.08f, 0.16f, 0.5f, 0.58f, 0.66f, 1f),
            )
        }
        private val spinMark = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            shader = SweepGradient(
                0f, 0f,
                intArrayOf(0x00FFFFFF, 0x2CFFFFFF, 0x00FFFFFF, 0x00FFFFFF, 0x00FFFFFF, 0x1CFFFFFF, 0x00FFFFFF, 0x00FFFFFF),
                floatArrayOf(0f, 0.04f, 0.10f, 0.25f, 0.5f, 0.54f, 0.60f, 1f),
            )
        }

        // particles: soft motes drifting up, in the cover's colours
        private val nDots = 44
        private val rnd = java.util.Random(7)
        private val dx = FloatArray(nDots) { rnd.nextFloat() }
        private val dy = FloatArray(nDots) { rnd.nextFloat() }
        private val dr = FloatArray(nDots) { 2f + rnd.nextFloat() * 6f }
        private val dv = FloatArray(nDots) { 0.012f + rnd.nextFloat() * 0.03f }
        private val da = FloatArray(nDots) { 40f + rnd.nextFloat() * 100f }
        private val dp = FloatArray(nDots) { rnd.nextFloat() * 6.28f }
        private val dotColor = IntArray(nDots)

        // ---- frame loop -------------------------------------------------------------------------------------------

        override fun doFrame(frameTimeNanos: Long) {
            if (!visible) return
            choreographer.postFrameCallback(this)
            // 120 Hz panels would draw twice as often for nothing: keep ~60 fps
            // (the home screen is mostly looked past: 30 fps there saves battery)
            if (frameTimeNanos - lastFrameNs < (if (homeMode) 30_000_000L else 14_000_000L)) return
            lastFrameNs = frameTimeNanos
            val a = System.nanoTime()
            draw((frameTimeNanos / 1_000_000L - t0) / 1000f)
            val d = System.nanoTime() - a
            statFrames++; statDrawNs += d; if (d > statWorstNs) statWorstNs = d
            val now = frameTimeNanos / 1_000_000L
            if (statSince == 0L) statSince = now
            if (now - statSince >= 2000L) {
                Log.d("CoverFps", "fps=${"%.1f".format(statFrames * 1000f / (now - statSince))} " +
                    "avgDrawMs=${"%.2f".format(statDrawNs / 1e6f / statFrames)} worstMs=${"%.1f".format(statWorstNs / 1e6f)} " +
                    "car=$carMode ${w}x$h")
                statFrames = 0; statDrawNs = 0; statWorstNs = 0; statSince = now
            }
        }

        override fun onSurfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
            w = width; h = height
            vignette.shader = null
            rebuildLights()
            clockKey = ""
            draw((SystemClock.uptimeMillis() - t0) / 1000f)
        }

        override fun onVisibilityChanged(v: Boolean) {
            visible = v
            choreographer.removeFrameCallback(this)
            if (v) {
                startTilt()
                statSince = 0L; statFrames = 0; statDrawNs = 0; statWorstNs = 0
                choreographer.postFrameCallback(this)
            } else stopTilt()
        }

        override fun onSurfaceDestroyed(holder: SurfaceHolder) {
            visible = false
            stopTilt()
            choreographer.removeFrameCallback(this)
        }

        override fun onDestroy() {
            visible = false
            stopTilt()
            choreographer.removeFrameCallback(this)
            clockFillBmp?.recycle(); clockEchoBmp?.recycle()
            super.onDestroy()
        }

        // ---- what is playing --------------------------------------------------------------------------------------

        private fun rebuildLights() {
            if (w == 0) return
            val r = w * 0.75f
            // home: the hour's colour is mixed in, so the wallpaper changes through the day even without music
            val l1 = if (homeMode) ColorUtils.blendARGB(c1, hourTint(0), 0.45f) else c1
            val l2 = if (homeMode) ColorUtils.blendARGB(c2, hourTint(1), 0.45f) else c2
            blob1.shader = RadialGradient(0f, 0f, r, intArrayOf(withAlpha(l1, 150), withAlpha(l1, 0)), null, Shader.TileMode.CLAMP)
            blob2.shader = RadialGradient(0f, 0f, r, intArrayOf(withAlpha(l2, 130), withAlpha(l2, 0)), null, Shader.TileMode.CLAMP)
            val light = Color.WHITE
            for (i in 0 until nDots) dotColor[i] = ColorUtils.blendARGB(if (i % 2 == 0) c1 else accent, light, 0.35f)
        }

        private fun refresh() {
            val now = CoverBus.current()
            // paused for a while: back to the dark base, like the still wallpaper puts the old one back
            val idle = !now.playing && System.currentTimeMillis() - now.at > 90_000L
            if (now === lastNow && idle == lastIdle) return
            lastNow = now; lastIdle = idle
            val cover = if (idle) null else now.cover
            val id = if (idle) "idle" else now.key + "#" + CoverBus.signature(cover)
            if (id == shownKey) return
            shownKey = id
            if (cover == null || cover.isRecycled || cover.width < 32) {
                square = null; backdrop = null
                // nothing to show: the lights go back to Mochi's teal
                c1 = Color.rgb(20, 120, 110); c2 = Color.rgb(30, 90, 70)
                accent = Color.argb(150, 94, 234, 212)
                rebuildLights()
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
            rebuildLights()
        }

        // ---- drawing ----------------------------------------------------------------------------------------------

        private fun draw(t: Float) {
            val holder = surfaceHolder ?: return
            refresh()
            val c = try {
                holder.lockHardwareCanvas()
            } catch (_: Throwable) {
                try { holder.lockCanvas() } catch (_: Throwable) { null }
            } ?: return
            try {
                paint2(c, t)
            } finally {
                holder.unlockCanvasAndPost(c)
            }
        }

        // ---- home variant -----------------------------------------------------------------------------------------

        private var pageX = 0.5f                // home page scroll, 0..1 (the launcher tells us)
        private var hourKey = -1L
        private val vignette = Paint()

        override fun onOffsetsChanged(xOffset: Float, yOffset: Float, xStep: Float, yStep: Float, xPixels: Int, yPixels: Int) {
            pageX = xOffset
        }

        /** Two colours per hour of the day: dawn teal-blue, noon teal, evening violet-teal, deep blue at night. */
        private fun hourTint(which: Int): Int {
            val cal = java.util.Calendar.getInstance()
            val hr = cal.get(java.util.Calendar.HOUR_OF_DAY) + cal.get(java.util.Calendar.MINUTE) / 60f
            val keys = floatArrayOf(0f, 5f, 8f, 13f, 18f, 21f, 24f)
            val a = arrayOf(
                intArrayOf(14, 40, 78), intArrayOf(40, 120, 150), intArrayOf(40, 170, 160), intArrayOf(30, 170, 140),
                intArrayOf(110, 80, 170), intArrayOf(30, 60, 110), intArrayOf(14, 40, 78),
            )
            val b = arrayOf(
                intArrayOf(10, 30, 60), intArrayOf(90, 120, 170), intArrayOf(60, 190, 170), intArrayOf(20, 130, 120),
                intArrayOf(60, 130, 160), intArrayOf(20, 50, 90), intArrayOf(10, 30, 60),
            )
            val set = if (which == 0) a else b
            var i = 0
            while (i < keys.size - 2 && hr >= keys[i + 1]) i++
            val f = ((hr - keys[i]) / (keys[i + 1] - keys[i])).coerceIn(0f, 1f)
            return Color.rgb(
                (set[i][0] + (set[i + 1][0] - set[i][0]) * f).toInt(),
                (set[i][1] + (set[i + 1][1] - set[i][1]) * f).toInt(),
                (set[i][2] + (set[i + 1][2] - set[i][2]) * f).toInt(),
            )
        }

        /** Dark base, two slow lights that follow the music's colours and the hour, motes, and a little depth from tilt and page. */
        private fun drawHome(c: Canvas, t: Float) {
            val minute = System.currentTimeMillis() / 60_000L
            if (minute != hourKey) { hourKey = minute; rebuildLights() }
            val dt = (t - lastT).coerceIn(0f, 0.1f)
            lastT = t
            val playing = CoverBus.current().playing
            val beat = if (playing) 1f + 0.06f * sin(t * 1.6f) else 1f     // the lights breathe a little with music
            val px = tiltX * w * 0.045f + (0.5f - pageX) * w * 0.12f
            val py = tiltY * w * 0.035f
            blob1.alpha = 150; blob2.alpha = 135
            c.save()
            c.translate(w * (0.30f + 0.22f * sin(t / 9f)) + px, h * (0.26f + 0.12f * cos(t / 11f)) + py)
            c.scale(beat, beat)
            c.drawCircle(0f, 0f, w * 0.75f, blob1)
            c.restore()
            c.save()
            c.translate(w * (0.72f + 0.20f * cos(t / 10f)) + px * 1.4f, h * (0.74f + 0.10f * sin(t / 12f)) + py * 1.4f)
            c.scale(beat, beat)
            c.drawCircle(0f, 0f, w * 0.75f, blob2)
            c.restore()
            for (i in 0 until nDots) {
                val y = (((dy[i] - t * dv[i] * 0.7f) % 1f) + 1f) % 1f * h + py * (0.4f + dr[i] / 8f)
                val x = (dx[i] + 0.03f * sin(t * 0.3f + dp[i])) * w + px * (0.4f + dr[i] / 8f)
                dot.color = dotColor[i]
                dot.alpha = (da[i] * 0.6f * (0.6f + 0.4f * sin(t * 1.1f + dp[i]))).toInt().coerceIn(0, 255)
                c.drawCircle(x, y, dr[i], dot)
            }
            // darker toward the bottom, where the dock and the icons are
            if (vignette.shader == null) {
                vignette.shader = LinearGradient(
                    0f, h * 0.45f, 0f, h.toFloat(),
                    Color.argb(0, 4, 12, 11), Color.argb(150, 4, 12, 11), Shader.TileMode.CLAMP,
                )
            }
            c.drawRect(0f, h * 0.45f, w.toFloat(), h.toFloat(), vignette)
        }

        private fun paint2(c: Canvas, t: Float) {
            if (w == 0 || h == 0) return
            c.drawColor(Color.rgb(7, 19, 17))
            if (homeMode) { drawHome(c, t); return }
            if (carMode) drawCarBehind(c, t)

            // blurred copy drifting slowly (Ken Burns)
            if (!carMode) backdrop?.let { b ->
                val k = maxOf(w.toFloat() / b.width, h.toFloat() / b.height) * (1.18f + 0.05f * sin(t / 9f))
                c.save()
                c.translate(w / 2f + 40f * sin(t / 13f), h / 2f + 40f * cos(t / 11f))
                c.scale(k, k)
                c.drawBitmap(b, -b.width / 2f, -b.height / 2f, bmp)
                c.restore()
            }
            if (!carMode && backdrop != null) c.drawColor(Color.argb(168, 7, 19, 17))

            // two soft lights in the cover's colours
            blob1.alpha = if (carMode) 70 else 255
            blob2.alpha = if (carMode) 70 else 255
            c.save()
            c.translate(w * (0.30f + 0.22f * sin(t / 7f)), h * (0.28f + 0.12f * cos(t / 9f)))
            c.drawCircle(0f, 0f, w * 0.75f, blob1)
            c.restore()
            c.save()
            c.translate(w * (0.72f + 0.20f * cos(t / 8f)), h * (0.78f + 0.10f * sin(t / 10f)))
            c.drawCircle(0f, 0f, w * 0.75f, blob2)
            c.restore()

            // spin of the record: speeds up with the music, coasts to a stop when it pauses
            val dt = (t - lastT).coerceIn(0f, 0.1f)
            lastT = t
            val target = if (CoverBus.current().playing) 70f else 0f
            spinSpeed += (target - spinSpeed) * minOf(1f, dt * 1.5f)
            spin = (spin + spinSpeed * dt) % 360f

            // lyrics: when there are some, the cover and record slide a little to the left to leave a column for them
            val ly = if (carMode) Lyrics.lines else emptyList()
            lyricMix += ((if (ly.isNotEmpty()) 1f else 0f) - lyricMix) * minOf(1f, dt * 3f)

            // particles, under everything that is solid
            for (i in 0 until nDots) {
                val y = (((dy[i] - t * dv[i]) % 1f) + 1f) % 1f * h
                val x = (dx[i] + 0.03f * sin(t * 0.4f + dp[i])) * w
                val a = da[i] * (0.6f + 0.4f * sin(t * 1.3f + dp[i]))
                dot.color = dotColor[i]
                dot.alpha = a.toInt().coerceIn(0, 255)
                c.drawCircle(x, y, dr[i], dot)
            }

            // the cover itself, breathing a little
            val sq = square ?: return
            val side = w * (if (carMode) 0.38f else 0.64f) * (1f + 0.012f * sin(t / 3f))
            val cx = w * (if (carMode) 0.36f - 0.085f * lyricMix else 0.40f) +(if (carMode) tiltX * w * 0.050f else 0f)
            val cy = h * (if (carMode) 0.735f else 0.60f) + 6f * sin(t / 4f) + (if (carMode) tiltY * w * 0.040f else 0f)
            rectA.set(cx - side / 2, cy - side / 2, cx + side / 2, cy + side / 2)

            // cover and record turn together: in the car variant they lean like the car in the photo (it points up and to
            // the left), and follow the tilt a little, so the group sits in the composition and not square to the screen
            c.save()
            c.rotate(if (carMode) -8f + 1.5f * sin(t / 6f) + tiltX * 3f else 0f, cx, cy)

            // the record sliding out from behind the cover
            val vr = side * 0.485f
            val vx = cx + side * 0.34f
            c.save()
            c.translate(vx, cy)
            dot.color = Color.argb(90, 0, 0, 0)
            c.drawCircle(side * 0.02f, side * 0.02f, vr, dot)
            dot.color = Color.rgb(11, 11, 12)
            c.drawCircle(0f, 0f, vr, dot)
            paint.style = Paint.Style.STROKE
            paint.strokeWidth = 1f
            paint.color = Color.argb(20, 255, 255, 255)
            for (k in 0..9) c.drawCircle(0f, 0f, vr * (0.40f + 0.058f * k), paint)
            c.drawCircle(0f, 0f, vr * 0.99f, sheen)
            // a faint wedge that turns with the record, so the spin shows even with the label behind the cover
            c.save()
            c.rotate(spin)
            c.drawCircle(0f, 0f, vr * 0.99f, spinMark)
            c.restore()
            val rl = vr * 0.34f
            c.save()
            c.rotate(spin)
            clip.reset(); clip.addCircle(0f, 0f, rl, Path.Direction.CW)
            c.clipPath(clip)
            val q = (sq.width * 0.30f).toInt()
            srcA.set(sq.width / 2 - q, sq.height / 2 - q, sq.width / 2 + q, sq.height / 2 + q)
            rectB.set(-rl, -rl, rl, rl)
            c.drawBitmap(sq, srcA, rectB, bmp)
            c.restore()
            paint.color = accent
            paint.strokeWidth = 2f
            c.drawCircle(0f, 0f, rl, paint)
            paint.style = Paint.Style.FILL
            dot.color = Color.rgb(11, 11, 12)
            c.drawCircle(0f, 0f, vr * 0.035f, dot)
            c.restore()
            val r = side * 0.07f
            c.save()
            c.rotate(1.0f * sin(t / 7f), cx, cy)
            paint.style = Paint.Style.FILL
            paint.color = Color.argb(90, 0, 0, 0)
            c.drawRoundRect(rectA.left, rectA.top + side * 0.025f, rectA.right, rectA.bottom + side * 0.025f, r, r, paint)
            clip.reset(); clip.addRoundRect(rectA, r, r, Path.Direction.CW)
            c.save(); c.clipPath(clip)
            srcA.set(0, 0, sq.width, sq.height)
            c.drawBitmap(sq, srcA, rectA, bmp)
            c.restore()
            paint.style = Paint.Style.STROKE
            paint.strokeWidth = 2f
            paint.color = accent
            c.drawRoundRect(rectA, r, r, paint)
            paint.style = Paint.Style.FILL
            c.restore()
            if (ly.isNotEmpty()) drawLyrics(c, ly, t, cx + side * 0.825f + w * 0.025f, cy)
            c.restore() // the lean of the whole group
        }

        // ---- lyrics beside the record ---------------------------------------------------------------------------

        private val lyricPaint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
            setShadowLayer(10f, 0f, 2f, Color.argb(170, 0, 0, 0))
        }
        private var lyricMix = 0f
        private var lyricFor: List<Lyrics.Line>? = null
        private var lyricIdx = -2
        private var lyricW = 0
        private var lyricAt = 0f
        private val lyricLayouts = arrayOfNulls<StaticLayout>(3)   // previous, current, next

        private fun layoutOf(text: String, size: Float, width: Int): StaticLayout {
            lyricPaint.textSize = size
            return StaticLayout.Builder.obtain(text, 0, text.length, lyricPaint, width)
                .setMaxLines(4).setEllipsize(android.text.TextUtils.TruncateAt.END).build()
        }

        /** The line being sung (bright), the one before and the one after (dim), in a column from [x0] to the right edge. */
        private fun drawLyrics(c: Canvas, ly: List<Lyrics.Line>, t: Float, x0: Float, cy: Float) {
            val width = (w * 0.965f - x0).toInt()
            if (width < w * 0.18f) return
            val idx = Lyrics.indexAt(ly, Lyrics.positionMs())
            if (ly !== lyricFor || idx != lyricIdx || width != lyricW) {
                if (ly !== lyricFor || idx != lyricIdx) lyricAt = t
                lyricFor = ly; lyricIdx = idx; lyricW = width
                fun text(i: Int) = ly.getOrNull(i)?.text.orEmpty()
                val big = w * 0.050f
                val small = w * 0.034f
                lyricLayouts[0] = text(idx - 1).takeIf { it.isNotBlank() }?.let { layoutOf(it, small, width) }
                lyricLayouts[1] = if (idx >= 0) layoutOf(text(idx).ifBlank { "♪" }, big, width) else null
                lyricLayouts[2] = text(idx + 1).takeIf { it.isNotBlank() }?.let { layoutOf(it, small, width) }
            }
            val e = ((t - lyricAt) / 0.35f).coerceIn(0f, 1f)
            val ease = 1f - (1f - e) * (1f - e)
            val gap = w * 0.022f
            val cur = lyricLayouts[1]
            val curH = cur?.height?.toFloat() ?: 0f
            val a = (255 * lyricMix).toInt()
            c.save()
            c.translate(x0, cy - curH / 2f + (1f - ease) * w * 0.03f)
            lyricPaint.color = Color.WHITE
            lyricPaint.alpha = (a * ease).toInt()
            cur?.draw(c)
            lyricPaint.color = Color.WHITE
            lyricLayouts[0]?.let { p ->
                c.save(); c.translate(0f, -gap - p.height); lyricPaint.alpha = (a * 0.38f * ease).toInt(); p.draw(c); c.restore()
            }
            lyricLayouts[2]?.let { n ->
                c.save(); c.translate(0f, curH + gap); lyricPaint.alpha = (a * 0.5f).toInt(); n.draw(c); c.restore()
            }
            c.restore()
        }

        // ---- car variant ------------------------------------------------------------------------------------------

        private fun loadCar() {
            if (carBg != null) return
            // The photo and its cut-out are the user's own picture (copyrighted, so not part of the app): they are read from
            // Android/data/<app>/files/car/car_bg.jpg and car_cut.png. Without them only the dark base and the clock show.
            runCatching {
                val dir = getExternalFilesDir("car") ?: return@runCatching
                carBg = android.graphics.BitmapFactory.decodeFile(java.io.File(dir, "car_bg.jpg").path)
                carCut = android.graphics.BitmapFactory.decodeFile(java.io.File(dir, "car_cut.png").path)
            }.onFailure { Log.w("CoverLive", "car layers missing", it) }
        }

        /**
         * The clock, rendered into two bitmaps (fill + outline, and the "echo" outline) once a minute. The digits are a
         * Path so the stroke keeps one width however much they are stretched; the font's glyphs overlap, so their
         * contours are merged first (otherwise the inner joins of "4" and "9" show). 12-hour "h:mm", always laid out as
         * if it were "00:00" wide so 9:52 and 10:52 have the same digit size.
         */
        private fun ensureClock(k: Float, photoW: Float) {
            val key = "${System.currentTimeMillis() / 60_000L}|$w|$h"
            if (key == clockKey) return
            clockKey = key
            val time = timeFmt.format(java.util.Date())
            if (refW == 0f) {
                clockPaint.getTextBounds("00:00", 0, 5, timeBounds)
                refW = timeBounds.width().toFloat(); refH = timeBounds.height().toFloat()
            }
            val visibleW = (w - 2f * w * 0.075f) / k   // leaves room for the parallax shift without touching the edges
            val fit = visibleW / refW
            val tall = 550f / (refH * fit)             // tall and narrow, to fill the free band above the car
            textPath.reset()
            clockPaint.getTextPath(time, 0, time.length, 0f, 0f, textPath)
            textPath.transform(pathMatrix.apply { setScale(fit, fit * tall) })
            mergedPath.reset()
            if (!mergedPath.op(textPath, textPath, Path.Op.UNION)) { mergedPath.reset(); mergedPath.addPath(textPath) }
            mergedPath.computeBounds(pathBounds, true)
            val pad = 14f
            clockBox.set(pathBounds.left - pad, pathBounds.top - pad, pathBounds.right + pad, pathBounds.bottom + pad)
            clockOriginX = photoW / 2f - (pathBounds.left + pathBounds.right) / 2f
            val bw = kotlin.math.ceil(clockBox.width() * k).toInt().coerceAtLeast(1)
            val bh = kotlin.math.ceil(clockBox.height() * k).toInt().coerceAtLeast(1)
            clockFillBmp?.recycle(); clockEchoBmp?.recycle()
            val fillBmp = Bitmap.createBitmap(bw, bh, Bitmap.Config.ARGB_8888)
            val echoBmp = Bitmap.createBitmap(bw, bh, Bitmap.Config.ARGB_8888)
            clockFill.shader = LinearGradient(
                0f, pathBounds.top, 0f, 0f,
                intArrayOf(Color.argb(205, 236, 246, 243), Color.argb(150, 190, 235, 228), Color.argb(14, 94, 234, 212)),
                floatArrayOf(0f, 0.55f, 1f), Shader.TileMode.CLAMP,
            )
            Canvas(fillBmp).apply {
                scale(k, k); translate(-clockBox.left, -clockBox.top)
                drawPath(mergedPath, clockFill)
                clockStroke.alpha = 235; clockStroke.strokeWidth = 3.2f
                drawPath(mergedPath, clockStroke)
            }
            Canvas(echoBmp).apply {
                scale(k, k); translate(-clockBox.left, -clockBox.top)
                clockStroke.alpha = 85; clockStroke.strokeWidth = 2.2f
                drawPath(mergedPath, clockStroke)
            }
            clockFillBmp = fillBmp; clockEchoBmp = echoBmp
        }

        /**
         * Photo, then the big clock, then the car in front of it, each layer shifted by the tilt by a different amount
         * (the farther, the less), so the phone behaves like a window onto a small 3D scene. The photo is fitted to the
         * width (the car must not be cropped), anchored to the bottom, and the dark band above it is the photo's own top row.
         */
        private fun drawCarBehind(c: Canvas, t: Float) {
            loadCar()
            val bg = carBg ?: return
            val k = w * 1.06f / bg.width
            val ox = (w - bg.width * k) / 2f
            val oy = h - bg.height * k
            val sway = if (tiltLive) 0f else 1f   // no sensor: a very slow idle sway keeps the depth readable
            val tx = tiltX + sway * 0.35f * sin(t / 5f)
            val ty = tiltY + sway * 0.20f * cos(t / 6f)

            c.save()
            c.translate(ox + tx * w * 0.012f, oy + ty * w * 0.010f)
            c.scale(k, k)
            if (oy > 0f) {
                srcA.set(0, 0, bg.width, 2)
                rectB.set(-w.toFloat(), -oy / k - 2f, bg.width + w.toFloat(), 2f)
                c.drawBitmap(bg, srcA, rectB, bmp)
            }
            c.drawBitmap(bg, 0f, 0f, bmp)
            c.restore()

            ensureClock(k, bg.width.toFloat())
            val fillBmp = clockFillBmp
            val echoBmp = clockEchoBmp
            if (fillBmp != null && echoBmp != null) {
                c.save()
                c.translate(ox + tx * w * 0.022f, oy + ty * w * 0.018f)
                c.scale(k, k)
                c.translate(clockOriginX, CLOCK_BASE_Y)
                c.save()
                c.translate(-tx * 20f, -ty * 14f + 10f)            // the echo, a little behind and apart
                c.drawBitmap(echoBmp, null, clockBox, bmp)
                c.restore()
                c.drawBitmap(fillBmp, null, clockBox, bmp)
                c.restore()
            }

            c.save()
            c.translate(ox + tx * w * 0.036f, oy + ty * w * 0.030f)
            c.scale(k, k)
            carCut?.let { c.drawBitmap(it, 0f, CUT_TOP.toFloat(), bmp) }
            c.restore()
        }

        // ---- tilt (gravity sensor: no permission), smoothed, relative to how the phone is usually held --------------

        private var tiltX = 0f
        private var tiltY = 0f
        private var tiltLive = false
        private var fx = 0f
        private var fy = 0f
        private var refX = 0f
        private var refY = 5f
        private var haveRef = false
        private val sensors by lazy { getSystemService(SENSOR_SERVICE) as? android.hardware.SensorManager }
        private val tiltListener = object : android.hardware.SensorEventListener {
            override fun onSensorChanged(e: android.hardware.SensorEvent) {
                val gx = e.values[0]
                val gy = e.values[1]
                if (!haveRef) { refX = gx; refY = gy; fx = gx; fy = gy; haveRef = true }
                fx += (gx - fx) * 0.10f
                fy += (gy - fy) * 0.10f
                refX += (fx - refX) * 0.0015f     // the way the phone is held becomes the neutral position, slowly
                refY += (fy - refY) * 0.0015f
                tiltX = ((fx - refX) / 4.5f).coerceIn(-1f, 1f) * -1f
                tiltY = ((fy - refY) / 4.5f).coerceIn(-1f, 1f)
                tiltLive = true
            }
            override fun onAccuracyChanged(s: android.hardware.Sensor?, a: Int) {}
        }

        private fun startTilt() {
            if (!carMode && !homeMode) return
            val m = sensors ?: return
            val s = m.getDefaultSensor(android.hardware.Sensor.TYPE_GRAVITY) ?: m.getDefaultSensor(android.hardware.Sensor.TYPE_ACCELEROMETER) ?: return
            haveRef = false
            m.registerListener(tiltListener, s, android.hardware.SensorManager.SENSOR_DELAY_GAME)
        }

        private fun stopTilt() {
            runCatching { sensors?.unregisterListener(tiltListener) }
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

    private companion object {
        const val CUT_TOP = 600
        const val CLOCK_BASE_Y = 730f
    }
}

/** For the home screen: slow lights and motes in the colours of the music and the hour, dim enough for the icons. */
class HomeAmbientWallpaperService : CoverLiveWallpaperService() {
    override val homeMode: Boolean get() = true
}

/** The same animated wallpaper, with the photo of the car and the clock behind it. */
class CarClockWallpaperService : CoverLiveWallpaperService() {
    override val carMode: Boolean get() = true
}
