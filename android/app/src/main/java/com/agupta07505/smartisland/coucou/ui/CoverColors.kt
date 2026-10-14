/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import android.graphics.Bitmap
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color

/**
 * The cover's colour, so the island dresses in what is playing (like Coucou on the PC does).
 * Computed by hand on an 8×8 copy — no Palette dependency, which matters because this project
 * builds offline and every new artifact must already be in the Gradle cache.
 */
object CoverColors {
    /**
     * Picks the pixel bucket with the most "colourfulness" (saturated, bright, frequent). Greys,
     * black, white and transparent pixels never win: a monochrome cover returns null so the
     * caller falls back to its setting colour instead of painting the island grey.
     *
     * Works on raw ARGB ints with its own RGB→HSV, so it runs on plain JVM tests too (the
     * framework's Color is a stub there). Returns packed ARGB.
     */
    fun dominant(argb: IntArray): Int? {
        // Hue in 24 slices, saturation/value each coarse (dim/vivid): small enough to be instant.
        val weight = HashMap<Int, Double>()
        val sumR = HashMap<Int, Long>()
        val sumG = HashMap<Int, Long>()
        val sumB = HashMap<Int, Long>()
        val count = HashMap<Int, Long>()
        for (px in argb) {
            if ((px ushr 24) < 128) continue
            val r = (px shr 16) and 0xFF
            val g = (px shr 8) and 0xFF
            val b = px and 0xFF
            val (h, s, v) = rgbToHsv(r, g, b)
            // No brightness ceiling: vivid red is value 1.0 too. White needs no special case —
            // it has no saturation, so the check below already skips it.
            if (s < 0.25f || v < 0.15f) continue
            val key = (h / 15).toInt() * 100 + (s * 2).toInt() * 10 + (v * 2).toInt()
            val w = (0.4 + 0.6 * s) * v
            weight[key] = (weight[key] ?: 0.0) + w
            sumR[key] = (sumR[key] ?: 0) + r
            sumG[key] = (sumG[key] ?: 0) + g
            sumB[key] = (sumB[key] ?: 0) + b
            count[key] = (count[key] ?: 0) + 1
        }
        val best = weight.maxByOrNull { it.value }?.key ?: return null
        val n = count[best] ?: return null
        return (0xFF shl 24) or ((sumR[best]!! / n).toInt() shl 16) or ((sumG[best]!! / n).toInt() shl 8) or (sumB[best]!! / n).toInt()
    }

    private fun rgbToHsv(r: Int, g: Int, b: Int): Triple<Float, Float, Float> {
        val rf = r / 255f
        val gf = g / 255f
        val bf = b / 255f
        val max = maxOf(rf, gf, bf)
        val min = minOf(rf, gf, bf)
        val d = max - min
        val h = when {
            d == 0f -> 0f
            max == rf -> 60f * (((gf - bf) / d) % 6)
            max == gf -> 60f * ((bf - rf) / d + 2)
            else -> 60f * ((rf - gf) / d + 4)
        }
        val s = if (max == 0f) 0f else d / max
        return Triple((h + 360) % 360, s, max)
    }
}

/** The cover's colour as a Compose [Color], or null when the cover has none (then use the setting). */
fun Bitmap.coverColor(): Color? = runCatching {
    val small = Bitmap.createScaledBitmap(this, 8, 8, true)
    val px = IntArray(64)
    small.getPixels(px, 0, 8, 0, 0, 8, 8)
    if (small != this) small.recycle()
    CoverColors.dominant(px)?.let { Color(it) }
}.getOrNull()

/** Remembers the cover colour while this artwork is on screen; recomputes only per new artwork. */
@Composable
fun rememberCoverColor(artwork: Bitmap?): Color? = remember(artwork) { artwork?.coverColor() }
