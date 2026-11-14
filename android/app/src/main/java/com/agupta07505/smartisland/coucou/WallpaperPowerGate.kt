/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.SharedPreferences
import android.os.BatteryManager
import android.os.PowerManager
import androidx.core.content.ContextCompat
import com.agupta07505.smartisland.coucou.phone.PhoneSettings
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.wallpaperSaver

/** The "Wallpapers en ahorro de batería" switch: [AUTO] follows Android's battery saver (or a nearly flat battery). */
enum class SaverPolicy {
    AUTO, ALWAYS, NEVER;

    companion object {
        fun of(id: String?): SaverPolicy = entries.firstOrNull { it.name.equals(id, ignoreCase = true) } ?: AUTO
    }
}

/**
 * One place that decides how detailed the animated wallpapers are: normal, or "saver" (one still frame, redrawn only when
 * the data really changes). The decision is pure ([lowBattery], [saver]) so it can be tested; [Watcher] feeds it.
 */
object WallpaperPowerGate {
    const val LOW_ENTER = 15   // % at or under which (and not charging) the saver kicks in
    const val LOW_EXIT = 20    // % over which it leaves again: the gap keeps it from flickering around 15 %

    /** Not charging and at/under 15 %; once low it stays low until the battery is over 20 % or plugged in. [pct] < 0 = unknown. */
    fun lowBattery(pct: Int, charging: Boolean, wasLow: Boolean): Boolean = when {
        pct < 0 || charging -> false
        wasLow -> pct <= LOW_EXIT
        else -> pct <= LOW_ENTER
    }

    fun saver(policy: SaverPolicy, powerSave: Boolean, lowBattery: Boolean): Boolean = when (policy) {
        SaverPolicy.ALWAYS -> true
        SaverPolicy.NEVER -> false
        SaverPolicy.AUTO -> powerSave || lowBattery
    }

    /** A one-off answer (no memory of the hysteresis), for things outside a wallpaper such as the lyrics download. */
    fun saverNow(context: Context): Boolean {
        val s = Snapshot.read(context)
        return saver(SaverPolicy.of(with(PhoneSettings) { context.wallpaperSaver }), s.powerSave, lowBattery(s.pct, s.charging, false))
    }

    private class Snapshot(val powerSave: Boolean, val pct: Int, val charging: Boolean) {
        companion object {
            fun read(context: Context): Snapshot {
                val app = context.applicationContext
                val ps = (app.getSystemService(Context.POWER_SERVICE) as? PowerManager)?.isPowerSaveMode == true
                val b = app.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
                val level = b?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
                val scale = b?.getIntExtra(BatteryManager.EXTRA_SCALE, 100) ?: 100
                val status = b?.getIntExtra(BatteryManager.EXTRA_STATUS, -1) ?: -1
                return Snapshot(
                    ps, if (level >= 0) level * 100 / scale.coerceAtLeast(1) else -1,
                    status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL,
                )
            }
        }
    }

    /**
     * Follows the phone's state for one wallpaper engine: [start] in `onCreate`, [stop] in `onDestroy`. [onChange] gets the
     * new answer, only when it differs from the last one.
     */
    class Watcher(private val context: Context, private val onChange: (Boolean) -> Unit) {
        var saver = false
            private set
        private var lowBat = false
        private var started = false

        private val receiver = object : BroadcastReceiver() {
            override fun onReceive(c: Context, i: Intent) = evaluate()
        }
        // SharedPreferences keeps its listeners weakly: this field is what holds it
        private val prefsListener = SharedPreferences.OnSharedPreferenceChangeListener { _, _ -> evaluate() }
        private val prefs: SharedPreferences get() = context.applicationContext.getSharedPreferences("coucou_phone", Context.MODE_PRIVATE)

        fun start() {
            if (started) return
            started = true
            val f = IntentFilter().apply {
                addAction(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED)
                addAction(Intent.ACTION_BATTERY_CHANGED)
            }
            ContextCompat.registerReceiver(context, receiver, f, ContextCompat.RECEIVER_NOT_EXPORTED)
            prefs.registerOnSharedPreferenceChangeListener(prefsListener)
            saver = compute()   // the first answer is silent: the engine asks [saver] before its first frame
        }

        fun stop() {
            if (!started) return
            started = false
            runCatching { context.unregisterReceiver(receiver) }
            prefs.unregisterOnSharedPreferenceChangeListener(prefsListener)
        }

        private fun compute(): Boolean {
            val s = Snapshot.read(context)
            lowBat = lowBattery(s.pct, s.charging, lowBat)
            return saver(SaverPolicy.of(with(PhoneSettings) { context.wallpaperSaver }), s.powerSave, lowBat)
        }

        private fun evaluate() {
            val now = compute()
            if (now != saver) { saver = now; onChange(now) }
        }
    }
}
