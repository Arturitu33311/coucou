/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.audiolink

import android.graphics.Bitmap
import java.io.ByteArrayOutputStream
import java.security.MessageDigest

/** Turns the cover an app gives the phone's media session into a small JPEG for the laptop. */
object NowArt {
    private const val SIDE = 320
    private val QUALITIES = intArrayOf(85, 70, 55, 40, 25)

    /** A JPEG of at most `NowWire.MAX_ART` bytes with its longest side at most `SIDE`, or null if it cannot be made. */
    fun encode(src: Bitmap): ByteArray? {
        if (src.width <= 0 || src.height <= 0) return null
        val k = minOf(1f, SIDE.toFloat() / maxOf(src.width, src.height))
        val bmp = if (k < 1f) Bitmap.createScaledBitmap(src, (src.width * k).toInt().coerceAtLeast(1), (src.height * k).toInt().coerceAtLeast(1), true) else src
        for (q in QUALITIES) {
            val out = ByteArrayOutputStream()
            if (!bmp.compress(Bitmap.CompressFormat.JPEG, q, out)) return null
            if (out.size() <= NowWire.MAX_ART) return out.toByteArray()
        }
        return null
    }

    /**
     * The laptop's cover as a bitmap of at most about `SIDE` pixels a side, or null. A small file can declare a huge
     * picture, so the size is read first and the picture is decoded already reduced (or not at all).
     */
    fun decodeSmall(bytes: ByteArray): Bitmap? {
        if (bytes.isEmpty()) return null
        return try {
            val probe = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
            android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, probe)
            val longest = maxOf(probe.outWidth, probe.outHeight)
            if (probe.outWidth <= 0 || probe.outHeight <= 0 || longest > 8192) return null
            val opts = android.graphics.BitmapFactory.Options().apply { inSampleSize = sampleFor(longest) }
            android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)
        } catch (_: Throwable) {
            null
        }
    }

    /** The power of two that brings `longest` pixels to at most `SIDE`·2 (a decode never needs more). */
    fun sampleFor(longest: Int): Int {
        var s = 1
        while (longest / s > SIDE * 2) s *= 2
        return s
    }

    /** 16 hex digits identifying the image bytes: the laptop names the cover's file after it. */
    fun hash(image: ByteArray): String =
        MessageDigest.getInstance("SHA-1").digest(image).joinToString("") { "%02x".format(it) }.take(16)
}
