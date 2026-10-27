/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.wifi.WifiManager
import android.os.BatteryManager
import android.telephony.TelephonyManager
import androidx.core.content.ContextCompat
import kotlin.math.cos
import kotlin.math.sin

/**
 * The status bar's essentials as one small gauge, drawn into the home wallpaper: the battery as an arc (red at 20 % or
 * less), its number on top, the connection symbol in the middle (Wi-Fi arcs, or bars on mobile data) and four dots under
 * it for the signal. Reads only what any app may read; the mobile signal needs READ_PHONE_STATE, and without it the dots
 * show the Wi-Fi level instead.
 */
class HomeStatusBadge(context: Context) {
    private val app = context.applicationContext
    private var at = 0L
    private var battery = -1
    private var charging = false
    private var onWifi = false
    private var onMobile = false
    private var wifiLevel = 0       // 0..4
    private var cellLevel = -1      // 0..4, -1 = unknown (no permission)

    private val box = RectF()
    private val p = Paint(Paint.ANTI_ALIAS_FLAG).apply { strokeCap = Paint.Cap.ROUND }
    private val text = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.BOLD)
        textAlign = Paint.Align.CENTER
    }

    /** At most every 5 s; everything here is a cheap local read. */
    private fun refresh() {
        val now = System.currentTimeMillis()
        if (now - at < 5000L) return
        at = now
        runCatching {
            val b = app.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
            val level = b?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
            val scale = b?.getIntExtra(BatteryManager.EXTRA_SCALE, 100) ?: 100
            val status = b?.getIntExtra(BatteryManager.EXTRA_STATUS, -1) ?: -1
            battery = if (level >= 0) level * 100 / scale.coerceAtLeast(1) else -1
            charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL
        }
        runCatching {
            val cm = app.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
            val caps = cm.getNetworkCapabilities(cm.activeNetwork)
            onWifi = caps?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true
            onMobile = caps?.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) == true
        }
        runCatching {
            if (onWifi) {
                @Suppress("DEPRECATION")
                val rssi = (app.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager).connectionInfo.rssi
                @Suppress("DEPRECATION")
                wifiLevel = WifiManager.calculateSignalLevel(rssi, 5)
            }
        }
        cellLevel = if (ContextCompat.checkSelfPermission(app, Manifest.permission.READ_PHONE_STATE) == PackageManager.PERMISSION_GRANTED) {
            runCatching { (app.getSystemService(Context.TELEPHONY_SERVICE) as TelephonyManager).signalStrength?.level }.getOrNull() ?: -1
        } else -1
    }

    /** Centred on ([cx], [cy]), [d] across; [tint] colours the battery arc. */
    fun draw(c: Canvas, cx: Float, cy: Float, d: Float, tint: Int) {
        refresh()
        val r = d / 2f
        val ink = Color.argb(240, 255, 255, 255)
        val dim = Color.argb(70, 255, 255, 255)

        // battery arc: 240 degrees open at the bottom, filled clockwise from the lower left
        p.style = Paint.Style.STROKE
        p.strokeWidth = d * 0.07f
        box.set(cx - r, cy - r, cx + r, cy + r)
        p.color = Color.argb(60, 255, 255, 255)
        c.drawArc(box, 150f, 240f, false, p)
        if (battery >= 0) {
            p.color = if (battery <= 20 && !charging) Color.rgb(255, 99, 99) else tint
            c.drawArc(box, 150f, 240f * battery / 100f, false, p)
        }

        // the number on top of the gauge (a bolt before it while charging)
        text.textSize = d * 0.21f
        text.color = if (battery in 0..20 && !charging) Color.rgb(255, 130, 130) else ink
        if (battery >= 0) c.drawText((if (charging) "⚡" else "") + battery, cx, cy - r * 0.40f, text)

        // the connection: Wi-Fi arcs, or ascending bars on mobile data, dimmed with nothing
        p.strokeWidth = d * 0.05f
        val gy = cy + r * 0.30f
        when {
            onMobile && !onWifi -> {
                p.style = Paint.Style.STROKE
                val lit = if (cellLevel >= 0) cellLevel else 3
                for (i in 0 until 4) {
                    val x = cx + (i - 1.5f) * d * 0.10f
                    val hgt = d * (0.07f + 0.05f * i)
                    p.color = if (i < lit) ink else dim
                    c.drawLine(x, gy, x, gy - hgt, p)
                }
            }
            else -> {
                val on = onWifi
                val lit = if (on) (wifiLevel * 3 / 4).coerceIn(0, 3).let { if (wifiLevel > 0 && it == 0) 1 else it } else 0
                p.style = Paint.Style.FILL
                p.color = if (on) ink else dim
                c.drawCircle(cx, gy, d * 0.04f, p)
                p.style = Paint.Style.STROKE
                for (i in 0 until 3) {
                    val rr = d * (0.11f + 0.075f * i)
                    p.color = if (i < lit || (on && wifiLevel >= 3)) ink else dim
                    box.set(cx - rr, gy - rr, cx + rr, gy + rr)
                    c.drawArc(box, 225f, 90f, false, p)
                }
            }
        }

        // signal dots under it, on a smile: the mobile level when it is known, else the Wi-Fi level
        val level = if (cellLevel >= 0) cellLevel else if (onWifi) wifiLevel else 0
        p.style = Paint.Style.FILL
        val sizes = floatArrayOf(0.030f, 0.040f, 0.040f, 0.030f)
        for (i in 0 until 4) {
            val ang = Math.toRadians((62.0 + i * 18.7))
            p.color = if (i < level) ink else dim
            c.drawCircle(cx + (r * 0.80f * cos(ang)).toFloat(), cy + (r * 0.80f * sin(ang)).toFloat(), d * sizes[i], p)
        }
    }
}
