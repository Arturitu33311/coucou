package com.agupta07505.smartisland.coucou.notas

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.core.content.ContextCompat

/**
 * The single way to start a class recording: the Notas screen, the button on its card and the
 * quick-settings tile all come through here, so they cannot drift apart.
 */
object NotasArranque {
    private const val PREFS = "notas"
    private const val ULTIMA = "ultima_materia"

    /** The subject to record: the class on now, or the last one used. */
    fun materiaPara(clase: ClaseAhora?, ultima: String?): String? =
        clase?.materia?.takeIf { it.isNotBlank() } ?: ultima?.takeIf { it.isNotBlank() }

    fun recordar(context: Context, materia: String) {
        if (materia.isBlank()) return
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(ULTIMA, materia).apply()
    }

    fun ultima(context: Context): String? =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(ULTIMA, null)

    fun tieneMicrofono(context: Context): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    /**
     * Starts with the class on now (or the last subject). Blocking: call it from a background thread.
     * Returns false when there is no subject at all, so the caller can send the user to the screen.
     */
    fun iniciarConDeteccion(context: Context, segundoMic: Boolean = false): Boolean {
        val clase = runCatching { NotasApi().claseAhora() }.getOrNull()
        val materia = materiaPara(clase, ultima(context)) ?: return false
        NotasService.start(context, materia, materia, segundoMic)
        return true
    }
}
