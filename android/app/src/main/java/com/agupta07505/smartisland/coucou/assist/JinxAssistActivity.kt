/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.app.Application
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.agupta07505.smartisland.coucou.CoucouHome
import com.agupta07505.smartisland.coucou.CoucouRuntime
import com.agupta07505.smartisland.coucou.CoucouTab
import androidx.lifecycle.compose.LifecycleResumeEffect
import com.agupta07505.smartisland.coucou.phone.PhoneSettings
import com.agupta07505.smartisland.coucou.ui.MochiEmerald
import com.agupta07505.smartisland.coucou.ui.MochiGlyph
import com.agupta07505.smartisland.coucou.ui.MochiMood
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

data class AssistUi(
    val lines: List<ChatLine> = emptyList(),
    val busy: Boolean = false,
    /** Which brain answered last: "laptop", "este teléfono · nube"… shown as a badge. */
    val badge: String = "",
    val notice: String = "",
    /** The on-device model: not there, being downloaded (with a percentage), or ready. */
    val model: ModelState = ModelState.NotDownloaded,
    val modelPercent: Int = -1,
    val modelBusy: Boolean = false,
    /** What Jinx is waiting for Alan to allow (on the server), oldest first. */
    val approvals: List<JinxApproval> = emptyList(),
    /** The key of the approval whose answer is on its way. */
    val answering: String? = null,
)

class AssistModel(app: Application) : AndroidViewModel(app) {
    private val store = ThreadStore.of(app)
    private val personas = PersonaStore(File(app.filesDir, "jinx-persona.json"))
    private val secrets = KeystoreSecrets(app)
    private val deviceId = PhoneSettings.deviceId(app)
    // Jinx's gateway on the server first; the laptop's only when the server fails (see Failover).
    private val laptopApi = FailoverChatApi(
        server = LaptopChatApi(app) { PhoneSettings.profile(app, PhoneSettings.SERVER) },
        laptop = LaptopChatApi(app) { PhoneSettings.profile(app, PhoneSettings.LAPTOP) },
    )
    // Its own connections: a `send` can hold the chat's for minutes while Jinx waits for Alan's permission,
    // and this is how that permission is seen and given meanwhile.
    private val approvalsApi = FailoverChatApi(
        server = LaptopChatApi(app) { PhoneSettings.profile(app, PhoneSettings.SERVER) },
        laptop = LaptopChatApi(app) { PhoneSettings.profile(app, PhoneSettings.LAPTOP) },
    )
    private val shown = MutableStateFlow(false)
    private val modelStore = ModelStore(File(app.filesDir, "models"), GemmaModel.spec)
    private val localBrain = LocalModelBrain(
        modelReady = { modelStore.state() == ModelState.Ready },
        persona = { personas.load() },
        factory = {
            LiteRtRuntime(modelStore.modelFile().path, File(app.cacheDir, "litert").apply { mkdirs() }.path)
        },
    )
    private val repo = ChatRepository(
        store, deviceId, laptopApi,
        // Laptop first (in the repository), then OpenRouter from this phone, then the model that lives here.
        listOf(OpenRouterBrain(secrets, { personas.load() }), localBrain),
        TelegramMirror(secrets),
    )
    private val appContext = app

    private val _ui = MutableStateFlow(AssistUi(lines = store.all(), model = modelStore.state()))
    val ui: StateFlow<AssistUi> = _ui.asStateFlow()

    init {
        viewModelScope.launch {
            ModelDownloads.percent.collect { p ->
                _ui.update { it.copy(modelBusy = p >= 0, modelPercent = p) }
            }
        }
        // Jinx's questions are looked for while one of this phone's messages waits for her (every 3 s), and
        // while her tab is on screen (every 10 s, for those asked from the PC). Otherwise nothing runs.
        viewModelScope.launch(Dispatchers.IO) {
            kotlinx.coroutines.flow.combine(shown, _ui.map { it.busy }.distinctUntilChanged()) { s, b -> s to b }
                .collectLatest { (visible, busy) ->
                    while (visible || busy) {
                        pollApprovals()
                        kotlinx.coroutines.delay(if (busy) 3_000 else 10_000)
                    }
                }
        }
    }

    /** The tab is on screen (or not): its questions are looked for only then, and while a message waits. */
    fun setShown(visible: Boolean) {
        shown.value = visible
        com.agupta07505.smartisland.coucou.presence.JinxChatScreen.visible = visible
    }

    private fun pollApprovals() {
        // A failed look keeps what was shown: the next one says whether it is still there.
        val items = runCatching { approvalsApi.approvals() }.getOrNull() ?: return
        _ui.update { ui -> ui.copy(approvals = items.sortedBy { it.ts }) }
    }

    /**
     * Alan's answer to one of Jinx's questions. Allowing has already been confirmed with the fingerprint (or
     * the screen lock) by the card; [choice] is once, session or deny, never always from the phone.
     */
    fun answerApproval(a: JinxApproval, choice: String) {
        if (choice !in JinxApprovalReview.buttons(a) || _ui.value.answering != null) return
        _ui.update { it.copy(answering = a.key, notice = "") }
        viewModelScope.launch(Dispatchers.IO) {
            val msg = try {
                approvalsApi.approve(a, choice)
                if (choice == "deny") "Le dije a Jinx que no." else "Permiso dado a Jinx."
            } catch (e: ChatException) {
                "No se pudo contestar: ${e.message}"
            }
            _ui.update { ui -> ui.copy(answering = null, notice = msg, approvals = ui.approvals.filter { it.key != a.key }) }
            pollApprovals()
        }
    }

    /** Only one background round with the laptop at a time (opening the panel again must not stack them). */
    private val warming = AtomicBoolean(false)

    /** Every write goes through [update], so two coroutines can never overwrite each other's field. */
    private fun refresh(change: (AssistUi) -> AssistUi = { it }) {
        _ui.update { change(it).copy(lines = store.all(), model = modelStore.state()) }
    }

    /**
     * The panel came to the front (first time, or again while the activity was alive): bring the laptop's
     * lines, retry what was left pending and repeat unmirrored lines to Telegram. Never blocks typing.
     */
    fun onOpen() {
        refresh()
        if (!warming.compareAndSet(false, true)) return
        viewModelScope.launch(Dispatchers.IO) {
            try {
                if (personas.isStale()) runCatching { refreshPersona(laptopApi, personas) }
                repo.sync()
                refresh()
                repo.retryPending()
                repo.flushMirror()
            } finally {
                warming.set(false)
                refresh()
            }
        }
    }

    fun send(text: String) {
        val t = text.trim()
        if (t.isEmpty() || _ui.value.busy) return
        _ui.update { it.copy(busy = true, notice = "") }
        viewModelScope.launch(Dispatchers.IO) {
            // What the user typed shows up at once; the answer arrives when some brain has it.
            val line = repo.enqueue(t)
            refresh()
            val r = repo.answer(line)
            // Not looking at her chat any more: her reply is on the island (the gateway sent it there), now.
            if (!shown.value) com.agupta07505.smartisland.coucou.presence.MochiPresence.pokeNow(appContext)
            refresh { ui ->
                when (r) {
                    is SendResult.Answered -> ui.copy(
                        busy = false,
                        badge = when (r.via) {
                            Via.Laptop -> "laptop"
                            Via.Cloud -> "este teléfono · nube"
                            Via.Local -> "este teléfono · sin conexión"
                        },
                    )
                    is SendResult.Pending -> ui.copy(
                        busy = false,
                        notice = "Sin respuesta: ${r.reason}. Queda pendiente y se reintenta al abrir.",
                    )
                }
            }
        }
    }

    /** Explicit sync: always refreshes the persona, then the same round as opening the panel. */
    fun syncNow() {
        if (_ui.value.busy) return
        _ui.update { it.copy(busy = true, notice = "") }
        viewModelScope.launch(Dispatchers.IO) {
            val persona = runCatching { refreshPersona(laptopApi, personas) }
            val added = repo.sync()
            repo.retryPending()
            repo.flushMirror()
            refresh {
                it.copy(
                    busy = false,
                    notice = if (persona.isSuccess) "Sincronizado ($added mensajes nuevos)." else "La laptop no responde; sigo con lo que tengo.",
                )
            }
        }
    }

    /** One time: the laptop gateway hands this phone the OpenRouter key and Telegram bot (needs ALLOW_PROVISION there). */
    fun provision() {
        viewModelScope.launch(Dispatchers.IO) {
            val msg = try {
                provisionFrom(laptopApi, secrets, personas)
                "Teléfono provisionado desde la laptop."
            } catch (e: ChatException) {
                "No se pudo: ${e.message}"
            }
            refresh { it.copy(notice = msg) }
        }
    }

    /** Downloads the on-device model (2.6 GB) on an unmetered network; keeps going if the panel is closed. */
    fun downloadModel() {
        if (_ui.value.modelBusy || ModelDownloads.percent.value >= 0 || modelStore.state() == ModelState.Ready) return
        if (!isUnmetered(appContext)) {
            refresh { it.copy(notice = "Conéctate a Wi-Fi para bajar el modelo local (2.6 GB).") }
            return
        }
        _ui.update { it.copy(modelBusy = true, modelPercent = 0, notice = "") }
        ModelDownloads.percent.value = 0
        ModelDownloads.scope.launch {
            val spec = GemmaModel.spec
            // Each cut continues from the bytes on disk, and the Wi-Fi gate is asked before every attempt.
            val (outcome, reason) = runDownloadAttempts(
                maxTries = MAX_DOWNLOAD_TRIES,
                networkOk = { isUnmetered(appContext) },
                attempt = {
                    modelStore.download(
                        open = { from -> openModelStream(spec.url, from) },
                        onProgress = { bytes ->
                            val pct = (bytes * 100 / spec.sizeBytes).toInt()
                            if (pct != ModelDownloads.percent.value) ModelDownloads.percent.value = pct
                        },
                    )
                },
                onRetry = { n, e ->
                    refresh { it.copy(notice = "Descarga interrumpida (${e.message ?: e.javaClass.simpleName}); reintento $n de $MAX_DOWNLOAD_TRIES.") }
                },
                pause = { kotlinx.coroutines.delay(it) },
            )
            val msg = if (outcome == DownloadOutcome.Done) "Modelo local listo." else reason ?: "Descarga interrumpida."
            ModelDownloads.percent.value = -1
            refresh { it.copy(modelBusy = false, modelPercent = -1, notice = msg) }
        }
    }

    /** Runs one short question on the on-device model, so the offline brain can be checked on its own. */
    fun testLocalModel() {
        if (_ui.value.modelBusy) return
        _ui.update { it.copy(modelBusy = true, notice = "Cargando el modelo local…") }
        viewModelScope.launch(Dispatchers.IO) {
            val t = android.os.SystemClock.elapsedRealtime()
            val msg = try {
                val probe = listOf(ChatLine("probe:me", 0.0, deviceId, "Di hola en una frase corta.", null, false))
                val text = localBrain.reply(probe)
                "Modelo local (${(android.os.SystemClock.elapsedRealtime() - t) / 1000.0} s): $text"
            } catch (e: ChatException) {
                e.message ?: "El modelo local no respondió."
            }
            refresh { it.copy(modelBusy = false, notice = msg) }
        }
    }

    override fun onCleared() {
        localBrain.release()
        laptopApi.close()
        approvalsApi.close()
        super.onCleared()
    }
}

/**
 * The assistant's entry (the side key, Digital assistant app): the Coucou app opened on Jinx's tab, so the
 * pendientes and the settings are one tap away.
 */
class JinxAssistActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { CoucouHome(initial = CoucouTab.Jinx) }
    }
}

/** Jinx's tab with its model: brings the laptop's lines and retries what was pending each time it is shown. */
@Composable
internal fun AssistScreen(model: AssistModel, modifier: Modifier = Modifier) {
    val ui by model.ui.collectAsState()
    val agents by CoucouRuntime.agentUi.collectAsState()
    LifecycleResumeEffect(model) {
        model.onOpen()
        model.setShown(true)
        onPauseOrDispose { model.setShown(false) }
    }
    AssistContent(
        ui = ui,
        sessions = agents.sessions,
        onSend = { model.send(it) },
        onSync = { model.syncNow() },
        onProvision = { model.provision() },
        onDownloadModel = { model.downloadModel() },
        onTestModel = { model.testLocalModel() },
        onApproval = { a, choice -> model.answerApproval(a, choice) },
        modifier = modifier,
    )
}

private const val MAX_DOWNLOAD_TRIES = 30

/** Downloads outlive the panel: they run in the process, not in the screen's view model. */
internal object ModelDownloads {
    val scope = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.SupervisorJob() + Dispatchers.IO)

    /** -1 when nothing is downloading, else the percentage; shared by every panel that is opened meanwhile. */
    val percent = MutableStateFlow(-1)
}

private fun isUnmetered(context: android.content.Context): Boolean {
    val cm = context.getSystemService(android.content.Context.CONNECTIVITY_SERVICE) as android.net.ConnectivityManager
    val caps = cm.getNetworkCapabilities(cm.activeNetwork) ?: return false
    return caps.hasCapability(android.net.NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
}

/** A stream of the model starting at byte [from] (HTTP Range). If the server ignores Range, the prefix is skipped. */
private fun openModelStream(url: String, from: Long): java.io.InputStream {
    val c = java.net.URL(url).openConnection() as java.net.HttpURLConnection
    c.connectTimeout = 15_000
    c.readTimeout = 60_000
    if (from > 0) c.setRequestProperty("Range", "bytes=$from-")
    c.connect()
    val code = c.responseCode
    if (code != 200 && code != 206) {
        c.disconnect()
        throw java.io.IOException("HTTP $code")
    }
    val input = c.inputStream
    if (from > 0 && code == 200) {
        var left = from
        while (left > 0) {
            val n = input.skip(left)
            if (n <= 0) break
            left -= n
        }
    }
    return input
}
