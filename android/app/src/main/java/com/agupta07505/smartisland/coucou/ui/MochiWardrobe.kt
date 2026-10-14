/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * Mochi's wardrobe in 3-D: a port of windows/src/mochi/wardrobe.ts (itself the port of the Mac's
 * MochiOutfitDrawing.swift) to Compose. The head is a superellipse "ball" (exponent 2.7) and every
 * accessory is built from points on its surface, projected with his yaw and pitch, so a hat turns
 * with his head, a rim follows the skull and glasses sit on his eyes. Two passes, as on the Mac:
 * [drawOutfitBehind] before the body (the back of a crown or a brim, bunny ears) and
 * [drawOutfitFront] after his eyes.
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import java.time.LocalDate
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sin

/** The Mac's wardrobe (same ids), then the two the PC and the phone add. */
enum class Outfit(val id: String, val label: String) {
    None("none", "None"),
    PartyHat("partyHat", "Party hat"),
    Beanie("beanie", "Beanie"),
    Crown("crown", "Crown"),
    Sunglasses("sunglasses", "Sunglasses"),
    RoundGlasses("roundGlasses", "Round glasses"),
    Bow("bow", "Bow"),
    Scarf("scarf", "Scarf"),
    WitchHat("witchHat", "Witch hat"),
    Pumpkin("pumpkin", "Pumpkin"),
    SantaHat("santaHat", "Santa hat"),
    BunnyEars("bunnyEars", "Bunny ears"),
    Headphones("headphones", "Headphones"),
    Nightcap("nightcap", "Night cap");

    companion object {
        const val AUTO = "auto"
        fun of(id: String?): Outfit? = entries.firstOrNull { it.id == id }
    }
}

/** Which outfit he wears: the user's choice, or "auto" = the seasons (the Mac's rule), then what he is doing. */
object Wardrobe {
    /** The season's outfit for a local date — MochiWardrobe.swift's `seasonal`, rule for rule. */
    fun seasonal(date: LocalDate): Outfit {
        val month = date.monthValue
        val day = date.dayOfMonth
        if ((month == 12 && day == 31) || (month == 1 && day <= 2)) return Outfit.PartyHat
        if (month == 12 && day <= 26) return Outfit.SantaHat
        if (month == 10 || (month == 11 && day == 1)) return Outfit.WitchHat
        val easter = easter(date.year)
        val days = java.time.temporal.ChronoUnit.DAYS.between(easter, date)
        if (days in -2..1) return Outfit.BunnyEars
        if ((month == 6 && day >= 21) || month == 7 || month == 8) return Outfit.Sunglasses
        return Outfit.None
    }

    /** Meeus/Jones/Butcher. */
    fun easter(year: Int): LocalDate {
        val a = year % 19
        val b = year / 100
        val c = year % 100
        val d = b / 4
        val e = b % 4
        val f = (b + 8) / 25
        val g = (b - f + 1) / 3
        val h = (19 * a + b - d - g + 15) % 30
        val i = c / 4
        val k = c % 4
        val l = (32 + 2 * e + 2 * i - h - k) % 7
        val m = (a + 11 * h + 22 * l) / 451
        val month = (h + l - 7 * m + 114) / 31
        val day = (h + l - 7 * m + 114) % 31 + 1
        return LocalDate.of(year, month, day)
    }

    /** [setting] is an [Outfit.id], "none" or "auto" (or anything unknown, read as auto). */
    fun pick(setting: String?, mood: MochiMood, date: LocalDate = LocalDate.now()): Outfit {
        if (setting == "none") return Outfit.None
        Outfit.of(setting)?.let { return it }
        // auto: asleep he wears a night cap, the season's outfit otherwise, and music puts headphones on.
        if (mood == MochiMood.Sleepy) return Outfit.Nightcap
        val season = seasonal(date)
        if (season != Outfit.None) return season
        if (mood == MochiMood.Dancing) return Outfit.Headphones
        return Outfit.None
    }

    /**
     * How much the whole Mochi (body and outfit) is shrunk so the outfit stays inside the box he is
     * drawn in: a witch hat's brim is wider than he is, a party hat is taller than a body.
     */
    fun fit(o: Outfit): Float = when (o) {
        Outfit.WitchHat -> 0.66f
        Outfit.PartyHat -> 0.74f
        Outfit.BunnyEars -> 0.78f
        Outfit.SantaHat, Outfit.Nightcap -> 0.80f
        Outfit.Crown, Outfit.Beanie -> 0.84f
        Outfit.Headphones -> 0.88f
        Outfit.Pumpkin -> 0.88f
        Outfit.Bow, Outfit.Scarf -> 0.9f
        else -> 1f
    }
}

// ── Geometry ─────────────────────────────────────────────────────────────────

private const val EXP = 2.7f
private const val VIEW_TILT = -0.30f
private const val ACC_PITCH = 0.4f
private const val EYE_SP = 0.37f
private const val EYE_P = -0.12f

/** Head geometry and physics (MochiH). */
class Head(
    val R: Float,
    val yaw: Float = 0f,
    val pitch: Float = 0f,
    val physDx: Float = 0f,
    val physDy: Float = 0f,
    /** A small Mochi (under ~40 dp): fewer details, a stubbier party hat. Decided by whoever knows the dp. */
    val simplified: Boolean = false,
) {
    val rx = R * 1.14f
    val ry = R * 0.88f
}

private class P3(val x: Float, val y: Float, val z: Float)

private fun ringR(y: Float): Float {
    val a = min(1f, abs(y))
    return (1f - a.pow(EXP)).pow(1f / EXP)
}

/** Rotates a head-local point (x right, y up, z toward the viewer) by yaw then pitch, and projects it. */
private fun proj(h: Head, x: Float, y: Float, z: Float): P3 {
    val pitch = VIEW_TILT + h.pitch * ACC_PITCH
    val cy = cos(h.yaw)
    val sy = sin(h.yaw)
    val x1 = x * cy + z * sy
    val z1 = -x * sy + z * cy
    val cp = cos(pitch)
    val sp = sin(pitch)
    val y2 = y * cp + z1 * sp
    val z2 = -y * sp + z1 * cp
    return P3(x1 * h.rx, -y2 * h.ry, z2)
}

private fun surf(h: Head, y: Float, lon: Float, s: Float = 1f): P3 {
    val r = ringR(y) * s
    return proj(h, r * sin(lon), y, r * cos(lon))
}

private fun frontSilhouetteArc(pts: List<P3>): List<P3> {
    val n = pts.size
    if (n <= 1) return pts
    var minI = 0
    var maxI = 0
    for (i in 1 until n) {
        if (pts[i].x < pts[minI].x) minI = i
        if (pts[i].x > pts[maxI].x) maxI = i
    }
    if (minI == maxI) return listOf(pts[minI])
    val a = ArrayList<P3>()
    var i = minI
    while (true) {
        a.add(pts[i])
        if (i == maxI || a.size > n) break
        i = (i + 1) % n
    }
    val b = ArrayList<P3>()
    i = minI
    while (true) {
        b.add(pts[i])
        if (i == maxI || b.size > n) break
        i = (i - 1 + n) % n
    }
    val za = a.sumOf { it.z.toDouble() } / max(1, a.size)
    val zb = b.sumOf { it.z.toDouble() } / max(1, b.size)
    return if (za >= zb) a else b
}

private fun frontArc(h: Head, y: Float, s: Float): List<P3> {
    val n = 120
    return frontSilhouetteArc((0 until n).map { i -> surf(h, y, -PI.toFloat() + i.toFloat() / n * 2f * PI.toFloat(), s) })
}

private fun poly(pts: List<P3>, close: Boolean = false): Path = Path().apply {
    pts.forEachIndexed { i, q -> if (i == 0) moveTo(q.x, q.y) else lineTo(q.x, q.y) }
    if (close) close()
}

/** The region above the front arc of ring y: what a cap covers. */
private fun capClip(h: Head, y: Float, s: Float, extraTop: Float = 3f): Path {
    val arc = frontArc(h, y, s)
    val p = Path()
    if (arc.isEmpty()) return p
    p.moveTo(arc[0].x - h.rx, arc[0].y)
    for (q in arc) p.lineTo(q.x, q.y)
    p.lineTo(arc.last().x + h.rx, arc.last().y)
    p.lineTo(h.rx * 2, -h.ry * extraTop)
    p.lineTo(-h.rx * 2, -h.ry * extraTop)
    p.close()
    return p
}

/** The complement of [p] in a large rectangle (clip with it: its fill type is even-odd). */
private fun invert(p: Path, h: Head): Path = Path().apply {
    fillType = PathFillType.EvenOdd
    addRect(Rect(-h.rx * 4, -h.ry * 4, h.rx * 4, h.ry * 4))
    addPath(p)
}

/** The head outline, a clean superellipse. */
private fun outfitPath(rx: Float, ry: Float): Path {
    val n = 96
    val e = 2f / EXP
    val p = Path()
    for (i in 0..n) {
        val a = i.toFloat() / n * 2f * PI.toFloat()
        val ca = cos(a)
        val sa = sin(a)
        val x = rx * (if (ca >= 0) ca.pow(e) else -(-ca).pow(e))
        val y = ry * (if (sa >= 0) sa.pow(e) else -(-sa).pow(e))
        if (i == 0) p.moveTo(x, y) else p.lineTo(x, y)
    }
    p.close()
    return p
}

// ── Paint helpers ────────────────────────────────────────────────────────────

private fun c(rgb: Int, a: Float = 1f) = Color(0xFF000000.toInt() or rgb).copy(alpha = a)
private fun rgba(r: Int, g: Int, b: Int, a: Float) = Color(r / 255f, g / 255f, b / 255f, a)
private val Clear = Color(0x00FFFFFF)

private fun lin(x0: Float, y0: Float, x1: Float, y1: Float, vararg stops: Pair<Float, Color>) =
    Brush.linearGradient(colorStops = arrayOf(*stops), start = Offset(x0, y0), end = Offset(x1, y1))

private fun rad(cx: Float, cy: Float, r: Float, vararg stops: Pair<Float, Color>) =
    Brush.radialGradient(colorStops = arrayOf(*stops), center = Offset(cx, cy), radius = max(0.001f, r))

private fun oval(cx: Float, cy: Float, rx: Float, ry: Float) = Path().apply {
    addOval(Rect(cx - abs(rx), cy - abs(ry), cx + abs(rx), cy + abs(ry)))
}

private fun rrect(x: Float, y: Float, w: Float, h: Float, r: Float) = Path().apply {
    addRoundRect(RoundRect(Rect(x, y, x + w, y + h), CornerRadius(r, r)))
}

private fun DrawScope.stroke(p: Path, color: Color, width: Float, cap: StrokeCap = StrokeCap.Round) =
    drawPath(p, color, style = Stroke(width = width, cap = cap, join = StrokeJoin.Round))

private fun DrawScope.fillAll(h: Head, color: Color) =
    drawRect(color, topLeft = Offset(-h.rx * 4, -h.ry * 4), size = Size(h.rx * 8, h.ry * 8))

private fun DrawScope.pompom(x: Float, y: Float, r: Float, base: Color = Color.White, shade: Color = c(0xD5D9E2)) {
    translate(x, y) {
        val n = 11
        for (i in 0 until n) {
            val a = i.toFloat() / n * 2f * PI.toFloat()
            val br = r * (0.34f + 0.06f * sin(i * 2.3f))
            val bx = cos(a) * r * 0.78f
            val by = sin(a) * r * 0.78f
            drawPath(oval(bx, by, br, br), rad(bx - br * 0.4f, by - br * 0.5f, br * 1.3f, 0f to base, 1f to shade))
        }
        drawPath(oval(0f, 0f, r * 0.86f, r * 0.86f), rad(-r * 0.3f, -r * 0.35f, r * 1.05f, 0f to base, 0.7f to base, 1f to shade))
    }
}

private fun DrawScope.fuzzyBand(arc: List<P3>, thick: Float, base: Color = Color.White, shade: Color = c(0xDADDE4)) {
    if (arc.size < 2) return
    val path = poly(arc)
    stroke(path, shade, thick)
    stroke(path, base, thick * 0.78f)
    val step = max(2, arc.size / 16)
    var i = 0
    while (i < arc.size) {
        val q = arc[i]
        val r = thick * (0.32f + 0.1f * sin(i * 1.7f))
        drawPath(oval(q.x, q.y - thick * 0.32f, r, r), rad(q.x - r * 0.3f, q.y - thick * 0.35f - r * 0.3f, r * 1.2f, 0f to base, 1f to shade))
        i += step
    }
}

// ── Hats ─────────────────────────────────────────────────────────────────────

private fun DrawScope.beanie(h: Head, body: Path) {
    val s = 1.035f
    val yEdge = 0.42f
    val yCuff = 0.58f
    val head = outfitPath(h.rx * s, h.ry * s)
    clipPath(body) { clipPath(capClip(h, yEdge - 0.12f, 1f)) { fillAll(h, rgba(30, 40, 70, 0.10f)) } }
    clipPath(capClip(h, yCuff, s)) {
        drawPath(head, lin(h.rx * 0.5f, -h.ry * 1.1f, -h.rx * 0.6f, h.ry * 0.2f, 0f to c(0x7DB6FF), 1f to c(0x2F6FE0)))
        if (!h.simplified) {
            clipPath(head) {
                for (k in -6..6) {
                    val lon = k * 0.24f
                    val pts = (0..16).map { i -> surf(h, yCuff + (1.05f - yCuff) * i / 16f, lon, s) }.filter { it.z > 0 }
                    if (pts.size >= 2) stroke(poly(pts), rgba(20, 50, 140, 0.16f), h.R * 0.045f)
                }
            }
        }
    }
    clipPath(capClip(h, yEdge, s * 1.04f)) {
        clipPath(invert(capClip(h, yCuff, s * 1.04f), h)) {
            val cuff = outfitPath(h.rx * s * 1.04f, h.ry * s * 1.04f)
            drawPath(cuff, lin(0f, -h.ry * 0.6f, 0f, -h.ry * 0.2f, 0f to c(0x3C7BEA), 1f to c(0x2257C4)))
            clipPath(cuff) {
                for (k in -14..14) {
                    val lon = k * 0.115f
                    val a = surf(h, yEdge, lon, s * 1.04f)
                    val b = surf(h, yCuff, lon, s * 1.04f)
                    if (a.z < 0) continue
                    stroke(poly(listOf(a, b)), rgba(10, 30, 100, 0.22f), h.R * 0.035f, StrokeCap.Butt)
                }
            }
        }
    }
    clipPath(capClip(h, yCuff, s)) {
        clipPath(head) {
            drawPath(head, rad(h.rx * 0.3f, -h.ry * 0.85f, h.R * 0.45f, 0f to Color.White.copy(alpha = 0.35f), 1f to Color.White.copy(alpha = 0f)))
        }
    }
    val top = proj(h, 0f, 1.08f * s, 0f)
    pompom(top.x + h.physDx * h.rx * 0.25f, top.y - h.R * 0.12f + h.physDy * h.ry * 0.15f, h.R * 0.24f)
}

private class CapPalette(val bag: Triple<Color, Color, Color>, val shadow: Color, val fold: Color, val trim: Color, val trimShade: Color)

private val Santa = CapPalette(Triple(c(0xFF6B6B), c(0xE53935), c(0xB71C1C)), rgba(120, 10, 10, 0.10f), rgba(90, 0, 0, 0.20f), Color.White, c(0xDADDE4))
private val NightCap = CapPalette(Triple(c(0x818CF8), c(0x4F46E5), c(0x312E81)), rgba(30, 20, 110, 0.12f), rgba(20, 10, 80, 0.22f), c(0xF4F5F9), c(0xC7CAE0))

private fun DrawScope.softCap(h: Head, body: Path, pal: CapPalette) {
    val s = 1.05f
    val yEdge = 0.52f
    val arc = frontArc(h, yEdge, s)
    if (arc.isEmpty()) return
    val l = arc.first()
    val r = arc.last()
    val crown = proj(h, 0f, 1.05f, 0f)
    val tipX = crown.x + h.rx * (0.95f + h.physDx * 0.35f)
    val tipY = crown.y + h.ry * (0.05f + h.physDy * 0.2f)
    val peakX = crown.x + h.rx * 0.25f
    val peakY = crown.y - h.ry * 0.62f
    val bag = Path().apply {
        moveTo(l.x, l.y)
        cubicTo(l.x - h.rx * 0.05f, l.y - h.ry * 0.7f, peakX - h.rx * 0.55f, peakY - h.ry * 0.05f, peakX, peakY)
        quadraticTo(tipX - h.rx * 0.05f, peakY - h.ry * 0.02f, tipX, tipY)
        quadraticTo(tipX - h.rx * 0.12f, tipY - h.ry * 0.22f, peakX + h.rx * 0.18f, peakY + h.ry * 0.32f)
        cubicTo(r.x + h.rx * 0.05f, peakY + h.ry * 0.45f, r.x + h.rx * 0.08f, r.y - h.ry * 0.35f, r.x, r.y)
        for (i in arc.indices.reversed()) lineTo(arc[i].x, arc[i].y)
        close()
    }
    clipPath(body) { clipPath(capClip(h, yEdge - 0.14f, 1f)) { fillAll(h, pal.shadow) } }
    drawPath(bag, lin(-h.rx * 0.6f, -h.ry * 1.6f, h.rx * 0.7f, -h.ry * 0.3f, 0f to pal.bag.first, 0.55f to pal.bag.second, 1f to pal.bag.third))
    clipPath(bag) {
        for ((a, b, w) in listOf(Triple(0.15f, 0.55f, 0.10f), Triple(0.45f, 0.85f, 0.08f))) {
            val f = Path().apply {
                moveTo(peakX - h.rx * 0.1f + (r.x - l.x) * a * 0.3f, peakY + h.ry * 0.15f)
                quadraticTo(peakX + h.rx * 0.35f, peakY + h.ry * (0.05f + a * 0.3f), tipX - h.rx * (0.45f - b * 0.3f), tipY - h.ry * 0.12f)
            }
            stroke(f, pal.fold, h.R * w)
        }
        drawPath(bag, rad(peakX - h.rx * 0.25f, peakY + h.ry * 0.05f, h.R * 0.5f, 0f to Color.White.copy(alpha = 0.32f), 1f to Color.White.copy(alpha = 0f)))
    }
    fuzzyBand(arc, h.R * 0.3f, pal.trim, pal.trimShade)
    pompom(tipX, tipY + h.R * 0.04f, h.R * 0.22f, pal.trim, pal.trimShade)
}

private fun DrawScope.partyHat(h: Head) {
    val baseY = 0.82f
    val baseR = 0.42f
    val lean = -0.24f + h.physDx * 0.12f
    val cen = proj(h, 0.16f, baseY + 0.06f, 0f)
    val ring = (0..48).map { i ->
        val a = i / 48f * 2f * PI.toFloat()
        proj(h, 0.16f + baseR * sin(a), baseY + 0.06f, baseR * cos(a))
    }
    val left = ring.minBy { it.x }
    val right = ring.maxBy { it.x }
    // On a small Mochi the full cone would poke out of the box: a stubbier one.
    val height = h.ry * (if (h.simplified) 0.95f else 1.6f)
    val apexX = cen.x + sin(lean) * height
    val apexY = cen.y - cos(lean) * height
    val front = frontSilhouetteArc(ring)
    val cone = Path().apply {
        moveTo(left.x, left.y)
        quadraticTo((left.x + apexX) / 2 - h.rx * 0.06f, (left.y + apexY) / 2, apexX - h.R * 0.05f, apexY + h.R * 0.06f)
        quadraticTo(apexX, apexY - h.R * 0.03f, apexX + h.R * 0.05f, apexY + h.R * 0.06f)
        quadraticTo((right.x + apexX) / 2 + h.rx * 0.06f, (right.y + apexY) / 2, right.x, right.y)
        for (i in front.indices.reversed()) lineTo(front[i].x, front[i].y)
        close()
    }
    drawPath(cone, lin(left.x, apexY, right.x, left.y, 0f to c(0xFF9BD0), 0.5f to c(0xF15BAE), 1f to c(0xC2187A)))
    clipPath(cone) {
        if (!h.simplified) {
            val dots = listOf(0.25f to -0.35f, 0.3f to 0.3f, 0.55f to -0.05f, 0.72f to 0.28f, 0.8f to -0.3f, 0.45f to 0.6f, 0.48f to -0.65f)
            for ((t, u) in dots) {
                val bx = left.x + (right.x - left.x) * (0.5f + u * 0.5f)
                val by = left.y + (right.y - left.y) * (0.5f + u * 0.5f)
                val px = bx + (apexX - bx) * (1 - t)
                val py = by + (apexY - by) * (1 - t)
                val r = h.R * 0.075f * (0.6f + t * 0.5f)
                drawPath(oval(px, py, r, r * 0.9f), Color.White.copy(alpha = 0.92f))
            }
        }
        drawPath(cone, lin(left.x, 0f, right.x, 0f, 0f to Color.White.copy(alpha = 0.28f), 0.35f to Color.White.copy(alpha = 0f), 1f to rgba(80, 0, 40, 0.18f)))
    }
    if (front.isNotEmpty()) stroke(poly(front), c(0xFFD84D), h.R * 0.07f)
    pompom(apexX, apexY - h.R * 0.04f, h.R * 0.16f, c(0xFFE27A), c(0xF2B705))
}

private const val CROWN_YB = 0.46f

private fun DrawScope.crownPart(h: Head, side: Int) {
    val s = 1.06f
    val yb = CROWN_YB
    val yt = 0.66f
    val n = 8
    val spikeH = 0.42f
    val total = 120
    class Seg(val b: P3, val tt: P3)
    val seg = (0..total).map { i ->
        val lon = -PI.toFloat() + i.toFloat() / total * 2f * PI.toFloat()
        val b = surf(h, yb, lon, s)
        val phase = (lon + PI.toFloat()) / (2f * PI.toFloat()) * n
        val f = phase - floor(phase)
        val spike = max(0f, 1f - abs(f - 0.5f) * 2f).pow(1.6f)
        val topY = yt + spikeH * spike
        val r = ringR(yt) * s * (1f - 0.08f * spike)
        Seg(b, proj(h, r * sin(lon), topY, r * cos(lon)))
    }
    val keep = seg.filter { if (side > 0) it.b.z >= 0f else it.b.z < 0.02f }.sortedBy { it.b.x }
    if (keep.size < 2) return
    val shape = Path().apply {
        moveTo(keep[0].tt.x, keep[0].tt.y)
        for (i in 1 until keep.size) lineTo(keep[i].tt.x, keep[i].tt.y)
        for (i in keep.indices.reversed()) lineTo(keep[i].b.x, keep[i].b.y)
        close()
    }
    val dark = side < 0
    drawPath(
        shape,
        if (dark) lin(0f, -h.ry * 1.05f, 0f, -h.ry * 0.45f, 0f to c(0xC98A12), 1f to c(0x8A5A06))
        else lin(0f, -h.ry * 1.05f, 0f, -h.ry * 0.45f, 0f to c(0xFFE58A), 0.5f to c(0xFBBF24), 1f to c(0xD08A0B)),
    )
    if (dark) return
    clipPath(shape) {
        drawPath(shape, lin(-h.rx, 0f, h.rx, 0f, 0f to rgba(120, 70, 0, 0.25f), 0.45f to Color.White.copy(alpha = 0f), 0.62f to Color.White.copy(alpha = 0.35f), 1f to rgba(120, 70, 0, 0.25f)))
    }
    if (h.simplified) return
    val gems = listOf(c(0xEF4444), c(0x3B82F6), c(0x22C55E), c(0xA855F7))
    for (k in 0 until n) {
        val lon = -PI.toFloat() + (k + 0.5f) / n * 2f * PI.toFloat()
        val rr = ringR(yt) * s
        val tip = proj(h, rr * sin(lon) * 0.92f, yt + spikeH, rr * cos(lon) * 0.92f)
        val mid = surf(h, (yb + yt) / 2f, lon, s * 1.01f)
        if (mid.z <= 0.12f) continue
        val r = h.R * 0.055f
        drawPath(oval(tip.x, tip.y - r * 0.5f, r, r), rad(tip.x - r * 0.3f, tip.y - r, r * 1.2f, 0f to c(0xFFF6CC), 1f to c(0xE0A21A)))
        val gr = h.R * 0.075f
        val gw = gr * max(0.35f, mid.z)
        drawPath(oval(mid.x, mid.y, gw, gr), gems[k % gems.size])
        drawPath(oval(mid.x - gr * 0.25f * mid.z, mid.y - gr * 0.35f, gr * 0.28f, gr * 0.28f), Color.White.copy(alpha = 0.75f))
    }
}

private fun DrawScope.crownFront(h: Head, body: Path) {
    clipPath(body) {
        clipPath(capClip(h, CROWN_YB - 0.1f, 1f)) {
            clipPath(invert(capClip(h, CROWN_YB, 1f), h)) { fillAll(h, rgba(80, 50, 0, 0.12f)) }
        }
    }
    crownPart(h, 1)
}

private fun witchBrim(h: Head): List<P3> = (0..120).map { i ->
    val a = -PI.toFloat() + i / 120f * 2f * PI.toFloat()
    val wob = 1f + 0.035f * sin(a * 3f + 0.6f)
    val droop = -0.10f * abs(sin(a)).pow(2)
    proj(h, 1.42f * wob * sin(a), 0.70f + droop, 1.42f * wob * cos(a))
}

private fun DrawScope.witchHatBack(h: Head) {
    val pts = witchBrim(h)
    if (pts.none { it.z < 0.05f }) return
    drawPath(poly(pts, true), lin(0f, -h.ry, 0f, -h.ry * 0.4f, 0f to c(0x2A0A4F), 1f to c(0x3B0F6B)))
}

private fun DrawScope.witchHatFront(h: Head, body: Path) {
    val all = witchBrim(h)
    val fr = all.filter { it.z >= 0f }.sortedBy { it.x }
    clipPath(body) { clipPath(capClip(h, 0.5f, 1f)) { fillAll(h, rgba(40, 0, 70, 0.10f)) } }
    drawPath(poly(all, true), lin(0f, -h.ry * 0.9f, 0f, -h.ry * 0.3f, 0f to c(0x5B21B6), 1f to c(0x3B0764)))
    if (fr.isNotEmpty()) stroke(poly(fr), rgba(190, 150, 255, 0.35f), h.R * 0.035f)

    val baseR = 0.62f
    val by = 0.74f
    val bl = proj(h, -baseR, by, 0f)
    val br = proj(h, baseR, by, 0f)
    val cen = proj(h, 0f, by, 0f)
    val lean = 0.10f + h.physDx * 0.15f
    val topX = cen.x + h.rx * 0.18f + sin(lean) * h.ry * 0.3f
    val topY = cen.y - h.ry * 1.25f
    val tipX = topX + h.rx * (0.45f + h.physDx * 0.25f)
    val tipY = topY + h.ry * (0.22f + h.physDy * 0.1f)
    val capFront = frontArc(h, by, baseR / ringR(by)).filter { it.x >= bl.x - 1 && it.x <= br.x + 1 }
    val cone = Path().apply {
        moveTo(bl.x, bl.y)
        cubicTo(bl.x + h.rx * 0.12f, bl.y - h.ry * 0.5f, topX - h.rx * 0.28f, topY + h.ry * 0.25f, topX - h.rx * 0.02f, topY - h.ry * 0.02f)
        quadraticTo(topX + h.rx * 0.25f, topY - h.ry * 0.08f, tipX, tipY)
        quadraticTo(topX + h.rx * 0.22f, topY + h.ry * 0.08f, topX + h.rx * 0.14f, topY + h.ry * 0.22f)
        cubicTo(br.x - h.rx * 0.18f, cen.y - h.ry * 0.45f, br.x - h.rx * 0.02f, br.y - h.ry * 0.2f, br.x, br.y)
        for (i in capFront.indices.reversed()) lineTo(capFront[i].x, capFront[i].y)
        close()
    }
    drawPath(cone, lin(bl.x, topY, br.x, bl.y, 0f to c(0x7C3AED), 0.55f to c(0x4C1D95), 1f to c(0x2E1065)))
    clipPath(cone) {
        drawPath(cone, lin(bl.x, 0f, br.x, 0f, 0f to Color.White.copy(alpha = 0.22f), 0.4f to Color.White.copy(alpha = 0f), 1f to Color.Black.copy(alpha = 0.15f)))
        val crease = Path().apply {
            moveTo(topX - h.rx * 0.05f, topY + h.ry * 0.05f)
            quadraticTo(topX + h.rx * 0.1f, topY + h.ry * 0.12f, topX + h.rx * 0.2f, topY + h.ry * 0.06f)
        }
        stroke(crease, rgba(20, 0, 40, 0.35f), h.R * 0.05f)
        val fc = proj(h, 0f, by, baseR)
        val lift = h.ry * 0.11f
        val band = Path().apply {
            moveTo(bl.x - 2, bl.y - lift)
            quadraticTo(fc.x, 2 * (fc.y - lift) - (bl.y + br.y) / 2, br.x + 2, br.y - lift)
        }
        stroke(band, c(0xF97316), h.ry * 0.17f, StrokeCap.Butt)
    }
    val bk = proj(h, 0f, by, baseR)
    val bkY = bk.y - h.ry * 0.11f
    val bw = h.R * 0.2f
    val bh = h.R * 0.16f
    drawPath(rrect(bk.x - bw / 2, bkY - bh / 2, bw, bh, bh * 0.25f), c(0xFCD34D))
    drawPath(rrect(bk.x - bw / 2 + bw * 0.24f, bkY - bh / 2 + bh * 0.28f, bw * 0.52f, bh * 0.44f, bh * 0.1f), c(0xC2410C))
}

private fun DrawScope.bunnyEars(h: Head) {
    val earH = h.R * (if (h.simplified) 0.6f else 0.85f)
    for (sd in listOf(-1f, 1f)) {
        val root = proj(h, sd * 0.45f, 0.92f, 0f)
        val rl = proj(h, sd * 0.45f - 0.22f, 0.92f, 0f)
        val rr = proj(h, sd * 0.45f + 0.22f, 0.92f, 0f)
        val hw = max(h.R * 0.04f, abs(rr.x - rl.x) / 2f)
        translate(root.x, root.y - earH * 0.65f + earH * 0.5f) {
            drawPath(oval(0f, 0f, hw, earH / 2f), c(0xF9F0F0))
            drawPath(oval(0f, 0f, hw, earH / 2f), Color.Black.copy(alpha = 0.06f), style = Stroke(width = 0.8f))
            drawPath(oval(0f, -earH / 2f + h.R * 0.1f + earH * 0.325f, hw * 0.5f, earH * 0.325f), c(0xFCA5A5, 0.70f))
        }
    }
}

// ── Face and neck (they follow his eyes) ─────────────────────────────────────

private class Eye(val sd: Float, val x: Float, val y: Float, val fx: Float, val fy: Float, val visible: Boolean)

private fun eyes(h: Head): List<Eye> = listOf(-1f, 1f).map { sd ->
    val eyeYaw = sd * EYE_SP + h.yaw
    val eyePitch = EYE_P + h.pitch
    val cp = cos(eyePitch)
    Eye(sd, sin(eyeYaw) * cp * h.rx, -sin(eyePitch) * h.ry, max(0.18f, cos(eyeYaw)), max(0.18f, cp), cos(eyeYaw) * cp > 0.04f)
}

private fun DrawScope.sunglasses(h: Head, body: Path) {
    val es = eyes(h)
    val w = h.R * 0.62f
    val hh = h.R * 0.46f
    clipPath(body) {
        val (le, re) = es
        val ink = c(0x111317)
        if (le.visible && re.visible) {
            val b = Path().apply {
                moveTo(le.x + w / 2 * le.fx * 0.9f, le.y - hh * 0.18f)
                quadraticTo((le.x + re.x) / 2, (le.y + re.y) / 2 - hh * 0.42f, re.x - w / 2 * re.fx * 0.9f, re.y - hh * 0.18f)
            }
            stroke(b, ink, h.R * 0.07f)
        }
        for (e in es) {
            if (!e.visible) continue
            val t = Path().apply {
                moveTo(e.x + e.sd * w / 2 * e.fx, e.y - hh * 0.2f)
                lineTo(e.sd * h.rx * 1.05f, e.y - hh * 0.35f)
            }
            stroke(t, ink, h.R * 0.06f)
        }
        for (e in es) {
            if (!e.visible) continue
            translate(e.x, e.y) {
                scale(e.fx, e.fy, Offset.Zero) {
                    val lens = rrect(-w / 2, -hh / 2, w, hh, hh * 0.42f)
                    drawPath(lens, rgba(17, 19, 23, 0.82f))
                    drawPath(lens, c(0x0B0C0F), style = Stroke(width = h.R * 0.05f))
                    val glare = Path().apply { moveTo(-w * 0.28f, -hh * 0.05f); lineTo(-w * 0.05f, -hh * 0.3f) }
                    stroke(glare, Color.White.copy(alpha = 0.45f), h.R * 0.05f)
                }
            }
        }
    }
}

private fun DrawScope.roundGlasses(h: Head, body: Path) {
    val es = eyes(h)
    val d = h.R * 0.56f
    clipPath(body) {
        val (le, re) = es
        val wire = c(0x8A4B12)
        if (le.visible && re.visible) {
            val b = Path().apply {
                moveTo(le.x + d / 2 * le.fx, le.y - d * 0.08f)
                quadraticTo((le.x + re.x) / 2, (le.y + re.y) / 2 - d * 0.3f, re.x - d / 2 * re.fx, re.y - d * 0.08f)
            }
            stroke(b, wire, h.R * 0.055f)
        }
        for (e in es) {
            if (!e.visible) continue
            val t = Path().apply {
                moveTo(e.x + e.sd * d / 2 * e.fx, e.y - d * 0.1f)
                lineTo(e.sd * h.rx * 1.05f, e.y - d * 0.25f)
            }
            stroke(t, wire, h.R * 0.05f)
        }
        for (e in es) {
            if (!e.visible) continue
            translate(e.x, e.y) {
                scale(e.fx, e.fy, Offset.Zero) {
                    val ring = oval(0f, 0f, d / 2, d / 2)
                    drawPath(ring, rgba(190, 225, 255, 0.18f))
                    drawPath(ring, c(0x9A5A1A), style = Stroke(width = h.R * 0.065f))
                    val arc = Path().apply {
                        val r = d / 2 - h.R * 0.03f
                        val a0 = PI.toFloat() * 1.1f
                        val a1 = PI.toFloat() * 1.45f
                        moveTo(cos(a0) * r, sin(a0) * r)
                        for (k in 1..8) {
                            val a = a0 + (a1 - a0) * k / 8f
                            lineTo(cos(a) * r, sin(a) * r)
                        }
                    }
                    stroke(arc, Color.White.copy(alpha = 0.55f), h.R * 0.03f)
                }
            }
        }
    }
}

private fun DrawScope.scarf(h: Head) {
    val s = 1.05f
    val y0 = -0.34f
    val y1 = -0.66f
    val top = frontSilhouetteArc((0 until 120).map { i -> surf(h, y0, -PI.toFloat() + i / 120f * 2f * PI.toFloat(), s) })
    val bot = frontSilhouetteArc((0 until 120).map { i -> surf(h, y1, -PI.toFloat() + i / 120f * 2f * PI.toFloat(), s) })
    if (top.isEmpty() || bot.isEmpty()) return
    val band = poly(top).apply {
        for (i in bot.indices.reversed()) lineTo(bot[i].x, bot[i].y)
        close()
    }
    clipPath(outfitPath(h.rx * s, h.ry * s)) {
        drawPath(band, lin(0f, -h.ry * 0.2f, 0f, h.ry * 0.7f, 0f to c(0xF87171), 1f to c(0xB91C1C)))
        clipPath(band) {
            for (lon in listOf(-1.0f, -0.45f, 0.1f, 0.65f, 1.2f)) {
                val a = surf(h, y0, lon, s)
                val b = surf(h, y1, lon, s)
                if (a.z < 0) continue
                val st = Path().apply { moveTo(a.x, a.y - 4); lineTo(b.x, b.y + 4) }
                stroke(st, Color.White.copy(alpha = 0.85f), h.R * 0.09f * max(0.3f, a.z))
            }
        }
        drawPath(band, lin(0f, -h.ry * 0.5f, 0f, h.ry * 0.3f, 0f to Color.White.copy(alpha = 0.18f), 1f to Color.Black.copy(alpha = 0.10f)))
    }
    val k = surf(h, (y0 + y1) / 2f, -0.55f, s * 1.03f)
    if (k.z <= 0) return
    val sw = h.physDx * h.rx * 0.12f
    val end = Path().apply {
        moveTo(k.x - h.R * 0.16f, k.y)
        quadraticTo(k.x - h.R * 0.24f + sw, k.y + h.ry * 0.35f, k.x - h.R * 0.2f + sw * 1.4f, k.y + h.ry * 0.62f)
        lineTo(k.x + h.R * 0.06f + sw * 1.4f, k.y + h.ry * 0.6f)
        quadraticTo(k.x + h.R * 0.02f + sw, k.y + h.ry * 0.3f, k.x + h.R * 0.12f, k.y)
        close()
    }
    drawPath(end, lin(0f, k.y, 0f, k.y + h.ry * 0.6f, 0f to c(0xEF4444), 1f to c(0xB91C1C)))
    clipPath(end) {
        for (t in listOf(0.35f, 0.7f)) {
            drawRect(Color.White.copy(alpha = 0.85f), Offset(k.x - h.R * 0.4f + sw, k.y + h.ry * 0.62f * t), Size(h.R * 0.8f, h.R * 0.07f))
        }
    }
    for (i in 0 until 4) {
        val fx = k.x - h.R * 0.17f + sw * 1.4f + i * h.R * 0.075f
        val f = Path().apply { moveTo(fx, k.y + h.ry * 0.6f); lineTo(fx, k.y + h.ry * 0.72f) }
        stroke(f, c(0xDC2626), h.R * 0.035f)
    }
    translate(k.x, k.y) {
        rotate(0.2f * 180f / PI.toFloat(), Offset.Zero) {
            drawPath(oval(0f, 0f, h.R * 0.17f, h.R * 0.14f), rad(-h.R * 0.05f, -h.R * 0.05f, h.R * 0.2f, 0f to c(0xF87171), 1f to c(0xB91C1C)))
        }
    }
}

private fun DrawScope.pumpkin(h: Head, body: Path) {
    // The body is recoloured by the caller; this draws ribs, stem, leaf and tendril.
    if (!h.simplified) {
        clipPath(body) {
            for (lon in listOf(-1.15f, -0.55f, 0f, 0.55f, 1.15f)) {
                val pts = (0..30).map { i -> surf(h, -0.98f + 1.96f * i / 30f, lon, 1f) }.filter { it.z > 0 }
                if (pts.size < 2) continue
                val zz = pts[pts.size / 2].z
                stroke(poly(pts), rgba(150, 50, 0, 0.22f * zz), h.R * 0.12f)
                stroke(poly(pts.map { P3(it.x + h.R * 0.07f, it.y, it.z) }), rgba(255, 220, 170, 0.18f * zz), h.R * 0.04f)
            }
        }
    }
    val t = proj(h, 0.02f, 1.0f, 0f)
    val stem = Path().apply {
        moveTo(t.x - h.R * 0.09f, t.y + h.R * 0.04f)
        quadraticTo(t.x - h.R * 0.08f, t.y - h.R * 0.22f, t.x + h.R * 0.08f, t.y - h.R * 0.3f)
        lineTo(t.x + h.R * 0.13f, t.y - h.R * 0.22f)
        quadraticTo(t.x + h.R * 0.04f, t.y - h.R * 0.15f, t.x + h.R * 0.08f, t.y + h.R * 0.04f)
        close()
    }
    drawPath(stem, lin(t.x - h.R * 0.1f, 0f, t.x + h.R * 0.1f, 0f, 0f to c(0x65A30D), 1f to c(0x3F6212)))
    translate(t.x - h.R * 0.06f, t.y - h.R * 0.02f) {
        rotate(-0.5f * 180f / PI.toFloat(), Offset.Zero) {
            val leaf = Path().apply {
                moveTo(0f, 0f)
                quadraticTo(-h.R * 0.18f, -h.R * 0.2f, -h.R * 0.38f, -h.R * 0.02f)
                quadraticTo(-h.R * 0.18f, h.R * 0.1f, 0f, 0f)
            }
            drawPath(leaf, lin(0f, -h.R * 0.15f, -h.R * 0.3f, 0f, 0f to c(0x84CC16), 1f to c(0x4D7C0F)))
            val vein = Path().apply { moveTo(-h.R * 0.02f, -h.R * 0.01f); quadraticTo(-h.R * 0.18f, -h.R * 0.08f, -h.R * 0.32f, -h.R * 0.03f) }
            stroke(vein, rgba(30, 60, 0, 0.4f), h.R * 0.02f)
        }
    }
    if (!h.simplified) {
        val tendril = Path().apply {
            moveTo(t.x + h.R * 0.1f, t.y - h.R * 0.12f)
            cubicTo(t.x + h.R * 0.3f, t.y - h.R * 0.25f, t.x + h.R * 0.35f, t.y - h.R * 0.02f, t.x + h.R * 0.22f, t.y - h.R * 0.06f)
        }
        stroke(tendril, c(0x4D7C0F), h.R * 0.03f)
    }
}

private fun DrawScope.bow(h: Head) {
    val a = surf(h, 0.86f, 0.55f, 1.02f)
    if (a.z < -0.2f) return
    val s = h.R * 0.26f
    val sq = max(0.45f, cos(0.55f + h.yaw))
    translate(a.x, a.y) {
        rotate((0.35f + h.yaw * 0.3f) * 180f / PI.toFloat(), Offset.Zero) {
            scale(sq, 1f, Offset.Zero) {
                for (sd in listOf(-1f, 1f)) {
                    val wing = Path().apply {
                        moveTo(0f, 0f)
                        cubicTo(sd * s * 0.6f, -s * 0.85f, sd * s * 1.35f, -s * 0.55f, sd * s * 1.15f, 0f)
                        cubicTo(sd * s * 1.35f, s * 0.55f, sd * s * 0.6f, s * 0.85f, 0f, 0f)
                    }
                    drawPath(wing, lin(0f, -s, 0f, s, 0f to c(0xFF8CC6), 1f to c(0xDB2777)))
                    val crease = Path().apply { moveTo(sd * s * 0.25f, -s * 0.05f); quadraticTo(sd * s * 0.7f, -s * 0.15f, sd * s * 0.95f, -s * 0.05f) }
                    stroke(crease, rgba(140, 10, 70, 0.35f), s * 0.08f)
                }
                drawPath(oval(0f, 0f, s * 0.24f, s * 0.30f), rad(-s * 0.06f, -s * 0.1f, s * 0.35f, 0f to c(0xFFB3D9), 1f to c(0xC2185B)))
            }
        }
    }
}

// ── Headphones (added on the PC and the phone) ───────────────────────────────

private fun DrawScope.headphones(h: Head, front: Boolean, t: Float) {
    val dark = c(0x2B2E38)
    val pts = (0..40).map { i ->
        val a = i / 40f * PI.toFloat()
        proj(h, 1.06f * cos(a), 0.02f + 1.04f * sin(a), 0f)
    }
    var run = ArrayList<P3>()
    for (q in pts) {
        if ((q.z >= 0f) == front) run.add(q) else {
            if (run.size >= 2) stroke(poly(run), dark, h.R * 0.15f)
            run = ArrayList()
        }
    }
    if (run.size >= 2) stroke(poly(run), dark, h.R * 0.15f)
    val pulse = 1f + 0.04f * sin(t * 7f)
    val wide = 0.6f + 0.4f * abs(sin(h.yaw))
    for (sd in listOf(-1f, 1f)) {
        val cc = proj(h, sd * 1.06f, 0f, 0f)
        if ((cc.z >= 0f) != front) continue
        translate(cc.x, cc.y) {
            scale(pulse * wide, pulse, Offset.Zero) {
                drawPath(rrect(-h.R * 0.17f, -h.R * 0.33f, h.R * 0.34f, h.R * 0.66f, h.R * 0.15f), dark)
                drawPath(rrect(-h.R * 0.07f - sd * h.R * 0.04f, -h.R * 0.2f, h.R * 0.14f, h.R * 0.4f, h.R * 0.07f), c(0xFF5C7A))
            }
        }
    }
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

private fun back(t: Float): Float {
    val c1 = 1.7f
    val c3 = c1 + 1f
    return 1f + c3 * (t - 1f).pow(3) + c1 * (t - 1f).pow(2)
}

/** Does this outfit recolour his body (so the glyph paints it itself)? */
fun Outfit.recolorsBody() = this == Outfit.Pumpkin

/**
 * Draws what is behind the body. [cx], [cy] is the middle of his head and [r] the unit
 * (width / 2.28); [presence] is 0 → 1 as the outfit goes on; [eyeDy] lifts what sits on his eyes
 * to where the phone's Mochi has them (his eyes are a little higher than the Mac's).
 */
fun DrawScope.drawOutfitBehind(outfit: Outfit, head: Head, cx: Float, cy: Float, presence: Float, t: Float) {
    if (outfit == Outfit.None || presence <= 0.005f) return
    val posP = back(presence.coerceIn(0f, 1f))
    val hat = 0.85f + 0.15f * posP
    translate(cx, cy) {
        when (outfit) {
            Outfit.Headphones -> headphones(head, false, t)
            Outfit.BunnyEars, Outfit.Crown, Outfit.WitchHat -> translate(0f, -(1f - posP) * head.ry) {
                scale(hat, hat, Offset.Zero) {
                    when (outfit) {
                        Outfit.BunnyEars -> bunnyEars(head)
                        Outfit.Crown -> crownPart(head, -1)
                        else -> witchHatBack(head)
                    }
                }
            }
            // The pieces that follow his eyes (glasses, bow, scarf, pumpkin) have nothing behind him:
            // he never faces away on the phone.
            else -> Unit
        }
    }
}

/** Draws what is in front of the body, after his eyes. See [drawOutfitBehind]. */
fun DrawScope.drawOutfitFront(
    outfit: Outfit, head: Head, cx: Float, cy: Float, presence: Float, t: Float, eyeDy: Float = 0f,
) {
    if (outfit == Outfit.None || outfit == Outfit.BunnyEars || presence <= 0.005f) return
    val p = presence.coerceIn(0f, 1f)
    val posP = back(p)
    val hat = 0.85f + 0.15f * posP
    val body = outfitPath(head.rx, head.ry)
    translate(cx, cy) {
        when (outfit) {
            Outfit.Headphones -> headphones(head, true, t)
            Outfit.Sunglasses, Outfit.RoundGlasses -> translate(0f, eyeDy + (1f - p) * 0.25f * head.ry) {
                if (outfit == Outfit.Sunglasses) sunglasses(head, body) else roundGlasses(head, body)
            }
            Outfit.Scarf -> translate(0f, (1f - p) * 0.3f * head.ry) { scarf(head) }
            Outfit.Pumpkin -> pumpkin(head, body)
            Outfit.Bow -> scale(max(0.001f, posP), max(0.001f, posP), Offset.Zero) { bow(head) }
            else -> translate(0f, -(1f - posP) * head.ry) {
                scale(hat, hat, Offset.Zero) {
                    when (outfit) {
                        Outfit.Beanie -> beanie(head, body)
                        Outfit.SantaHat -> softCap(head, body, Santa)
                        Outfit.Nightcap -> softCap(head, body, NightCap)
                        Outfit.PartyHat -> partyHat(head)
                        Outfit.Crown -> crownFront(head, body)
                        Outfit.WitchHat -> witchHatFront(head, body)
                        else -> Unit
                    }
                }
            }
        }
    }
}

/** What he wears here: the person's choice in Settings (or the auto rule) for this mood. */
@androidx.compose.runtime.Composable
fun rememberMochiOutfit(mood: MochiMood): Outfit {
    val ctx = androidx.compose.ui.platform.LocalContext.current
    val setting = with(com.agupta07505.smartisland.coucou.phone.PhoneSettings) { ctx.mochiOutfit }
    return Wardrobe.pick(setting, mood)
}

/** The pumpkin's body paint (top and bottom of its gradient). */
val PumpkinTop = c(0xFFA94D)
val PumpkinBottom = c(0xE8590C)

/** Rounds a value for tests. */
internal fun Float.r2() = (this * 100f).roundToInt() / 100f
