package com.agupta07505.smartisland.coucou.assist

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LocalActionsTest {
    private fun p(s: String) = LocalActions.parse(s)

    @Test fun opensAnApp() {
        assertEquals(LocalAction.OpenApp("whatsapp"), p("Abre WhatsApp"))
        assertEquals(LocalAction.OpenApp("spotify"), p("oye jinx, abre la app de Spotify"))
        assertEquals(LocalAction.OpenApp("camara"), p("ábreme la cámara"))
        assertEquals(LocalAction.OpenApp("youtube"), p("open YouTube"))
    }

    @Test fun opensSettingsPages() {
        assertEquals(LocalAction.OpenSettings(SettingsPage.Wifi), p("abre los ajustes de wifi"))
        assertEquals(LocalAction.OpenSettings(SettingsPage.Bluetooth), p("abre bluetooth"))
        assertEquals(LocalAction.OpenSettings(SettingsPage.General), p("abre los ajustes"))
        assertEquals(LocalAction.OpenSettings(SettingsPage.Battery), p("abre ajustes de batería"))
    }

    @Test fun searchesAndNavigates() {
        assertEquals(LocalAction.WebSearch("recetas de pozole"), p("busca en internet recetas de pozole"))
        assertEquals(LocalAction.Navigate("la central de abastos"), p("llévame a la central de abastos"))
    }

    @Test fun dialsOnlyNumbers() {
        assertEquals(LocalAction.Dial("5512345678"), p("llama al 55 1234 5678"))
        assertNull(p("llama a mamá"))
    }

    @Test fun leavesTheRestToTheModel() {
        assertNull(p("¿qué clima hace hoy?"))
        assertNull(p("abre youtube y busca gatos graciosos en la tele"))
        assertNull(p(""))
        assertNull(p("a".repeat(300)))
    }

    private val apps = listOf(
        AppEntry("WhatsApp", "com.whatsapp"), AppEntry("WhatsApp Business", "com.whatsapp.w4b"),
        AppEntry("Cámara", "cam"), AppEntry("Spotify", "com.spotify"), AppEntry("YouTube", "yt"), AppEntry("YouTube Music", "ytm"),
        AppEntry("Ajustes", "settings"), AppEntry("Calculadora", "calc"),
    )

    @Test fun matchesNamesAndNicknames() {
        assertEquals("com.whatsapp", AppMatcher.best("whatsapp", apps)?.packageName)
        assertEquals("com.whatsapp", AppMatcher.best("wasap", apps)?.packageName)
        assertEquals("cam", AppMatcher.best("camara", apps)?.packageName)
        assertEquals("yt", AppMatcher.best("youtube", apps)?.packageName)
        assertEquals("calc", AppMatcher.best("calculadora", apps)?.packageName)
        assertNull(AppMatcher.best("una alarma rarisima", apps))
    }
}
