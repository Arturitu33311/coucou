package com.agupta07505.smartisland.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class IslandGestureLogicTest {
    @Test
    fun hold_follows_the_system_delay_but_never_below_300() {
        assertEquals(400L, IslandGestureLogic.holdThresholdMs(400))
        assertEquals(300L, IslandGestureLogic.holdThresholdMs(100))
        assertEquals(1500L, IslandGestureLogic.holdThresholdMs(1500))
    }

    @Test
    fun finger_jitter_is_not_a_drag() {
        val slop = 24f // ~8 dp at 3x
        assertFalse(IslandGestureLogic.crossedSlop(0.6f, 0.4f, slop))
        assertFalse(IslandGestureLogic.crossedSlop(15f, 15f, slop)) // 21 px diagonal
        assertTrue(IslandGestureLogic.crossedSlop(20f, 20f, slop))  // 28 px diagonal
        assertTrue(IslandGestureLogic.crossedSlop(0f, -30f, slop))
    }

    @Test
    fun a_still_hold_is_a_slow_tap_but_a_hold_swipe_is_not() {
        assertTrue(IslandGestureLogic.releaseIsTap(holdRegistered = true, dragging = false, handledInside = false))
        assertFalse(IslandGestureLogic.releaseIsTap(holdRegistered = true, dragging = true, handledInside = false))
        assertTrue(IslandGestureLogic.releaseIsTap(holdRegistered = false, dragging = true, handledInside = false))
    }

    @Test
    fun a_press_a_child_button_handled_never_also_taps_the_island() {
        assertFalse(IslandGestureLogic.releaseIsTap(holdRegistered = false, dragging = false, handledInside = true))
        assertFalse(IslandGestureLogic.releaseIsTap(holdRegistered = true, dragging = false, handledInside = true))
    }

    @Test
    fun collapsed_touch_region_is_at_least_a_fingertip_tall() {
        assertEquals(44f, IslandGestureLogic.collapsedTouchHeightDp(25f), 0.001f)  // 25 + 16 = 41 → 44
        assertEquals(56f, IslandGestureLogic.collapsedTouchHeightDp(40f), 0.001f)  // taller pill keeps its margin
    }
}
