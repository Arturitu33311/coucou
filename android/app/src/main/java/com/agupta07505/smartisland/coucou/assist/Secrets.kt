/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import org.json.JSONObject
import java.security.KeyStore
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

object SecretNames {
    const val OPENROUTER = "openrouter_key"
    const val TG_TOKEN = "telegram_token"
    const val TG_CHAT = "telegram_chat"
}

interface Secrets {
    fun get(name: String): String?
    fun put(name: String, value: String)
}

/** Values encrypted with an AES-GCM key of the Android Keystore (it cannot be taken out of the phone). */
class KeystoreSecrets(context: Context) : Secrets {
    private val prefs = context.applicationContext.getSharedPreferences("coucou_assist_secrets", Context.MODE_PRIVATE)

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    override fun put(name: String, value: String) {
        val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val blob = c.iv + c.doFinal(value.toByteArray())
        prefs.edit().putString(name, Base64.getEncoder().encodeToString(blob)).apply()
    }

    override fun get(name: String): String? {
        val raw = prefs.getString(name, null) ?: return null
        return runCatching {
            val blob = Base64.getDecoder().decode(raw)
            val c = Cipher.getInstance("AES/GCM/NoPadding")
                .apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, blob, 0, 12)) }
            String(c.doFinal(blob, 12, blob.size - 12))
        }.getOrNull()
    }

    private companion object {
        const val ALIAS = "coucou-assist-secrets-wrap"
    }
}

/** Repeats lines into Telegram with the bot's `sendMessage` (no polling, so it never competes with the relay). */
class TelegramMirror(
    private val secrets: Secrets,
    private val post: (url: String, key: String, body: String) -> String = ::httpPost,
) : Mirror {
    override fun deliver(text: String): Boolean {
        val token = secrets.get(SecretNames.TG_TOKEN)?.takeIf { it.isNotBlank() } ?: return false
        val chat = secrets.get(SecretNames.TG_CHAT)?.takeIf { it.isNotBlank() } ?: return false
        return runCatching {
            val body = JSONObject().put("chat_id", chat).put("text", text.take(4000)).toString()
            JSONObject(post("https://api.telegram.org/bot$token/sendMessage", "", body)).optBoolean("ok")
        }.getOrDefault(false)
    }
}

/** Brings the persona from the laptop. Throws [ChatException] if it cannot. */
fun refreshPersona(api: ChatApi, personas: PersonaStore) {
    val c = api.core()
    personas.save(Persona(c.core, c.memory))
}

/** One time: persona plus the secrets this phone needs to be independent. */
fun provisionFrom(api: ChatApi, secrets: Secrets, personas: PersonaStore) {
    refreshPersona(api, personas)
    val s = api.provision()
    if (s.openrouterKey.isNotBlank()) secrets.put(SecretNames.OPENROUTER, s.openrouterKey)
    if (s.telegramToken.isNotBlank()) secrets.put(SecretNames.TG_TOKEN, s.telegramToken)
    if (s.chat.isNotBlank()) secrets.put(SecretNames.TG_CHAT, s.chat)
}
