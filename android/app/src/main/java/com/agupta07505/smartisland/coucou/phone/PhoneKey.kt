/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.phone

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.io.File
import java.security.KeyFactory
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.MessageDigest
import java.security.interfaces.RSAPublicKey
import java.security.spec.PKCS8EncodedKeySpec
import java.security.spec.X509EncodedKeySpec
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The phone's own ssh key. It is made here (RSA 3072) and never leaves the phone: the private half
 * is kept encrypted with a key of the Android Keystore that cannot be taken out. Only the public
 * line is shown, to be pasted into `authorized_keys` on the server and the laptop (restricted to
 * the one command each is meant to run: see scripts/phone/README.md).
 */
object PhoneKey {
    private const val ALIAS = "coucou-phone-key-wrap"

    // ── Pure parts (tested on the JVM) ─────────────────────────────────────────

    private fun DataOutputStream.writeString(b: ByteArray) {
        writeInt(b.size)
        write(b)
    }

    /** SSH "mpint": big-endian, with a leading zero byte when the top bit would make it negative. */
    private fun mpint(v: java.math.BigInteger): ByteArray = v.toByteArray()

    /** `ssh-rsa AAAA… comment`, the line `authorized_keys` takes. */
    fun openSshPublicKey(key: RSAPublicKey, comment: String = "coucou-android"): String {
        val blob = ByteArrayOutputStream().also { bos ->
            DataOutputStream(bos).use {
                it.writeString("ssh-rsa".toByteArray())
                it.writeString(mpint(key.publicExponent))
                it.writeString(mpint(key.modulus))
            }
        }.toByteArray()
        return "ssh-rsa ${Base64.getEncoder().encodeToString(blob)} $comment"
    }

    /** `SHA256:…`, the fingerprint `ssh-keygen -l` prints for that line. */
    fun fingerprint(publicLine: String): String {
        val blob = Base64.getDecoder().decode(publicLine.trim().split(" ")[1])
        val digest = MessageDigest.getInstance("SHA-256").digest(blob)
        return "SHA256:" + Base64.getEncoder().withoutPadding().encodeToString(digest)
    }

    fun keyPairOf(pkcs8: ByteArray, x509: ByteArray): KeyPair {
        val f = KeyFactory.getInstance("RSA")
        return KeyPair(f.generatePublic(X509EncodedKeySpec(x509)), f.generatePrivate(PKCS8EncodedKeySpec(pkcs8)))
    }

    // ── Storage (Android only) ─────────────────────────────────────────────────

    private fun privateFile(c: Context) = File(c.applicationContext.filesDir, "coucou-phone-key.enc")
    private fun publicFile(c: Context) = File(c.applicationContext.filesDir, "coucou-phone-key.pub")

    private fun wrapKey(): SecretKey {
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

    fun exists(context: Context) = privateFile(context).exists() && publicFile(context).exists()

    /** Makes the key (once). Returns the line to paste into authorized_keys. */
    fun create(context: Context): String {
        check(!exists(context)) { "The phone already has a key" }
        val pair = KeyPairGenerator.getInstance("RSA").apply { initialize(3072) }.generateKeyPair()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, wrapKey()) }
        val sealed = cipher.iv + cipher.doFinal(pair.private.encoded)
        val tmp = File(privateFile(context).path + ".tmp")
        tmp.writeBytes(sealed)
        check(tmp.renameTo(privateFile(context)))
        publicFile(context).writeBytes(pair.public.encoded)
        return openSshPublicKey(pair.public as RSAPublicKey)
    }

    fun publicLine(context: Context): String? {
        if (!exists(context)) return null
        val f = KeyFactory.getInstance("RSA")
        return openSshPublicKey(f.generatePublic(X509EncodedKeySpec(publicFile(context).readBytes())) as RSAPublicKey)
    }

    /** The key pair, opened with the Keystore. Only the app's own process can do this. */
    fun load(context: Context): KeyPair {
        val sealed = privateFile(context).readBytes()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
            init(Cipher.DECRYPT_MODE, wrapKey(), GCMParameterSpec(128, sealed.copyOfRange(0, 12)))
        }
        return keyPairOf(cipher.doFinal(sealed.copyOfRange(12, sealed.size)), publicFile(context).readBytes())
    }
}
