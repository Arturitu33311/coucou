package com.agupta07505.smartisland.coucou.audiolink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SwapAskTest {
    @Test
    fun one_is_release_and_two_is_connect() {
        assertEquals(SwapAsk.Release, swapAskOf(byteArrayOf(1)))
        assertEquals(SwapAsk.Connect, swapAskOf(byteArrayOf(2)))
    }

    @Test
    fun anything_else_is_nothing() {
        assertNull(swapAskOf(byteArrayOf()))
        assertNull(swapAskOf(byteArrayOf(0)))
        assertNull(swapAskOf(byteArrayOf(3)))
        assertNull(swapAskOf(byteArrayOf(1, 1)))
    }
}
