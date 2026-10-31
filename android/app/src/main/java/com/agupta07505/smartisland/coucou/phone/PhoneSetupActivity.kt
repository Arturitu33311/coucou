/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.phone

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.agupta07505.smartisland.coucou.CoucouRuntime
import com.agupta07505.smartisland.coucou.agents.AgentUi
import com.agupta07505.smartisland.coucou.presence.Arbiter
import com.agupta07505.smartisland.coucou.ui.MochiGlyph
import com.agupta07505.smartisland.coucou.ui.MochiMood
import com.agupta07505.smartisland.coucou.ui.Outfit
import com.agupta07505.smartisland.coucou.ui.Wardrobe
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.followClaude
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.followJinx
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.jinxSync
import com.agupta07505.smartisland.coucou.presence.MochiPresence
import com.agupta07505.smartisland.coucou.phone.PhoneSettings.showDesk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Where the phone connects, and how it is allowed to. The phone makes its own key; the one line to
 * paste on each computer already carries the restriction (`restrict,command=…`), so that key can do
 * one thing there and nothing else.
 */
class PhoneSetupActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MaterialTheme(colorScheme = darkColorScheme(background = Color(0xFF0B0B10), surface = Color(0xFF16161C))) {
                Setup()
            }
        }
    }
}

private fun copy(context: Context, label: String, text: String) {
    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(label, text))
}

@Composable
private fun Setup() {
    val context = LocalContext.current
    var publicLine by remember { mutableStateOf(PhoneKey.publicLine(context)) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    Column(
        Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).statusBarsPadding()
            .verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Text("Phone link", color = Color.White, fontSize = 24.sp, fontWeight = FontWeight.SemiBold)
        Text(
            "The phone talks to your computers over ssh, with a key of its own that can only do one thing on each: " +
                "the server's key only reaches Jinx's pendientes, the laptop's only follows Claude Code. " +
                "Use the computers' Tailscale names or addresses.",
            color = Color.White.copy(alpha = 0.65f), fontSize = 13.sp,
        )

        Card("Mochi in the island") {
            var desk by remember { mutableStateOf(context.showDesk) }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Switch(checked = desk, onCheckedChange = { desk = it; context.showDesk = it; CoucouRuntime.apply(context) })
                Spacer(Modifier.width(10.dp))
                Text("Keep Mochi in the island, with my tasks", color = Color.White, fontSize = 14.sp)
            }
            Text("A small Mochi that is always there: asleep when nothing is open, looking up when something is overdue. Open the island to see the tasks and add one.", color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp)
        }

        Card("Cover on the lock screen") {
            var cover by remember { mutableStateOf(with(PhoneSettings) { context.lockCover }) }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Switch(
                    checked = cover,
                    onCheckedChange = {
                        cover = it
                        with(PhoneSettings) { context.lockCover = it }
                        if (!it) com.agupta07505.smartisland.coucou.LockCoverWallpaper.restore(context)
                    },
                )
                Spacer(Modifier.width(10.dp))
                Text("Show the cover of what's playing", color = Color.White, fontSize = 14.sp)
            }
            Text(
                "While music plays the lock screen shows the cover itself, centred over a blurred copy. " +
                    "It replaces your lock wallpaper meanwhile and puts it back about 90 seconds after the music stops. " +
                    "Nothing leaves the phone.",
                color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp,
            )
        }

        Card("Lyrics on the lock screen") {
            var on by remember { mutableStateOf(with(PhoneSettings) { context.lyricsOn }) }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Switch(
                    checked = on,
                    onCheckedChange = {
                        on = it
                        with(PhoneSettings) { context.lyricsOn = it }
                        if (!it) com.agupta07505.smartisland.coucou.Lyrics.clear()
                    },
                )
                Spacer(Modifier.width(10.dp))
                Text("Show the lyrics beside the record", color = Color.White, fontSize = 14.sp)
            }
            Text(
                "In the Coucou car wallpaper, the line being sung shows next to the record, in time with the song. " +
                    "To find them, the artist, title and album of each song are sent once to lrclib.net, a free community " +
                    "lyrics database (no account, nothing else is sent). Off by default.",
                color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp,
            )
        }

        Card("Mochi's outfit") {
            var choice by remember { mutableStateOf(with(PhoneSettings) { context.mochiOutfit }) }
            Text(
                "Auto follows the seasons (Santa hat in December, witch hat in October, bunny ears at Easter, sunglasses " +
                    "in summer, party hat at New Year), a night cap when he sleeps and headphones when music plays.",
                color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp,
            )
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                val options = listOf("auto" to "Auto") + Outfit.entries.map { it.id to it.label }
                for ((id, label) in options) {
                    val on = choice == id
                    Column(
                        Modifier
                            .clip(RoundedCornerShape(14.dp))
                            .background(if (on) Color.White.copy(alpha = 0.16f) else Color.White.copy(alpha = 0.05f))
                            .clickable {
                                choice = id
                                with(PhoneSettings) { context.mochiOutfit = id }
                            }
                            .padding(horizontal = 10.dp, vertical = 8.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        // A live preview of each one (auto previews what it picks today).
                        val shown = if (id == "auto") Wardrobe.pick("auto", MochiMood.Idle) else Outfit.of(id) ?: Outfit.None
                        MochiGlyph(size = 52.dp, mood = MochiMood.Idle, outfit = shown)
                        Spacer(Modifier.height(4.dp))
                        Text(label, color = Color.White.copy(alpha = if (on) 1f else 0.7f), fontSize = 11.sp)
                    }
                }
            }
        }

        Card("This phone") {
            var id by remember { mutableStateOf(PhoneSettings.deviceId(context)) }
            Text(
                "What it calls itself on the laptop's link and in the presence server's heartbeat, so two phones can " +
                    "share Mochi (he stays on the one in use). By default it comes from the model.",
                color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                for (phone in Arbiter.PHONES.sorted()) {
                    Button(
                        onClick = { id = phone; PhoneSettings.saveDeviceId(context, phone); CoucouRuntime.apply(context) },
                        enabled = id != phone,
                    ) { Text(phone.uppercase()) }
                }
            }
            Text("Now: ${id.uppercase()} (applies at the next start of the link).", color = Color.White.copy(alpha = 0.55f), fontSize = 12.sp)
        }

        Card("1 · The phone's key") {
            val line = publicLine
            if (line == null) {
                Button(onClick = {
                    scope.launch {
                        error = runCatching { withContext(Dispatchers.Default) { PhoneKey.create(context) } }
                            .fold({ publicLine = it; null }, { it.message })
                    }
                }) { Text("Make the key") }
                Text("An RSA 3072 key, kept encrypted with the Android Keystore. It never leaves the phone.", color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp)
            } else {
                Text("Fingerprint ${PhoneKey.fingerprint(line)}", color = Color.White.copy(alpha = 0.7f), fontSize = 12.sp)
            }
        }

        publicLine?.let { line ->
            HostCard(
                name = PhoneSettings.SERVER, title = "2 · The server (Jinx's pendientes)",
                authorizedLine = { user -> "restrict,command=\"python3 /home/$user/.hermes/coucou/pendientes-gateway.py\" $line" },
                switchLabel = "Share my tasks with Jinx", switchOn = context.jinxSync,
                onSwitch = { context.jinxSync = it; CoucouRuntime.apply(context); CoucouRuntime.kick() },
                status = { CoucouRuntime.jinx.collectAsState().value.let { o -> o.error ?: "${o.fromJinx.size} of Jinx's own pendientes" } },
            )
            HostCard(
                name = PhoneSettings.LAPTOP, title = "3 · The laptop (Claude Code)",
                authorizedLine = { user -> "restrict,command=\"/home/$user/.local/share/coucou/bin/coucou-hook --remote\" $line" },
                switchLabel = "Follow Claude Code and answer it", switchOn = context.followClaude,
                onSwitch = { context.followClaude = it; CoucouRuntime.apply(context) },
                status = {
                    val ui = CoucouRuntime.agentUi.collectAsState().value
                    when (ui.status) {
                        AgentUi.Status.Off -> ui.detail.ifEmpty { "off" }
                        AgentUi.Status.Connecting -> "connecting…"
                        AgentUi.Status.Connected -> {
                            // Who has Mochi right now: it follows whichever device the person is using.
                            val here = com.agupta07505.smartisland.coucou.presence.DeviceActivity.active.collectAsState().value
                            "connected · ${ui.sessions.size} session(s) · Mochi is " + if (here) "on this phone" else "on the laptop"
                        }
                        AgentUi.Status.Reconnecting -> "reconnecting… ${ui.detail}"
                        AgentUi.Status.LaptopClosed -> "Coucou is closed on the laptop"
                    }
                },
            )
            PresenceCard()
        }
        error?.let { Text(it, color = Color(0xFFF5A524), fontSize = 12.sp) }
        Spacer(Modifier.padding(20.dp))
    }
}

/**
 * Where Jinx's texts reach this phone: the presence server over Tailscale,
 * with the inbox key. Text only, no voice. Needs no ssh: just the key.
 */
@Composable
private fun PresenceCard() {
    val context = LocalContext.current
    var on by remember { mutableStateOf(context.followJinx) }
    var url by remember { mutableStateOf(PhoneSettings.presenceUrl(context)) }
    var key by remember { mutableStateOf(PhoneSettings.mochiKey(context)) }
    val status by MochiPresence.status.collectAsState()

    Card("4 · Jinx's texts (presence)") {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Switch(checked = on, onCheckedChange = { on = it; context.followJinx = it; CoucouRuntime.apply(context) })
            Spacer(Modifier.width(10.dp))
            Text("Hear Jinx's texts on this phone", color = Color.White, fontSize = 14.sp)
        }
        Text(
            "Heartbeat says this phone is looked at; Jinx's texts then land on the island instead of Telegram. Only while the screen is on. The key is the same as ~/.config/mochi-presence/key on the server.",
            color = Color.White.copy(alpha = 0.5f), fontSize = 12.sp,
        )
        OutlinedTextField(url, { url = it; PhoneSettings.savePresenceUrl(context, it) },
            label = { Text("Presence URL") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(key, { key = it; PhoneSettings.saveMochiKey(context, it) },
            label = { Text("Inbox key") }, singleLine = true, modifier = Modifier.fillMaxWidth(),
            visualTransformation = PasswordVisualTransformation())
        Text(status, color = Color.White.copy(alpha = 0.55f), fontSize = 12.sp)
    }
}

@Composable
private fun Card(title: String, content: @Composable () -> Unit) {    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(MaterialTheme.colorScheme.surface).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(title, color = Color.White, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
        content()
    }
}

@Composable
private fun HostCard(
    name: String,
    title: String,
    authorizedLine: (String) -> String,
    switchLabel: String,
    switchOn: Boolean,
    onSwitch: (Boolean) -> Unit,
    status: @Composable () -> String,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var profile by remember { mutableStateOf(PhoneSettings.profile(context, name)) }
    var host by remember { mutableStateOf(profile.host) }
    var port by remember { mutableStateOf(profile.port.toString()) }
    var user by remember { mutableStateOf(profile.user) }
    var seen by remember { mutableStateOf<String?>(null) }
    var note by remember { mutableStateOf<String?>(null) }
    var on by remember { mutableStateOf(switchOn) }

    fun current() = SshProfile(host.trim(), port.toIntOrNull() ?: 22, user.trim(), if (PhoneSettings.changedHost(profile, SshProfile(host, port.toIntOrNull() ?: 22))) "" else profile.hostKey)

    Card(title) {
        OutlinedTextField(host, { host = it }, label = { Text("Host (name or address)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(user, { user = it }, label = { Text("User") }, singleLine = true, modifier = Modifier.weight(1f))
            OutlinedTextField(port, { port = it.filter(Char::isDigit) }, label = { Text("Port") }, singleLine = true, modifier = Modifier.width(96.dp))
        }
        if (user.isNotBlank()) {
            Text("Paste this line in ~/.ssh/authorized_keys on that computer:", color = Color.White.copy(alpha = 0.6f), fontSize = 12.sp)
            SelectionContainer {
                Text(
                    authorizedLine(user.trim()), color = Color.White.copy(alpha = 0.85f), fontSize = 10.sp, fontFamily = FontFamily.Monospace,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(Color.White.copy(alpha = 0.06f)).padding(8.dp),
                )
            }
            OutlinedButton(onClick = { copy(context, "authorized_keys line", authorizedLine(user.trim())) }) { Text("Copy the line") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                enabled = host.isNotBlank() && user.isNotBlank(),
                onClick = {
                    note = "meeting ${host.trim()}…"
                    scope.launch {
                        val p = SshProfile(host.trim(), port.toIntOrNull() ?: 22, user.trim())
                        runCatching { withContext(Dispatchers.IO) { Ssh.probe(p) } }
                            .onSuccess { seen = it; note = "It shows this key. Compare it with the one on that computer, then trust it." }
                            .onFailure { seen = null; note = it.message }
                    }
                },
            ) { Text(if (profile.paired) "Meet again" else "Meet") }
            OutlinedButton(
                enabled = profile.paired,
                onClick = {
                    note = "testing…"
                    scope.launch {
                        note = withContext(Dispatchers.IO) {
                            runCatching { Ssh.open(context, profile).use { "Connected as ${profile.user}" } }.getOrElse { it.message }
                        }
                    }
                },
            ) { Text("Test") }
        }
        seen?.let { fp ->
            Text(fp, color = Color(0xFF86EFAC), fontSize = 12.sp, fontFamily = FontFamily.Monospace)
            Text("On that computer: ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub", color = Color.White.copy(alpha = 0.5f), fontSize = 11.sp, fontFamily = FontFamily.Monospace)
            Button(onClick = {
                val p = SshProfile(host.trim(), port.toIntOrNull() ?: 22, user.trim(), fp)
                PhoneSettings.save(context, name, p)
                profile = p
                seen = null
                note = "Trusted."
            }) { Text("It matches: trust it") }
        }
        if (profile.paired) Text("Trusted: ${profile.hostKey}", color = Color.White.copy(alpha = 0.5f), fontSize = 11.sp, fontFamily = FontFamily.Monospace)
        note?.let { Text(it, color = Color.White.copy(alpha = 0.75f), fontSize = 12.sp) }
        // Saving what was typed (without pairing) so the line above stays when the screen is left.
        androidx.compose.runtime.LaunchedEffect(host, port, user) {
            val c = current()
            if (c.host != profile.host || c.user != profile.user || c.port != profile.port) {
                PhoneSettings.save(context, name, c)
                profile = c
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Switch(checked = on, enabled = profile.paired, onCheckedChange = { on = it; onSwitch(it) })
            Spacer(Modifier.width(10.dp))
            Text(switchLabel, color = Color.White, fontSize = 14.sp)
        }
        Text(status(), color = Color.White.copy(alpha = 0.55f), fontSize = 12.sp)
    }
}
