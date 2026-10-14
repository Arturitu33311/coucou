package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.phone.PhoneKey
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.DataInputStream
import java.security.KeyPairGenerator
import java.security.interfaces.RSAPublicKey
import java.util.Base64

class PhoneKeyTest {
    private val pair = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.generateKeyPair()

    @Test
    fun the_public_line_is_what_authorized_keys_takes() {
        val line = PhoneKey.openSshPublicKey(pair.public as RSAPublicKey, "coucou-test")
        val parts = line.split(" ")
        assertEquals(listOf("ssh-rsa", "coucou-test"), listOf(parts[0], parts[2]))
        // The blob is: string "ssh-rsa", mpint e, mpint n.
        val d = DataInputStream(ByteArrayInputStream(Base64.getDecoder().decode(parts[1])))
        fun next(): ByteArray = ByteArray(d.readInt()).also { d.readFully(it) }
        assertEquals("ssh-rsa", String(next()))
        val pub = pair.public as RSAPublicKey
        assertEquals(pub.publicExponent, java.math.BigInteger(next()))
        assertEquals(pub.modulus, java.math.BigInteger(next()))
        assertEquals(0, d.available())
    }

    @Test
    fun the_fingerprint_has_the_shape_ssh_keygen_prints() {
        val fp = PhoneKey.fingerprint(PhoneKey.openSshPublicKey(pair.public as RSAPublicKey))
        assertTrue(fp.matches(Regex("SHA256:[A-Za-z0-9+/]{43}")))
    }

    @Test
    fun the_key_pair_survives_being_taken_apart_and_put_back() {
        val back = PhoneKey.keyPairOf(pair.private.encoded, pair.public.encoded)
        assertArrayEquals(pair.private.encoded, back.private.encoded)
        assertArrayEquals(pair.public.encoded, back.public.encoded)
    }
}
