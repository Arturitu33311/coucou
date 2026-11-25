/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import java.text.Normalizer

/**
 * What Alan can ask Jinx to do on this phone without any model: open an app, a settings page, search the web,
 * navigate, dial. Understood here, on the phone, so it works offline and answers at once (like "ok Google, abre…").
 */
sealed interface LocalAction {
    data class OpenApp(val name: String) : LocalAction
    data class OpenSettings(val page: SettingsPage) : LocalAction
    data class WebSearch(val query: String) : LocalAction
    data class Navigate(val place: String) : LocalAction
    data class Dial(val number: String) : LocalAction
}

enum class SettingsPage { General, Wifi, Bluetooth, Display, Battery, Notifications, Accessibility, Sound, Location, Apps, Data, DoNotDisturb, Storage }

internal fun fold(s: String): String =
    Normalizer.normalize(s.lowercase().trim(), Normalizer.Form.NFD).replace(Regex("\\p{Mn}+"), "")

object LocalActions {
    private val FILLER = Regex("^(jinx|oye|hey|ok|por favor|porfa|puedes|podrias|me puedes|me podrias|quiero que|please|can you|could you)[ ,]+")
    private val OPEN = Regex("^(abre|abrir|abreme|lanza|inicia|ejecuta|open|launch|start)\\s+(.+)$")
    private val ARTICLES = Regex("^(la aplicacion de |la app de |la aplicacion |la app |app |aplicacion |el |la |los |las |mi |the )")
    private val SEARCH = Regex("^(busca|buscar|buscame|googlea|investiga|search for|search|google)\\s+(en internet |en google |en la web |online )?(.+)$")
    private val NAV = Regex("^(llevame|navega|navegame|como llego|como llegar|ruta|rutas|navigate to|directions to|take me)\\s+(a |al |hacia |hasta |to )?(.+)$")
    private val DIAL = Regex("^(llama|llamar|marca|marcar|call|dial)\\s+(al |a |el |to )?(.+)$")

    private val SETTINGS_WORDS = listOf(
        SettingsPage.Wifi to listOf("wifi", "wi-fi", "wi fi", "internet inalambrico"),
        SettingsPage.Bluetooth to listOf("bluetooth", "blutu"),
        SettingsPage.Display to listOf("brillo", "pantalla", "display", "fondo de pantalla"),
        SettingsPage.Battery to listOf("bateria", "ahorro de energia", "battery"),
        SettingsPage.Notifications to listOf("notificaciones", "notifications"),
        SettingsPage.Accessibility to listOf("accesibilidad", "accessibility"),
        SettingsPage.Sound to listOf("sonido", "volumen", "sound", "volume"),
        SettingsPage.Location to listOf("ubicacion", "gps", "location"),
        SettingsPage.Data to listOf("datos moviles", "datos celulares", "mobile data"),
        SettingsPage.DoNotDisturb to listOf("no molestar", "do not disturb"),
        SettingsPage.Storage to listOf("almacenamiento", "storage"),
        SettingsPage.Apps to listOf("aplicaciones", "apps instaladas"),
    )

    /** The action a sentence asks for, or null when it is for a model to answer. */
    fun parse(raw: String): LocalAction? {
        var t = fold(raw).trimEnd('.', '!', '?', ' ')
        if (t.isEmpty() || t.length > 120) return null
        repeat(3) { t = FILLER.replace(t, "").trim() }

        SEARCH.matchEntire(t)?.let { return LocalAction.WebSearch(it.groupValues[3].trim()).takeIf { a -> a.query.isNotEmpty() } }
        NAV.matchEntire(t)?.let { return LocalAction.Navigate(it.groupValues[3].trim()).takeIf { a -> a.place.isNotEmpty() } }
        DIAL.matchEntire(t)?.let { m ->
            val digits = m.groupValues[3].filter { it.isDigit() || it == '+' }
            // A name needs the contacts (not asked for): only a number is dialled here; a name goes to the model.
            return if (digits.count { it.isDigit() } >= 3 && m.groupValues[3].none { it.isLetter() }) LocalAction.Dial(digits) else null
        }
        val open = OPEN.matchEntire(t) ?: return null
        var what = open.groupValues[2].trim()
        if (what.startsWith("los ajustes") || what.startsWith("ajustes") || what.startsWith("la configuracion") || what.startsWith("configuracion") || what.startsWith("settings")) {
            val page = SETTINGS_WORDS.firstOrNull { (_, ws) -> ws.any { what.contains(it) } }?.first ?: SettingsPage.General
            return LocalAction.OpenSettings(page)
        }
        SETTINGS_WORDS.firstOrNull { (_, ws) -> ws.any { what == it } }?.let { return LocalAction.OpenSettings(it.first) }
        what = ARTICLES.replace(what, "").trim()
        // "abre youtube y busca gatos": two steps, for a model; one app name has no verb in it.
        if (what.isEmpty() || Regex("\\b(y|and|luego|despues|then)\\b").containsMatchIn(what) && what.split(' ').size > 3) return null
        return LocalAction.OpenApp(what)
    }
}

/** One launchable app on the phone. */
data class AppEntry(val label: String, val packageName: String)

/** Finds the app a person named, tolerant of accents, spaces, case and the usual nicknames. */
object AppMatcher {
    private val ALIASES = mapOf(
        "wasap" to "whatsapp", "guasap" to "whatsapp", "wsp" to "whatsapp", "insta" to "instagram", "ig" to "instagram",
        "camara" to "camera", "calculadora" to "calculator", "galeria" to "gallery", "fotos" to "photos",
        "mapas" to "maps", "correo" to "gmail", "ajustes" to "settings", "configuracion" to "settings",
        "reloj" to "clock", "calendario" to "calendar", "musica" to "music", "telefono" to "phone", "mensajes" to "messages",
        "tienda" to "play store", "play" to "play store", "yt" to "youtube", "tiktok" to "tik tok",
    )

    private fun key(s: String) = fold(s).replace(Regex("[^a-z0-9]"), "")

    /** The best match for [query] among [apps], or null when nothing is close enough. */
    fun best(query: String, apps: List<AppEntry>): AppEntry? {
        val q = fold(query)
        val wanted = listOf(key(q), key(ALIASES[q] ?: q)).filter { it.isNotEmpty() }.distinct()
        if (wanted.isEmpty()) return null
        var best: AppEntry? = null
        var bestScore = 0
        for (a in apps) {
            val l = key(a.label)
            if (l.isEmpty()) continue
            val score = wanted.maxOf { w ->
                when {
                    l == w -> 100
                    l.startsWith(w) -> 80 - (l.length - w.length).coerceAtMost(20)
                    w.length >= 3 && l.contains(w) -> 60 - (l.length - w.length).coerceAtMost(20)
                    w.startsWith(l) && l.length >= 4 -> 50
                    else -> 0
                }
            }
            if (score > bestScore) { bestScore = score; best = a }
        }
        return best.takeIf { bestScore >= 40 }
    }
}
