package com.agupta07505.smartisland.coucou

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class WallpaperPowerGateTest {
    private val g = WallpaperPowerGate

    @Test
    fun auto_follows_android_battery_saver() {
        assertTrue(g.saver(SaverPolicy.AUTO, powerSave = true, lowBattery = false))
        assertFalse(g.saver(SaverPolicy.AUTO, powerSave = false, lowBattery = false))
    }

    @Test
    fun auto_follows_a_nearly_flat_battery() {
        assertTrue(g.saver(SaverPolicy.AUTO, powerSave = false, lowBattery = true))
    }

    @Test
    fun the_switch_overrides_everything() {
        assertTrue(g.saver(SaverPolicy.ALWAYS, powerSave = false, lowBattery = false))
        assertFalse(g.saver(SaverPolicy.NEVER, powerSave = true, lowBattery = true))
    }

    @Test
    fun low_battery_needs_15_or_less_and_no_charger() {
        assertTrue(g.lowBattery(15, charging = false, wasLow = false))
        assertFalse(g.lowBattery(16, charging = false, wasLow = false))
        assertFalse(g.lowBattery(5, charging = true, wasLow = true))
        assertFalse(g.lowBattery(-1, charging = false, wasLow = false))
    }

    @Test
    fun low_battery_does_not_flicker_between_15_and_20() {
        var low = false
        val seen = mutableListOf<Boolean>()
        for (pct in listOf(17, 16, 15, 16, 15, 17, 19, 20, 21, 20, 17)) {
            low = g.lowBattery(pct, charging = false, wasLow = low); seen += low
        }
        // enters at 15, stays through 16..20, leaves at 21, and 17 again does not re-enter
        assertEquals(listOf(false, false, true, true, true, true, true, true, false, false, false), seen)
    }

    @Test
    fun policy_parses_ids_and_defaults_to_auto() {
        assertEquals(SaverPolicy.ALWAYS, SaverPolicy.of("always"))
        assertEquals(SaverPolicy.NEVER, SaverPolicy.of("NEVER"))
        assertEquals(SaverPolicy.AUTO, SaverPolicy.of(null))
        assertEquals(SaverPolicy.AUTO, SaverPolicy.of("whatever"))
    }
}
