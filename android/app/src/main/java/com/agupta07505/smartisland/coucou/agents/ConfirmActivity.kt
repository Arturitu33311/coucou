/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.agents

import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.Bundle
import android.os.CancellationSignal

/**
 * Allowing a permission request from the phone asks for the fingerprint (or the screen lock) first:
 * a phone left on a table must not let anyone approve a command on the laptop with one tap.
 * An invisible activity runs the system's prompt and reports back. On a phone with neither a
 * biometric nor a lock screen there is nothing to ask: the island asks for a second tap instead.
 */
class ConfirmActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val title = intent.getStringExtra(EXTRA_TITLE) ?: "Allow on the laptop?"
        if (Build.VERSION.SDK_INT < 30) {
            finish(false)
            return
        }
        val prompt = BiometricPrompt.Builder(this)
            .setTitle(title)
            .setSubtitle(intent.getStringExtra(EXTRA_SUBTITLE) ?: "")
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL)
            .build()
        prompt.authenticate(
            CancellationSignal(),
            mainExecutor,
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult?) = finish(true)
                override fun onAuthenticationError(errorCode: Int, errString: CharSequence?) = finish(false)
            },
        )
    }

    private fun finish(ok: Boolean) {
        val cb = pending
        pending = null
        cb?.invoke(ok)
        finish()
    }

    override fun onDestroy() {
        // Dismissed some other way (the activity was swept away): that is a "no".
        pending?.invoke(false)
        pending = null
        super.onDestroy()
    }

    companion object {
        private const val EXTRA_TITLE = "title"
        private const val EXTRA_SUBTITLE = "subtitle"

        @Volatile private var pending: ((Boolean) -> Unit)? = null

        /** Can the phone ask for a fingerprint or the screen lock at all? */
        fun available(context: Context): Boolean {
            if (Build.VERSION.SDK_INT < 30) return false
            // Asking must never be able to take the island down: whatever goes wrong, there is nothing to ask with.
            return runCatching {
                val bm = context.getSystemService(BiometricManager::class.java) ?: return@runCatching false
                val ok = bm.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL)
                ok == BiometricManager.BIOMETRIC_SUCCESS ||
                    (context.getSystemService(KeyguardManager::class.java)?.isDeviceSecure == true)
            }.getOrDefault(false)
        }

        /** Asks; [then] is called with whether it was confirmed. */
        fun ask(context: Context, title: String, subtitle: String, then: (Boolean) -> Unit) {
            pending?.invoke(false)
            pending = then
            context.startActivity(
                Intent(context, ConfirmActivity::class.java)
                    .putExtra(EXTRA_TITLE, title)
                    .putExtra(EXTRA_SUBTITLE, subtitle)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION),
            )
        }
    }
}
