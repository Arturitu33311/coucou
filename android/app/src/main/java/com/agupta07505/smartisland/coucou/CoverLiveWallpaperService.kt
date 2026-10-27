/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.LinearGradient
import android.graphics.RadialGradient
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.SweepGradient
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.service.wallpaper.WallpaperService
import android.util.Log
import android.view.SurfaceHolder
import kotlin.math.cos
import kotlin.math.sin

/**
 * PROTOTYPE of the animated cover wallpaper: the cover of what is playing, sharp and breathing, over a blurred copy that
 * drifts slowly, with two soft lights in the cover's own colours moving behind it. Draws only while it is visible, ~30 fps,
 * and does nothing else: no network; the car variant reads the gravity sensor (no permission) only while visible. With nothing playing it shows the dark teal base with the same lights.
 */
open class CoverLiveWallpaperService : WallpaperService() {
    /** The "car" variant: a photo with the clock BEHIND the car (cut-out in front), the music effects on the floor. */
    protected open val carMode: Boolean = false

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
        private val dot = Paint(Paint.ANTI_ALIAS_FLAG)

        // car variant: photo, cut-out of the car (rows [CUT_TOP, CUT_TOP + height) of the photo) and the big clock
        private var carBg: Bitmap? = null
        private var carCut: Bitmap? = null
        private val clockPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = android.graphics.Typeface.create("sans-serif-condensed", android.graphics.Typeface.BOLD)
            color = Color.argb(150, 232, 240, 238)
            textAlign = Paint.Align.LEFT
        }
        private val datePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = android.graphics.Typeface.create("sans-serif-condensed", android.graphics.Typeface.NORMAL)
            color = Color.argb(190, 232, 240, 238)
            textAlign = Paint.Align.CENTER
        }
        private val timeBounds = Rect()
        private val textPath = Path()
        private val mergedPath = Path()
        private var merged = ""
        private val pathMatrix = android.graphics.Matrix()
        private val clockFill = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }
        private val clockStroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            color = Color.rgb(94, 234, 212)
            strokeJoin = Paint.Join.ROUND
        }
        /** Only for bitmaps and never recoloured: a colour with alpha on [paint] would make the pictures translucent. */
        private val bmp = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)

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
            if (v) { startTilt(); main.post(tick) } else stopTilt()
        }

        override fun onSurfaceDestroyed(holder: SurfaceHolder) {
            visible = false
            stopTilt()
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
            if (carMode) drawCarBehind(c)

            // blurred copy drifting slowly (Ken Burns)
            if (!carMode) backdrop?.let { b ->
                val k = maxOf(w.toFloat() / b.width, h.toFloat() / b.height) * (1.18f + 0.05f * sin(t / 9f))
                c.save()
                c.translate(w / 2f + 40f * sin(t / 13f), h / 2f + 40f * cos(t / 11f))
                c.scale(k, k)
                c.drawBitmap(b, -b.width / 2f, -b.height / 2f, bmp)
                c.restore()
            }
            if (!carMode) c.drawColor(Color.argb(if (backdrop != null) 168 else 0, 7, 19, 17))

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

            // particles, under everything that is solid
            val light = Color.argb(255, 255, 255, 255)
            for (i in 0 until nDots) {
                val y = (((dy[i] - t * dv[i]) % 1f) + 1f) % 1f * h
                val x = (dx[i] + 0.03f * sin(t * 0.4f + dp[i])) * w
                val a = da[i] * (0.6f + 0.4f * sin(t * 1.3f + dp[i]))
                val base = if (i % 2 == 0) c1 else accent
                dot.color = androidx.core.graphics.ColorUtils.blendARGB(base, light, 0.35f)
                dot.alpha = a.toInt().coerceIn(0, 255)
                c.drawCircle(x, y, dr[i], dot)
            }

            // the cover itself, breathing a little
            val sq = square ?: return
            val side = w * (if (carMode) 0.38f else 0.64f) * (1f + 0.012f * sin(t / 3f))
            val cx = w * (if (carMode) 0.36f else 0.40f) + (if (carMode) tiltX * w * 0.050f else 0f)
            val cy = h * (if (carMode) 0.735f else 0.60f) + 6f * sin(t / 4f) + (if (carMode) tiltY * w * 0.040f else 0f)
            val box = RectF(cx - side / 2, cy - side / 2, cx + side / 2, cy + side / 2)

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
            val q = sq.width * 0.30f
            c.drawBitmap(sq, Rect((sq.width / 2 - q).toInt(), (sq.height / 2 - q).toInt(), (sq.width / 2 + q).toInt(), (sq.height / 2 + q).toInt()), RectF(-rl, -rl, rl, rl), bmp)
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
            c.drawRoundRect(RectF(box.left, box.top + side * 0.025f, box.right, box.bottom + side * 0.025f), r, r, paint)
            clip.reset(); clip.addRoundRect(box, r, r, Path.Direction.CW)
            c.save(); c.clipPath(clip)
            c.drawBitmap(sq, Rect(0, 0, sq.width, sq.height), box, bmp)
            c.restore()
            paint.style = Paint.Style.STROKE
            paint.strokeWidth = 2f
            paint.color = accent
            c.drawRoundRect(box, r, r, paint)
            paint.style = Paint.Style.FILL
            c.restore()
            c.restore() // the lean of the whole group
        }

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
         * Photo, then the big clock, then the car in front of it, each layer shifted by the tilt by a different amount
         * (the farther, the less), so the phone behaves like a window onto a small 3D scene. The photo is fitted to the
         * width (the car must not be cropped), anchored to the bottom, and the dark band above it is the photo's own top row.
         */
        private fun drawCarBehind(c: Canvas) {
            loadCar()
            val bg = carBg ?: return
            val k = w * 1.06f / bg.width
            val ox = (w - bg.width * k) / 2f
            val oy = h - bg.height * k
            val sway = if (tiltLive) 0f else 1f   // no sensor: a very slow idle sway keeps the depth readable
            val t = (SystemClock.uptimeMillis() - t0) / 1000f
            val tx = tiltX + sway * 0.35f * sin(t / 5f)
            val ty = tiltY + sway * 0.20f * cos(t / 6f)

            c.save()
            c.translate(ox + tx * w * 0.012f, oy + ty * w * 0.010f)
            c.scale(k, k)
            if (oy > 0f) c.drawBitmap(bg, Rect(0, 0, bg.width, 2), RectF(-w.toFloat(), -oy / k - 2f, bg.width + w.toFloat(), 2f), bmp)
            c.drawBitmap(bg, 0f, 0f, bmp)
            c.restore()

            // the clock, in the photo's coordinates: wide, tall, translucent, its lower part hidden by the car
            val now = java.util.Date()
            val time = java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault()).format(now)
            clockPaint.textSize = 420f
            clockPaint.getTextBounds(time, 0, time.length, timeBounds)
            val visibleW = (w - 2f * w * 0.075f) / k   // leaves room for the parallax shift without touching the edges
            val fit = visibleW / timeBounds.width()
            // The digits are a Path (not text) so the stroke keeps one width however much they are stretched:
            // a hairline teal outline, a fill that dissolves downwards into the floor, and an "echo" outline behind
            // them that slides apart from the front one with the tilt, which is what makes the clock itself look deep.
            val tall = 550f / (timeBounds.height() * fit)
            // The font's glyphs are built from overlapping strokes; stroking them as they are draws the inner joins
            // ("4", "9"). Merging the contours into one silhouette removes them. Rebuilt only when the minute or size changes.
            val pathKey = "$time|$w|$h"
            if (pathKey != merged) {
                textPath.reset()
                clockPaint.getTextPath(time, 0, time.length, 0f, 0f, textPath)
                textPath.transform(pathMatrix.apply { setScale(fit, fit * tall) })
                mergedPath.reset()
                if (!mergedPath.op(textPath, textPath, Path.Op.UNION)) { mergedPath.reset(); mergedPath.addPath(textPath) }
                merged = pathKey
            }
            val half = timeBounds.width() * fit / 2f - timeBounds.left * fit
            val baseY = 730f
            c.save()
            c.translate(ox + tx * w * 0.022f, oy + ty * w * 0.018f)
            c.scale(k, k)
            c.translate(bg.width / 2f - half, baseY)
            val top = -550f
            clockFill.shader = LinearGradient(
                0f, top, 0f, 0f,
                intArrayOf(Color.argb(205, 236, 246, 243), Color.argb(150, 190, 235, 228), Color.argb(14, 94, 234, 212)),
                floatArrayOf(0f, 0.55f, 1f), Shader.TileMode.CLAMP,
            )
            c.save()
            c.translate(-tx * 20f, -ty * 14f + 10f)            // the echo, drawn first, a little behind and apart
            clockStroke.alpha = 85
            clockStroke.strokeWidth = 2.2f
            c.drawPath(mergedPath, clockStroke)
            c.restore()
            c.drawPath(mergedPath, clockFill)
            clockStroke.alpha = 235
            clockStroke.strokeWidth = 3.2f
            c.drawPath(mergedPath, clockStroke)
            c.restore()

            c.save()
            c.translate(ox + tx * w * 0.036f, oy + ty * w * 0.030f)
            c.scale(k, k)
            carCut?.let { c.drawBitmap(it, 0f, CUT_TOP.toFloat(), bmp) }
            c.restore()
        }

        // --- tilt (gravity sensor: no permission), smoothed, relative to how the phone is usually held ---
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
            if (!carMode) return
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

    private companion object { const val CUT_TOP = 600 }
}

/** The same animated wallpaper, with the photo of the car and the clock behind it. */
class CarClockWallpaperService : CoverLiveWallpaperService() {
    override val carMode: Boolean get() = true
}
