package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.ui.GlanceKind
import com.agupta07505.smartisland.coucou.ui.GlanceMode
import com.agupta07505.smartisland.coucou.ui.GlanceStyle
import com.agupta07505.smartisland.coucou.ui.GlanceUnlockLogic as L
import com.agupta07505.smartisland.coucou.ui.MochiMood
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GlanceUnlockTest {
    private fun show(mode: GlanceMode, on: Boolean = true, secure: Boolean = true, face: Boolean = false, now: Long = 10_000, last: Long = 0) =
        L.shouldShow(mode, on, secure, face, now, last)

    @Test fun offNeverShows() = assertFalse(show(GlanceMode.OFF))

    @Test fun alwaysShowsOnASecurePhone() = assertTrue(show(GlanceMode.ALWAYS))

    @Test fun noScreenLockMeansNoUnlockToCelebrate() = assertFalse(show(GlanceMode.ALWAYS, secure = false))

    @Test fun hiddenIslandShowsNothing() = assertFalse(show(GlanceMode.ALWAYS, on = false))

    @Test fun facePhonesNeedAConfiguredFace() {
        assertFalse(show(GlanceMode.FACE_PHONES, face = false))
        assertTrue(show(GlanceMode.FACE_PHONES, face = true))
    }

    @Test fun userPresentAndScreenOnForTheSameUnlockShowOnce() {
        assertFalse(show(GlanceMode.ALWAYS, now = 10_400, last = 10_000))
        assertTrue(show(GlanceMode.ALWAYS, now = 11_600, last = 10_000))
    }

    @Test fun faceHeuristicNeedsAllThree() {
        assertTrue(L.faceConfigured(true, -1, true))
        assertTrue(L.faceConfigured(true, 1, true))
        assertFalse(L.faceConfigured(true, 0, true))
        assertFalse(L.faceConfigured(false, 1, true))
        assertFalse(L.faceConfigured(true, 1, false))
    }

    @Test fun bubbleStaysQuietOnlyWhenThePillWithMochiOwnsTheUnlock() {
        assertTrue(L.bubbleStaysQuiet(GlanceMode.ALWAYS, GlanceStyle.MOCHI, true, false))
        assertFalse(L.bubbleStaysQuiet(GlanceMode.ALWAYS, GlanceStyle.RING, true, false))
        assertFalse(L.bubbleStaysQuiet(GlanceMode.OFF, GlanceStyle.MOCHI, true, false))
        assertFalse(L.bubbleStaysQuiet(GlanceMode.ALWAYS, GlanceStyle.MOCHI, false, false))
        assertFalse(L.bubbleStaysQuiet(GlanceMode.FACE_PHONES, GlanceStyle.MOCHI, true, false))
    }

    @Test fun modeKeysRoundTripAndDefaultToAlways() {
        GlanceMode.entries.forEach { assertEquals(it, GlanceMode.from(it.key)) }
        assertEquals(GlanceMode.ALWAYS, GlanceMode.from(null))
        assertEquals(GlanceStyle.MOCHI, GlanceStyle.from("nonsense"))
    }

    @Test fun pillOpensWithASmallOvershootAndFoldsBackToNothing() {
        assertEquals(0f, L.pillOpen(GlanceKind.SUCCESS, 0), 0.001f)
        val peak = (0..900 step 10).maxOf { L.pillOpen(GlanceKind.SUCCESS, it.toLong()) }
        assertTrue(peak > 1.0f && peak < 1.12f)
        assertEquals(1f, L.pillOpen(GlanceKind.SUCCESS, 1_800), 0.01f)
        assertEquals(0f, L.pillOpen(GlanceKind.SUCCESS, L.SUCCESS_TOTAL_MS), 0.01f)
    }

    @Test fun markDrawsLeftToRightAndIsDoneBeforeTheHoldEnds() {
        assertEquals(0f, L.markProgress(GlanceKind.SUCCESS, 600), 0.001f)
        assertEquals(1f, L.markProgress(GlanceKind.SUCCESS, 1_000), 0.001f)
        assertTrue(L.markProgress(GlanceKind.SUCCESS, 800) in 0.2f..0.8f)
        assertEquals(0f, L.markProgress(GlanceKind.SCAN, 2_000), 0.001f)
    }

    @Test fun ringClosesAndFailureNeverOpensTheLock() {
        assertEquals(1f, L.ringSweep(GlanceKind.SUCCESS, 800), 0.001f)
        assertEquals(0f, L.lockOpen(GlanceKind.FAILURE, 2_000), 0f)
        assertEquals(1f, L.lockOpen(GlanceKind.SUCCESS, 1_000), 0.001f)
    }

    @Test fun onlyFailureShakesAndItSettles() {
        assertEquals(0f, L.shakeDp(GlanceKind.SUCCESS, 800), 0f)
        assertTrue((650..1_150 step 10).any { L.shakeDp(GlanceKind.FAILURE, it.toLong()) != 0f })
        assertEquals(0f, L.shakeDp(GlanceKind.FAILURE, 1_300), 0f)
    }

    @Test fun reducedMotionStartsAtTheFinalFrameAndOnlyFades() {
        assertEquals(L.SUCCESS_TOTAL_MS - L.REDUCED_START_MS, L.totalMs(GlanceKind.SUCCESS, true))
        assertEquals(0f, L.pillAlpha(GlanceKind.SUCCESS, 0, true), 0.001f)
        assertEquals(1f, L.pillAlpha(GlanceKind.SUCCESS, 500, true), 0.001f)
        assertEquals(1f, L.pillAlpha(GlanceKind.SUCCESS, 1_000, false), 0.001f)
        assertEquals(0f, L.pillAlpha(GlanceKind.SUCCESS, L.SUCCESS_TOTAL_MS, false), 0.001f)
        assertEquals(1f, L.markProgress(GlanceKind.SUCCESS, L.REDUCED_START_MS), 0.001f)
    }

    @Test fun mochiSearchesThenIsGladOrStartledThenSad() {
        assertEquals(MochiMood.Searching, L.mochiMood(GlanceKind.SCAN, 3_000))
        assertEquals(MochiMood.Glance, L.mochiMood(GlanceKind.SUCCESS, 100))
        assertEquals(MochiMood.Happy, L.mochiMood(GlanceKind.SUCCESS, 900))
        assertEquals(MochiMood.Scared, L.mochiMood(GlanceKind.FAILURE, 800))
        assertEquals(MochiMood.Sad, L.mochiMood(GlanceKind.FAILURE, 2_000))
    }
}
