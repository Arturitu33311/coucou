/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 *
 * The unlock animation takes its look from "Glance" by Jonny Oo (https://github.com/jonnyoo/glance, MIT License,
 * Copyright (c) Jonny Oo): the pill that widens on unlock, the ring that closes, the check that draws itself, the
 * red shake on failure, the spring numbers and the 1.7 s hold. Only the idea and the numbers were studied; the code
 * is written from scratch for Compose. See NOTICE-glance.md next to the Gradle project.
 */

package com.agupta07505.smartisland.coucou.ui

import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.provider.Settings
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.sin
import kotlin.math.sqrt

/** What the unlock pill is showing. [SCAN] only exists for debugging: a real unlock is already over when we hear of it. */
enum class GlanceKind { SCAN, SUCCESS, FAILURE }

/** The user's switch. Stored as [key]. */
enum class GlanceMode(val key: String) {
    OFF("off"),
    ALWAYS("always"),

    /** Only on phones that report a configured face unlock (see [GlanceUnlockLogic.faceConfigured]). */
    FACE_PHONES("face");

    companion object {
        fun from(key: String?): GlanceMode = entries.firstOrNull { it.key == key } ?: ALWAYS
    }
}

/** Who stars on the left flank: Jinx's Mochi, or Glance's own padlock. */
enum class GlanceStyle(val key: String) {
    MOCHI("mochi"),
    RING("ring");

    companion object {
        fun from(key: String?): GlanceStyle = entries.firstOrNull { it.key == key } ?: MOCHI
    }
}

/**
 * Pure rules and curves. Android gives an app no way to know HOW the phone was unlocked (PIN, fingerprint or face),
 * so nothing here claims "recognised": the pill says "unlocked", whatever opened it.
 */
object GlanceUnlockLogic {
    /** USER_PRESENT and SCREEN_ON can both describe the same unlock; one pill per this window. */
    const val DEBOUNCE_MS = 1_500L

    const val HOLD_END_MS = 1_900L
    const val SUCCESS_TOTAL_MS = 2_500L
    const val FAILURE_TOTAL_MS = 3_100L
    const val SCAN_TOTAL_MS = 6_000L

    /** Reduced motion starts the clock here, i.e. already at the final frame (spring settled, ring closed, check drawn). */
    const val REDUCED_START_MS = 1_000L
    private const val REDUCED_FADE_MS = 160L

    fun totalMs(kind: GlanceKind, reduced: Boolean): Long {
        val full = when (kind) {
            GlanceKind.SCAN -> SCAN_TOTAL_MS
            GlanceKind.SUCCESS -> SUCCESS_TOTAL_MS
            GlanceKind.FAILURE -> FAILURE_TOTAL_MS
        }
        return if (reduced) full - REDUCED_START_MS else full
    }

    fun holdEndMs(kind: GlanceKind): Long = when (kind) {
        GlanceKind.SCAN -> SCAN_TOTAL_MS - 1_000L
        GlanceKind.SUCCESS -> HOLD_END_MS
        GlanceKind.FAILURE -> 2_500L
    }

    /**
     * May the unlock pill own this unlock at all (before the anti-double window)? The island and Mochi's bubble both ask,
     * so exactly one of them reacts: when the pill owns the unlock in [GlanceStyle.MOCHI] style it carries Mochi, and the
     * bubble must not greet as well.
     */
    fun owns(mode: GlanceMode, deviceSecure: Boolean, faceConfigured: Boolean): Boolean = when {
        mode == GlanceMode.OFF -> false
        // No lock set => nothing was "unlocked"; every screen-on would look like one.
        !deviceSecure -> false
        mode == GlanceMode.FACE_PHONES -> faceConfigured
        else -> true
    }

    /** The bubble's own hello stays quiet when the pill (with Mochi in it) already says it. */
    fun bubbleStaysQuiet(mode: GlanceMode, style: GlanceStyle, deviceSecure: Boolean, faceConfigured: Boolean): Boolean =
        style == GlanceStyle.MOCHI && owns(mode, deviceSecure, faceConfigured)

    /** Is this unlock worth a pill? [lastShownMs] is when the previous one started (0 = never). */
    fun shouldShow(
        mode: GlanceMode,
        islandOn: Boolean,
        deviceSecure: Boolean,
        faceConfigured: Boolean,
        nowMs: Long,
        lastShownMs: Long,
    ): Boolean {
        if (!islandOn || !owns(mode, deviceSecure, faceConfigured)) return false
        return lastShownMs == 0L || nowMs - lastShownMs >= DEBOUNCE_MS
    }

    /** What Mochi feels along the clock: he searches (eyes on the ring), then is glad, or is startled and then sad. */
    fun mochiMood(kind: GlanceKind, tMs: Long): MochiMood = when (kind) {
        GlanceKind.SCAN -> MochiMood.Searching
        GlanceKind.SUCCESS -> if (tMs < 450) MochiMood.Glance else MochiMood.Happy
        GlanceKind.FAILURE -> when {
            tMs < 450 -> MochiMood.Glance
            tMs < 1_150 -> MochiMood.Scared
            else -> MochiMood.Sad
        }
    }

    /**
     * The one honest heuristic for "this phone unlocks with a face": the hardware/feature says so, the keyguard flag is
     * not switched off, and some biometric is enrolled. It cannot say that THIS unlock was the face.
     */
    fun faceConfigured(featureFace: Boolean, keyguardFlag: Int, biometricEnrolled: Boolean): Boolean =
        featureFace && keyguardFlag != 0 && biometricEnrolled

    // --- curves (all functions of the elapsed time, so a frame never depends on the previous one) ---

    /** Glance's panel spring: response 0.45 s, damping 0.7 (it overshoots a hair). */
    private const val RESPONSE_S = 0.45
    private const val DAMPING = 0.7

    fun openSpring(tMs: Long): Float {
        if (tMs <= 0) return 0f
        val t = tMs / 1000.0
        val w = 2 * PI / RESPONSE_S
        val wd = w * sqrt(1 - DAMPING * DAMPING)
        val v = 1 - exp(-DAMPING * w * t) * (cos(wd * t) + DAMPING * w / wd * sin(wd * t))
        return v.toFloat()
    }

    /** Glance's close: critically damped, no bounce. 0 -> 1. */
    fun closeSpring(tMs: Long): Float {
        if (tMs <= 0) return 0f
        val t = tMs / 1000.0
        val w = 2 * PI / RESPONSE_S
        return (1 - (1 + w * t) * exp(-w * t)).toFloat()
    }

    /** 0 closed .. 1 fully open (a little above 1 while the spring overshoots). */
    fun pillOpen(kind: GlanceKind, tMs: Long): Float {
        val end = holdEndMs(kind)
        return if (tMs < end) openSpring(tMs) else openSpring(end) * (1f - closeSpring(tMs - end))
    }

    /** Glance's check/ring easing: cubic-bezier(0.65, 0, 0.35, 1). */
    fun ease(x: Float): Float {
        val p = x.coerceIn(0f, 1f)
        // Solve bezier x(s)=p by bisection; 20 steps is far below a pixel.
        var lo = 0f
        var hi = 1f
        repeat(20) {
            val s = (lo + hi) / 2
            val bx = 3 * (1 - s) * (1 - s) * s * 0.65f + 3 * (1 - s) * s * s * 0.35f + s * s * s
            if (bx < p) lo = s else hi = s
        }
        val s = (lo + hi) / 2
        return 3 * (1 - s) * s * s * 1f + s * s * s
    }

    private fun span(t: Long, from: Long, len: Long): Float = ((t - from).toFloat() / len).coerceIn(0f, 1f)

    /** How much of the ring is drawn, 0..1 (SCAN keeps a quarter arc going round). */
    fun ringSweep(kind: GlanceKind, tMs: Long): Float = when (kind) {
        GlanceKind.SCAN -> 0.28f
        else -> 0.12f + 0.88f * ease(span(tMs, 150, 500))
    }

    /** Degrees the ring's start has turned: it spins while it searches and settles as it closes. */
    fun ringRotation(kind: GlanceKind, tMs: Long): Float = when (kind) {
        GlanceKind.SCAN -> (tMs % 900L) / 900f * 360f
        else -> 360f * (1f - ease(span(tMs, 0, 650))) * 0.5f
    }

    /** The tick (or the cross on failure) drawing itself: 0..1, 0.28 s as in Glance. */
    fun markProgress(kind: GlanceKind, tMs: Long): Float = when (kind) {
        GlanceKind.SCAN -> 0f
        GlanceKind.SUCCESS -> ease(span(tMs, 650, 280))
        GlanceKind.FAILURE -> ease(span(tMs, 650, 280))
    }

    /** Shackle: 0 closed .. 1 open (0.4 s, starts as the ring closes). Failure never opens it. */
    fun lockOpen(kind: GlanceKind, tMs: Long): Float =
        if (kind == GlanceKind.SUCCESS) ease(span(tMs, 450, 400)) else 0f

    /** Sideways wobble in dp for the failure (damped, three swings). */
    fun shakeDp(kind: GlanceKind, tMs: Long): Float {
        if (kind != GlanceKind.FAILURE || tMs < 650 || tMs > 1_150) return 0f
        val x = (tMs - 650) / 500f
        return (4f * (1 - x) * sin(x * 3 * 2 * PI)).toFloat()
    }

    /**
     * Whole-pill opacity: a short fade in (so the island's own content does not vanish with a pop) and a fade out over
     * the last 250 ms (so it hands the island back without a jump). Reduced motion fades a little slower and never widens.
     */
    fun pillAlpha(kind: GlanceKind, tMs: Long, reduced: Boolean): Float {
        val total = totalMs(kind, reduced)
        val inMs = if (reduced) 160f else 80f
        val outMs = if (reduced) 160f else 250f
        return minOf((tMs / inMs).coerceIn(0f, 1f), ((total - tMs) / outMs).coerceIn(0f, 1f))
    }

    /** Reads the phone for [faceConfigured]; best effort, false when anything is unavailable (or Android < 11). */
    fun faceConfigured(context: Context): Boolean = runCatching {
        if (android.os.Build.VERSION.SDK_INT < 30) return false
        val pm = context.packageManager
        val feature = pm.hasSystemFeature("android.hardware.biometrics.face") || pm.hasSystemFeature("com.samsung.android.bio.face")
        val flag = Settings.Secure.getInt(context.contentResolver, "face_unlock_keyguard_enabled", -1)
        val bm = context.getSystemService(BiometricManager::class.java)
        val enrolled = bm != null &&
            bm.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_WEAK) == BiometricManager.BIOMETRIC_SUCCESS
        faceConfigured(feature, flag, enrolled)
    }.getOrDefault(false)
}

/** The switch, kept in its own tiny preferences file (like the other Coucou toggles). */
object GlanceSettings {
    private const val FILE = "coucou_glance"
    private const val KEY = "mode"

    private fun prefs(c: Context) = c.applicationContext.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    fun mode(c: Context): GlanceMode = GlanceMode.from(prefs(c).getString(KEY, null))

    fun style(c: Context): GlanceStyle = GlanceStyle.from(prefs(c).getString("style", null))

    fun setStyle(c: Context, style: GlanceStyle) {
        prefs(c).edit().putString("style", style.key).apply()
    }

    fun setMode(c: Context, mode: GlanceMode) {
        prefs(c).edit().putString(KEY, mode.key).apply()
    }

    fun listen(c: Context, onChange: () -> Unit): () -> Unit {
        val p = prefs(c)
        val l = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, k -> if (k == KEY || k == "style") onChange() }
        p.registerOnSharedPreferenceChangeListener(l)
        return { p.unregisterOnSharedPreferenceChangeListener(l) }
    }
}

/** Anything that wants the pill on screen right now (the debug receiver does; real unlocks go through [rememberUnlockMs]). */
object GlanceBus {
    data class Request(val id: Long, val kind: GlanceKind)

    private val _requests = kotlinx.coroutines.flow.MutableStateFlow<Request?>(null)
    val requests: kotlinx.coroutines.flow.StateFlow<Request?> = _requests

    fun show(kind: GlanceKind) {
        _requests.value = Request(System.nanoTime(), kind)
    }
}
