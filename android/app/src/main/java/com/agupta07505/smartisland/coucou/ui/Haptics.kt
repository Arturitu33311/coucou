/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import android.content.Context
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

/**
 * Fine haptics, Apple-style: barely-there ticks for small wins (a task done, an important arrival)
 * and a firmer click for physical metaphors (the padlock). Best-effort everywhere — it buzzes or
 * it doesn't, but it never crashes and never asks. VIBRATE is a declared normal permission.
 */
object Haptics {
    /** A light tick: finishing a task, an important arrival taking the island. */
    fun tap(context: Context) = buzz(context, 30L)

    /** A firmer click: the padlock's shackle letting go. */
    fun click(context: Context) = buzz(context, 60L)

    private fun buzz(context: Context, ms: Long) {
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                val vm = context.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager
                vm?.defaultVibrator?.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE))
                return
            }
            @Suppress("DEPRECATION")
            val vibrator = context.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                vibrator?.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE))
            } else {
                @Suppress("DEPRECATION")
                vibrator?.vibrate(ms)
            }
        }
    }
}
