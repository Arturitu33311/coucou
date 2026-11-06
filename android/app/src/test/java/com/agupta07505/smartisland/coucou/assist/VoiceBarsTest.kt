package com.agupta07505.smartisland.coucou.assist

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class VoiceBarsTest {
    @Test fun idleIsFlat() {
        for (i in 0 until VOICE_BANDS) assertEquals(0f, barGoal(VoicePhase.Idle, i, 12_345, 0.9f), 0f)
    }

    @Test fun listeningFollowsTheVoice() {
        val quiet = (0 until VOICE_BANDS).maxOf { barGoal(VoicePhase.Listening, it, 1_000, 0f) }
        val loud = (0 until VOICE_BANDS).maxOf { barGoal(VoicePhase.Listening, it, 1_000, 0.9f) }
        assertTrue(quiet <= 0.16f + 1e-4f)
        assertTrue(loud > 0.4f)
    }

    @Test fun thinkingAndSpeakingStayInRange() {
        for (t in 0L..5_000L step 37) for (i in 0 until VOICE_BANDS) {
            assertTrue(barGoal(VoicePhase.Thinking, i, t, 0f) in 0f..0.5f)
            assertTrue(barGoal(VoicePhase.Speaking, i, t, 0f) in 0.2f..0.55f)
        }
    }

    @Test fun easingMovesTowardTheGoalWithoutOvershoot() {
        var v = 0f
        repeat(120) { v = easeBar(v, 0.8f, 0.016f) }
        assertEquals(0.8f, v, 0.01f)
        assertEquals(0.8f, easeBar(0f, 0.8f, 1f), 1e-6f)
    }

    @Test fun recognizerLevelIsScaledToZeroOne() {
        assertEquals(0f, VoiceInput.levelOf(-2f), 0f)
        assertEquals(1f, VoiceInput.levelOf(10f), 0f)
        assertEquals(0f, VoiceInput.levelOf(-30f), 0f)
        assertEquals(1f, VoiceInput.levelOf(40f), 0f)
    }

    @Test fun spokenFormDropsMarkdownAndLinks() {
        assertEquals("Hola Alan mira esto y lo otro", Speaker.spokenForm("**Hola** Alan, mira `esto`  https://x.co/abc y lo otro".replace(",", "")))
        assertEquals(600, Speaker.spokenForm("a".repeat(1000)).length)
    }
}
