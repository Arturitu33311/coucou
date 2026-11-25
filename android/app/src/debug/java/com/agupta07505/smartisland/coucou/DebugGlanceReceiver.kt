/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import com.agupta07505.smartisland.BuildConfig
import com.agupta07505.smartisland.coucou.ui.GlanceBus
import com.agupta07505.smartisland.coucou.ui.GlanceKind
import com.agupta07505.smartisland.coucou.ui.GlanceMode
import com.agupta07505.smartisland.coucou.ui.GlanceSettings
import com.agupta07505.smartisland.coucou.ui.GlanceStyle

/**
 * Debug builds only (this file lives in src/debug: it does not exist in release). Drives the unlock pill from adb:
 * `adb shell am broadcast -n com.agupta07505.smartisland/.coucou.DebugGlanceReceiver --es state scan|success|failure
 *  [--es mode off|always|face] [--es style mochi|ring]`. With only `--es mode`/`--es style` it just sets the switch.
 */
class DebugGlanceReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (!BuildConfig.DEBUG) return
        intent.getStringExtra("mode")?.let { k -> GlanceSettings.setMode(context, GlanceMode.entries.first { it.key == k }) }
        intent.getStringExtra("style")?.let { k -> GlanceSettings.setStyle(context, GlanceStyle.entries.first { it.key == k }) }
        val kind = when (intent.getStringExtra("state")) {
            "scan" -> GlanceKind.SCAN
            "success" -> GlanceKind.SUCCESS
            "failure" -> GlanceKind.FAILURE
            else -> return
        }
        GlanceBus.show(kind)
    }
}
