package com.agupta07505.smartisland.ui.sections

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.SystemClock
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import com.agupta07505.smartisland.R
import com.agupta07505.smartisland.coucou.notas.ClaseAhora
import com.agupta07505.smartisland.coucou.notas.NotasApi
import com.agupta07505.smartisland.coucou.notas.NotasBus
import com.agupta07505.smartisland.coucou.notas.NotasCola
import com.agupta07505.smartisland.coucou.notas.NotasService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

@Composable
fun NotasSection() {
    val context = LocalContext.current
    val estado by NotasBus.estado.collectAsState()

    var materias by remember { mutableStateOf<List<String>>(emptyList()) }
    var materia by remember { mutableStateOf("") }
    var clase by remember { mutableStateOf<ClaseAhora?>(null) }
    var offline by remember { mutableStateOf(false) }
    var segundoMic by remember { mutableStateOf(false) }
    var permisoNegado by remember { mutableStateOf(false) }
    var ahora by remember { mutableStateOf(SystemClock.elapsedRealtime()) }

    // On entering: what class is on now (preselects the subject) and the subject list. Also resend what is left on disk.
    LaunchedEffect(Unit) {
        withContext(Dispatchers.IO) {
            val api = NotasApi()
            runCatching { api.materias() }.onSuccess { materias = it; offline = false }.onFailure { offline = true }
            runCatching { api.claseAhora() }.onSuccess { c ->
                clase = c
                if (c != null && materia.isBlank()) materia = c.materia
            }
            if (!NotasBus.estado.value.grabando) {
                val q = NotasCola.get(context)
                q.clearStale()
                if (q.pendingCount() > 0) {
                    q.drain { NotasBus.update { it.copy(pendientes = q.pendingCount()) } }
                }
                NotasBus.update { it.copy(pendientes = q.pendingCount()) }
            }
        }
    }
    LaunchedEffect(estado.grabando) {
        while (estado.grabando) { ahora = SystemClock.elapsedRealtime(); delay(500) }
    }

    fun grabar() {
        NotasBus.update { it.copy(error = null) }
        NotasService.start(context, materia, materia, segundoMic)
    }

    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { r ->
        // Notifications are optional (the recording keeps going without them); the microphone is not.
        if (r[Manifest.permission.RECORD_AUDIO] == true) { permisoNegado = false; grabar() } else permisoNegado = true
    }

    fun onGrabar() {
        val mic = ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        if (mic) { permisoNegado = false; grabar(); return }
        val wanted = buildList {
            add(Manifest.permission.RECORD_AUDIO)
            if (Build.VERSION.SDK_INT >= 33) add(Manifest.permission.POST_NOTIFICATIONS)
        }
        launcher.launch(wanted.toTypedArray())
    }

    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Card(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(16.dp),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.35f)),
            elevation = CardDefaults.cardElevation(defaultElevation = 0.dp),
        ) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                val c = clase
                if (c != null) {
                    Text(
                        text = stringResource(R.string.notas_detected, c.materia, c.fin ?: "—"),
                        style = MaterialTheme.typography.titleSmall,
                        color = MaterialTheme.colorScheme.primary,
                        fontWeight = FontWeight.SemiBold,
                    )
                } else {
                    Text(
                        text = stringResource(if (offline) R.string.notas_offline else R.string.notas_no_class),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Text(stringResource(R.string.notas_subject), style = MaterialTheme.typography.labelLarge)
                // one chip per subject; no free typing while recording
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    materias.forEach { m ->
                        FilterChip(
                            selected = m == materia,
                            enabled = !estado.grabando,
                            onClick = { materia = m },
                            label = { Text(m) },
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(R.string.notas_second_mic), style = MaterialTheme.typography.bodyLarge)
                        Text(
                            stringResource(R.string.notas_second_mic_hint),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(checked = segundoMic, enabled = !estado.grabando, onCheckedChange = { segundoMic = it })
                }
            }
        }

        if (estado.grabando) {
            val s = ((ahora - estado.inicio) / 1000).coerceAtLeast(0)
            Text(
                text = "%02d:%02d:%02d".format(s / 3600, s % 3600 / 60, s % 60),
                style = MaterialTheme.typography.displayMedium,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        Button(
            onClick = { if (estado.grabando) NotasService.send(context, NotasService.ACTION_STOP) else onGrabar() },
            enabled = estado.grabando || materia.isNotBlank(),
            modifier = Modifier.fillMaxWidth().height(72.dp),
            shape = RoundedCornerShape(20.dp),
            colors = if (estado.grabando) ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
            else ButtonDefaults.buttonColors(),
        ) {
            Text(
                stringResource(if (estado.grabando) R.string.notas_stop else R.string.notas_record),
                style = MaterialTheme.typography.titleLarge,
            )
        }

        if (estado.grabando) {
            OutlinedButton(
                onClick = { NotasService.send(context, NotasService.ACTION_MARK) },
                modifier = Modifier.fillMaxWidth(),
            ) { Text(stringResource(R.string.notas_mark) + if (estado.marcas > 0) " (" + estado.marcas + ")" else "") }
            if (segundoMic && !estado.segundoMicActivo) {
                Text(
                    stringResource(R.string.notas_second_mic_failed),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }

        if (permisoNegado) {
            Text(
                stringResource(R.string.notas_perm_denied),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.error,
            )
        }
        estado.error?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.error) }

        Text(
            text = if (estado.pendientes > 0) stringResource(R.string.notas_upload_pending, estado.pendientes)
            else stringResource(R.string.notas_upload_ok),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
