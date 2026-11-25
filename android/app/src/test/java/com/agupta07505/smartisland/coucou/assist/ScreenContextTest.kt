package com.agupta07505.smartisland.coucou.assist

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ScreenContextTest {
    @Test fun readsVisibleTextInOrderWithoutPasswords() {
        val root = ScreenNode(children = listOf(
            ScreenNode(text = "Bandeja de entrada"),
            ScreenNode(text = "Responder", clickable = true),
            ScreenNode(text = "hunter2", password = true),
            ScreenNode(text = "oculto", visible = false),
            ScreenNode(description = "Foto de perfil"),
            ScreenNode(text = "Bandeja de entrada"),
        ))
        assertEquals("Bandeja de entrada\n[Responder]\nFoto de perfil", ScreenText.format(listOf(root)))
    }

    @Test fun cutsLongScreens() {
        val many = ScreenNode(children = (1..2000).map { ScreenNode(text = "línea número $it") })
        val out = ScreenText.format(listOf(many), maxChars = 200)
        assertTrue(out.length <= 201)
        assertTrue(out.endsWith("…"))
    }

    @Test fun neverReadsMoneyOrPasswordApps() {
        assertTrue(SensitiveApps.isSensitive("com.nu.production"))
        assertTrue(SensitiveApps.isSensitive("mx.com.bbva.bancomermovil"))
        assertTrue(SensitiveApps.isSensitive("com.bitwarden.mobile"))
        assertTrue(SensitiveApps.isSensitive("com.google.android.apps.authenticator2"))
        assertFalse(SensitiveApps.isSensitive("com.whatsapp"))
        assertFalse(SensitiveApps.isSensitive("com.android.chrome"))
    }

    @Test fun snapshotsExpire() {
        val s = ScreenSnapshot("p", "App", "texto", takenAtMs = 1_000)
        assertTrue(s.isFresh(1_000 + ScreenSnapshot.TTL_MS))
        assertFalse(s.isFresh(1_001 + ScreenSnapshot.TTL_MS))
    }

    @Test fun theModelIsToldItIsTheScreenNotAMessage() {
        val out = ScreenText.forModel(ScreenSnapshot("p", "Gmail", "Hola Alan", 0), "¿qué dice?")
        assertTrue(out.contains("no algo que escribió"))
        assertTrue(out.contains("Hola Alan"))
        assertTrue(out.endsWith("¿qué dice?"))
    }
}
