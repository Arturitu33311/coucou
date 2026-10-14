/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.phone

import android.content.Context
import net.schmizz.sshj.DefaultConfig
import net.schmizz.sshj.SSHClient
import net.schmizz.sshj.common.Buffer
import net.schmizz.sshj.connection.channel.direct.Session
import net.schmizz.sshj.transport.verification.HostKeyVerifier
import net.schmizz.sshj.userauth.keyprovider.KeyPairWrapper
import org.bouncycastle.jce.provider.BouncyCastleProvider
import java.io.Closeable
import java.io.InputStream
import java.io.OutputStream
import java.security.MessageDigest
import java.security.PublicKey
import java.security.Security
import java.util.Base64
import java.util.concurrent.TimeUnit

/** Where to connect: the server (Jinx's pendientes) or the laptop (Claude Code). */
data class SshProfile(
    val host: String = "",
    val port: Int = 22,
    val user: String = "",
    /** `SHA256:…` of the host's key, fixed at pairing. Nothing connects until it is set. */
    val hostKey: String = "",
) {
    val configured: Boolean get() = host.isNotBlank() && user.isNotBlank()
    val paired: Boolean get() = configured && hostKey.isNotBlank()
}

class SshException(message: String, cause: Throwable? = null) : Exception(message, cause)

/** What the phone does over ssh: one command per connection, run with the phone's own key. */
object Ssh {
    init {
        installProvider()
    }

    /**
     * Android ships a stripped-down "BC" provider (it is also where the platform's own TLS finds its default
     * key store, BKS); sshj needs the real one. The real one is built FIRST (loading it takes a while) and the
     * swap is one step, so there is no moment in which this process has no "BC": in that window any
     * HttpURLConnection died with a bare AssertionError, which took the island's whole process down. It goes
     * last: sshj asks for it by name, the platform keeps resolving to its own providers. Safe to call twice.
     */
    @Synchronized
    fun installProvider() {
        val current = Security.getProvider("BC")
        if (current is BouncyCastleProvider) return
        val full = BouncyCastleProvider()
        Security.removeProvider("BC")
        Security.addProvider(full)
    }

    private const val CONNECT_MS = 8_000
    private const val IO_MS = 20_000

    /** `SHA256:…` as `ssh-keygen -l` prints it. */
    fun fingerprint(key: PublicKey): String {
        val blob = Buffer.PlainBuffer().putPublicKey(key).compactData
        return "SHA256:" + Base64.getEncoder().withoutPadding().encodeToString(MessageDigest.getInstance("SHA-256").digest(blob))
    }

    private fun newClient(verifier: HostKeyVerifier, connectMs: Int = CONNECT_MS): SSHClient = SSHClient(DefaultConfig()).apply {
        connectTimeout = connectMs
        timeout = IO_MS
        addHostKeyVerifier(verifier)
    }

    /**
     * First meeting with a host: connects, takes its key's fingerprint and leaves without logging
     * in. The user compares it with what `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` says
     * on that host, and only then is it kept.
     */
    fun probe(profile: SshProfile): String {
        var seen: String? = null
        val client = newClient(object : HostKeyVerifier {
            override fun verify(hostname: String?, port: Int, key: PublicKey): Boolean {
                seen = fingerprint(key)
                return true
            }
            override fun findExistingAlgorithms(hostname: String?, port: Int): List<String> = emptyList()
        })
        try {
            client.connect(profile.host, profile.port)
        } catch (e: Exception) {
            if (seen == null) throw SshException("Cannot reach ${profile.host}:${profile.port} (${e.message})", e)
        } finally {
            runCatching { client.disconnect() }
        }
        return seen ?: throw SshException("${profile.host} did not show a host key")
    }

    /** A connection, logged in with the phone's key; the host key must be the one pinned at pairing. */
    fun open(context: Context, profile: SshProfile, connectTimeoutMs: Int = CONNECT_MS): Connection {
        if (!profile.paired) throw SshException("Not paired: pair ${profile.host.ifBlank { "the host" }} in Coucou's phone settings")
        val client = newClient(object : HostKeyVerifier {
            override fun verify(hostname: String?, port: Int, key: PublicKey) = fingerprint(key) == profile.hostKey
            override fun findExistingAlgorithms(hostname: String?, port: Int): List<String> = emptyList()
        }, connectTimeoutMs)
        try {
            client.connect(profile.host, profile.port)
            client.connection.keepAlive.keepAliveInterval = 15
            client.authPublickey(profile.user, KeyPairWrapper(PhoneKey.load(context)))
        } catch (e: Exception) {
            runCatching { client.disconnect() }
            throw SshException(explain(e, profile), e)
        }
        return Connection(client)
    }

    private fun explain(e: Exception, p: SshProfile): String = when {
        e.message?.contains("verif", ignoreCase = true) == true || e.message?.contains("host key", ignoreCase = true) == true ->
            "${p.host}'s key is not the one you trusted (it changed, or this is not the same machine)"
        e.message?.contains("Exhausted available authentication", ignoreCase = true) == true ||
            e.message?.contains("Authentication", ignoreCase = true) == true ->
            "${p.host} refused the phone's key: is its public line in authorized_keys?"
        else -> "Cannot reach ${p.host}:${p.port} (${e.message})"
    }

    class Connection(private val client: SSHClient) : Closeable {
        val connected: Boolean get() = client.isConnected && client.isAuthenticated

        /** Runs the (forced) command once, gives it [input] on stdin, and returns what it printed. */
        fun exec(command: String, input: String, timeoutSec: Long = 20): String {
            client.startSession().use { session ->
                val cmd = session.exec(command)
                cmd.outputStream.use { it.write(input.toByteArray()) }
                val out = cmd.inputStream.readBytes().toString(Charsets.UTF_8)
                cmd.join(timeoutSec, TimeUnit.SECONDS)
                return out
            }
        }

        /** A command that stays open: lines in, lines out. Closing it ends the command. */
        fun stream(command: String): Stream {
            val session = client.startSession()
            val cmd = session.exec(command)
            return Stream(session, cmd.inputStream, cmd.outputStream)
        }

        override fun close() {
            runCatching { client.disconnect() }
        }
    }

    class Stream(private val session: Session, val input: InputStream, val output: OutputStream) : Closeable {
        override fun close() {
            runCatching { session.close() }
        }
    }
}
