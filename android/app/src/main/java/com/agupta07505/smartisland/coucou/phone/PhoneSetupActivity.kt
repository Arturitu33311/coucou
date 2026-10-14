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
import androidx.compose.foundation.border
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
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.TextButton
import androidx.compose.ui.text.style.TextOverflow
import com.agupta07505.smartisland.coucou.CoucouHome
import com.agupta07505.smartisland.coucou.CoucouTab
import com.agupta07505.smartisland.coucou.ui.CoucouColors
import com.agupta07505.smartisland.coucou.ui.GroupLabel
import com.agupta07505.smartisland.coucou.ui.ScreenHeader
import com.agupta07505.smartisland.coucou.ui.SectionCard
import com.agupta07505.smartisland.coucou.ui.StatusLine
import com.agupta07505.smartisland.coucou.ui.SwitchRow
import com.agupta07505.smartisland.coucou.ui.Tone
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
 * one thing there and nothing else. Shown as the Coucou app's Ajustes tab.
 */
class PhoneSetupActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { CoucouHome(initial = CoucouTab.Settings) }
    }
}

/** The wardrobe's names in Spanish, for this screen (the ids are the contract; Outfit.label stays the Mac's). */
private val OUTFIT_ES = mapOf(
    "none" to "Nada", "partyHat" to "Fiesta", "beanie" to "Gorro", "crown" to "Corona", "sunglasses" to "Gafas de sol",
    "roundGlasses" to "Gafas", "bow" to "Moño", "scarf" to "Bufanda", "witchHat" to "Bruja", "pumpkin" to "Calabaza",
    "santaHat" to "Santa", "bunnyEars" to "Conejo", "headphones" to "Cascos", "nightcap" to "Gorro de dormir",
    "necktie" to "Corbata", "monocle" to "Monóculo", "headband" to "Diadema", "earmuffs" to "Orejeras",
    "bandana" to "Pañuelo", "wizardHat" to "Mago",
)

private fun copy(context: Context, label: String, text: String) {
    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(label, text))
}

private val fieldColors @Composable get() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = CoucouColors.Accent,
    unfocusedBorderColor = Color.White.copy(alpha = 0.12f),
    focusedLabelColor = CoucouColors.Accent,
    cursorColor = CoucouColors.Accent,
    focusedTextColor = CoucouColors.Text,
    unfocusedTextColor = CoucouColors.Text,
)

private val primaryButton @Composable get() = ButtonDefaults.buttonColors(
    containerColor = CoucouColors.Accent, contentColor = Color(0xFF04140D),
)

@Composable
internal fun Setup(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    var publicLine by remember { mutableStateOf(PhoneKey.publicLine(context)) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    Column(
        modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        ScreenHeader(title = "Ajustes", subtitle = "Mochi, este teléfono y la conexión con tus computadoras")

        GroupLabel("Mochi")
        SectionCard {
            var desk by remember { mutableStateOf(context.showDesk) }
            SwitchRow(
                title = "Mochi en la isla, con mis pendientes",
                description = "Un Mochi pequeño siempre ahí: dormido si no hay nada, atento cuando algo vence. Abre la isla para ver y añadir pendientes.",
                checked = desk,
                onChange = { desk = it; context.showDesk = it; CoucouRuntime.apply(context) },
            )
        }
        SectionCard(
            title = "Ropa de Mochi",
            subtitle = "Auto sigue las estaciones (gorro de Santa en diciembre, de bruja en octubre, orejas de conejo en Pascua, " +
                "gafas de sol en verano, gorro de fiesta en Año Nuevo), gorro de dormir cuando duerme y cascos con música.",
        ) {
            var choice by remember { mutableStateOf(with(PhoneSettings) { context.mochiOutfit }) }
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                val options = listOf("auto" to "Auto") + Outfit.entries.map { it.id to (OUTFIT_ES[it.id] ?: it.label) }
                for ((id, label) in options) {
                    val on = choice == id
                    Column(
                        Modifier
                            .width(76.dp)
                            .clip(RoundedCornerShape(16.dp))
                            .background(if (on) CoucouColors.AccentSoft else CoucouColors.SurfaceHigh)
                            .border(1.5.dp, if (on) CoucouColors.Accent else Color.Transparent, RoundedCornerShape(16.dp))
                            .clickable {
                                choice = id
                                with(PhoneSettings) { context.mochiOutfit = id }
                            }
                            .padding(vertical = 10.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        // A live preview of each one (auto previews what it picks today).
                        val shown = if (id == "auto") Wardrobe.pick("auto", MochiMood.Idle) else Outfit.of(id) ?: Outfit.None
                        MochiGlyph(size = 46.dp, mood = MochiMood.Idle, outfit = shown)
                        Spacer(Modifier.height(6.dp))
                        Text(
                            label, color = if (on) CoucouColors.Text else CoucouColors.TextSecondary, fontSize = 11.sp,
                            maxLines = 1, overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }

        GroupLabel("Este teléfono")
        SectionCard(
            subtitle = "Cómo se llama en el enlace con la laptop y en el latido del servidor de presencia, para que los dos " +
                "teléfonos compartan a Mochi (se queda en el que usas). Por defecto sale del modelo.",
        ) {
            var id by remember { mutableStateOf(PhoneSettings.deviceId(context)) }
            Row(
                Modifier.clip(RoundedCornerShape(14.dp)).background(CoucouColors.SurfaceHigh).padding(4.dp),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                for (phone in Arbiter.PHONES.sorted()) {
                    val on = id == phone
                    Text(
                        phone.uppercase(),
                        color = if (on) Color(0xFF04140D) else CoucouColors.TextSecondary,
                        fontSize = 14.sp, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.clip(RoundedCornerShape(10.dp))
                            .background(if (on) CoucouColors.Accent else Color.Transparent)
                            .clickable(enabled = !on) { id = phone; PhoneSettings.saveDeviceId(context, phone); CoucouRuntime.apply(context) }
                            .padding(horizontal = 22.dp, vertical = 8.dp),
                    )
                }
            }
            Text("Se aplica la próxima vez que arranque el enlace.", color = CoucouColors.TextTertiary, fontSize = 12.sp)
        }

        GroupLabel("Conexiones")
        Text(
            "El teléfono habla con tus computadoras por ssh, con una llave propia que en cada una solo puede hacer una " +
                "cosa: en el servidor, los pendientes de Jinx; en la laptop, seguir a Claude Code y hablar con Jinx. " +
                "Usa los nombres o direcciones de Tailscale.",
            color = CoucouColors.TextTertiary, fontSize = 12.5.sp, lineHeight = 17.sp, modifier = Modifier.padding(horizontal = 4.dp),
        )
        SectionCard(title = "1 · La llave del teléfono") {
            val line = publicLine
            if (line == null) {
                Button(colors = primaryButton, onClick = {
                    scope.launch {
                        error = runCatching { withContext(Dispatchers.Default) { PhoneKey.create(context) } }
                            .fold({ publicLine = it; null }, { it.message })
                    }
                }) { Text("Crear la llave") }
                Text("Una llave RSA 3072, cifrada con el Android Keystore. Nunca sale del teléfono.", color = CoucouColors.TextTertiary, fontSize = 12.sp)
            } else {
                StatusLine("Llave lista", Tone.Good)
                Text(PhoneKey.fingerprint(line), color = CoucouColors.TextTertiary, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
            }
        }

        publicLine?.let { line ->
            HostCard(
                name = PhoneSettings.SERVER, title = "2 · El servidor (pendientes de Jinx)",
                authorizedLine = { user -> "restrict,command=\"python3 /home/$user/.hermes/coucou/pendientes-gateway.py\" $line" },
                switchLabel = "Compartir mis pendientes con Jinx", switchOn = context.jinxSync,
                onSwitch = { context.jinxSync = it; CoucouRuntime.apply(context); CoucouRuntime.kick() },
                status = {
                    CoucouRuntime.jinx.collectAsState().value.let { o ->
                        o.error?.let { it to Tone.Warn } ?: ("${o.fromJinx.size} pendientes propios de Jinx" to Tone.Good)
                    }
                },
            )
            HostCard(
                name = PhoneSettings.LAPTOP, title = "3 · La laptop (Claude Code y Jinx)",
                authorizedLine = { user -> "restrict,command=\"/home/$user/.local/share/coucou/bin/coucou-hook --remote\" $line" },
                switchLabel = "Seguir a Claude Code y contestarle", switchOn = context.followClaude,
                onSwitch = { context.followClaude = it; CoucouRuntime.apply(context) },
                status = {
                    val ui = CoucouRuntime.agentUi.collectAsState().value
                    when (ui.status) {
                        AgentUi.Status.Off -> ui.detail.ifEmpty { "apagado" } to Tone.Neutral
                        AgentUi.Status.Connecting -> "conectando…" to Tone.Neutral
                        AgentUi.Status.Connected -> {
                            // Who has Mochi right now: it follows whichever device the person is using.
                            val here = com.agupta07505.smartisland.coucou.presence.DeviceActivity.active.collectAsState().value
                            ("conectado · ${ui.sessions.size} sesión(es) · Mochi está " + if (here) "en este teléfono" else "en la laptop") to Tone.Good
                        }
                        AgentUi.Status.Reconnecting -> "reconectando… ${ui.detail}" to Tone.Warn
                        AgentUi.Status.LaptopClosed -> "Coucou está cerrado en la laptop" to Tone.Warn
                    }
                },
            )
            PresenceCard()
        }
        error?.let { Text(it, color = CoucouColors.Warning, fontSize = 12.5.sp) }
        Spacer(Modifier.padding(12.dp))
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

    SectionCard(title = "4 · Los mensajes de Jinx (presencia)") {
        SwitchRow(
            title = "Recibir los mensajes de Jinx aquí",
            description = "El latido dice que miras este teléfono; los mensajes de Jinx llegan entonces a la isla en vez de a " +
                "Telegram. Solo con la pantalla encendida. La llave es la de ~/.config/mochi-presence/key en el servidor.",
            checked = on,
            onChange = { on = it; context.followJinx = it; CoucouRuntime.apply(context) },
        )
        OutlinedTextField(url, { url = it; PhoneSettings.savePresenceUrl(context, it) },
            label = { Text("URL de presencia") }, singleLine = true, modifier = Modifier.fillMaxWidth(), colors = fieldColors,
            shape = RoundedCornerShape(14.dp))
        OutlinedTextField(key, { key = it; PhoneSettings.saveMochiKey(context, it) },
            label = { Text("Llave del buzón") }, singleLine = true, modifier = Modifier.fillMaxWidth(), colors = fieldColors,
            shape = RoundedCornerShape(14.dp), visualTransformation = PasswordVisualTransformation())
        val (text, tone) = presenceLabel(status)
        StatusLine(text, tone)
    }
}

/** MochiPresence's status, in the screen's words and colour. */
private fun presenceLabel(status: String): Pair<String, Tone> = when {
    status == "off" -> "apagado" to Tone.Neutral
    status == "awake" || status == "presencia aquí" -> (if (status == "awake") "activo" else status) to Tone.Good
    status == "screen off" -> "pantalla apagada" to Tone.Neutral
    status == "presencia en otro dispositivo" -> status to Tone.Neutral
    status.startsWith("error") || status.startsWith("no screen events") -> status to Tone.Bad
    else -> status to Tone.Warn
}

@Composable
private fun HostCard(
    name: String,
    title: String,
    authorizedLine: (String) -> String,
    switchLabel: String,
    switchOn: Boolean,
    onSwitch: (Boolean) -> Unit,
    status: @Composable () -> Pair<String, Tone>,
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

    SectionCard(title = title) {
        val (statusText, tone) = status()
        StatusLine(if (profile.paired) statusText else "sin emparejar", if (profile.paired) tone else Tone.Neutral)
        OutlinedTextField(host, { host = it }, label = { Text("Equipo (nombre o dirección)") }, singleLine = true,
            modifier = Modifier.fillMaxWidth(), colors = fieldColors, shape = RoundedCornerShape(14.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(user, { user = it }, label = { Text("Usuario") }, singleLine = true, modifier = Modifier.weight(1f),
                colors = fieldColors, shape = RoundedCornerShape(14.dp))
            OutlinedTextField(port, { port = it.filter(Char::isDigit) }, label = { Text("Puerto") }, singleLine = true,
                modifier = Modifier.width(96.dp), colors = fieldColors, shape = RoundedCornerShape(14.dp))
        }
        if (user.isNotBlank()) {
            Text("Pega esta línea en ~/.ssh/authorized_keys de ese equipo:", color = CoucouColors.TextSecondary, fontSize = 12.5.sp)
            SelectionContainer {
                Text(
                    authorizedLine(user.trim()), color = CoucouColors.Text.copy(alpha = 0.85f), fontSize = 10.sp, fontFamily = FontFamily.Monospace,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(CoucouColors.SurfaceHigh).padding(10.dp),
                )
            }
            TextButton(onClick = { copy(context, "authorized_keys line", authorizedLine(user.trim())) }) {
                Text("Copiar la línea", color = CoucouColors.Accent)
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                colors = primaryButton,
                enabled = host.isNotBlank() && user.isNotBlank(),
                onClick = {
                    note = "conectando con ${host.trim()}…"
                    scope.launch {
                        val p = SshProfile(host.trim(), port.toIntOrNull() ?: 22, user.trim())
                        runCatching { withContext(Dispatchers.IO) { Ssh.probe(p) } }
                            .onSuccess { seen = it; note = "Muestra esta llave. Compárala con la de ese equipo y, si coincide, confía en ella." }
                            .onFailure { seen = null; note = it.message }
                    }
                },
            ) { Text(if (profile.paired) "Volver a emparejar" else "Emparejar") }
            OutlinedButton(
                enabled = profile.paired,
                onClick = {
                    note = "probando…"
                    scope.launch {
                        note = withContext(Dispatchers.IO) {
                            runCatching { Ssh.open(context, profile).use { "Conectado como ${profile.user}" } }.getOrElse { it.message }
                        }
                    }
                },
            ) { Text("Probar") }
        }
        seen?.let { fp ->
            Text(fp, color = Color(0xFF86EFAC), fontSize = 12.sp, fontFamily = FontFamily.Monospace)
            Text("En ese equipo: ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub", color = CoucouColors.TextTertiary, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
            Button(colors = primaryButton, onClick = {
                val p = SshProfile(host.trim(), port.toIntOrNull() ?: 22, user.trim(), fp)
                PhoneSettings.save(context, name, p)
                profile = p
                seen = null
                note = "Confiado."
            }) { Text("Coincide: confiar") }
        }
        if (profile.paired) Text("Llave del equipo: ${profile.hostKey}", color = CoucouColors.TextTertiary, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
        note?.let { Text(it, color = CoucouColors.TextSecondary, fontSize = 12.5.sp) }
        // Saving what was typed (without pairing) so the line above stays when the screen is left.
        androidx.compose.runtime.LaunchedEffect(host, port, user) {
            val c = current()
            if (c.host != profile.host || c.user != profile.user || c.port != profile.port) {
                PhoneSettings.save(context, name, c)
                profile = c
            }
        }
        SwitchRow(title = switchLabel, checked = on, enabled = profile.paired, onChange = { on = it; onSwitch(it) })
    }
}
