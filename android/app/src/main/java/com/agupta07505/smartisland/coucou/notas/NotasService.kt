package com.agupta07505.smartisland.coucou.notas

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.MediaRecorder
import android.net.ConnectivityManager
import android.net.Network
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.os.PowerManager
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import com.agupta07505.smartisland.MainActivity
import com.agupta07505.smartisland.R
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.io.File

/** What the Notas screen shows; the service writes it. */
data class NotasEstado(
    val grabando: Boolean = false,
    /** SystemClock.elapsedRealtime() when the recording began. */
    val inicio: Long = 0,
    val pendientes: Int = 0,
    val marcas: Int = 0,
    val error: String? = null,
    val segundoMicActivo: Boolean = false,
)

object NotasBus {
    private val _estado = MutableStateFlow(NotasEstado())
    val estado: StateFlow<NotasEstado> = _estado.asStateFlow()
    fun update(f: (NotasEstado) -> NotasEstado) { _estado.value = f(_estado.value) }
}

/** One queue per process, shared by the service and the screen. */
object NotasCola {
    @Volatile private var q: UploadQueue? = null
    fun get(context: Context): UploadQueue = q ?: synchronized(this) {
        q ?: UploadQueue(File(context.applicationContext.filesDir, "cola").also { it.mkdirs() }, NotasApi()).also { q = it }
    }
}

/**
 * Records a class in the background (foreground service, microphone type) in 10 s AAC segments, and uploads them as it goes.
 * Segments are closed and a new one opened every 10 s, so there is a gap of a few milliseconds between two files.
 */
class NotasService : Service() {
    private lateinit var work: HandlerThread
    private lateinit var handler: Handler
    private lateinit var up: HandlerThread
    private lateinit var upHandler: Handler
    private var wake: PowerManager.WakeLock? = null
    private var netCb: ConnectivityManager.NetworkCallback? = null

    private var dir: File? = null
    private var idx = 0
    private var rec1: MediaRecorder? = null
    private var rec2: MediaRecorder? = null
    private var file1: File? = null
    private var file2: File? = null
    private var inicio = 0L
    private var marks = 0
    private var recording = false
    private var finishing = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        work = HandlerThread("notas-rec").also { it.start() }
        handler = Handler(work.looper)
        up = HandlerThread("notas-up").also { it.start() }
        upHandler = Handler(up.looper)
        val ch = NotificationChannel(CHANNEL, getString(R.string.notas_channel), NotificationManager.IMPORTANCE_LOW)
        getSystemService(NotificationManager::class.java).createNotificationChannel(ch)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START -> start(intent)
            ACTION_STOP -> handler.post { stopRecording() }
            ACTION_MARK -> handler.post { mark() }
            else -> if (!recording && !finishing) {
                // Sticky restart after the process died: a microphone service cannot be started from the background,
                // so the recording is gone. Send what is on disk and leave.
                finishing = true
                NotasCola.get(this).clearStale()
                watchNetwork()
                kick()
            }
        }
        return START_STICKY
    }

    private fun start(intent: Intent) {
        if (recording) return
        val granted = ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        if (!granted) {
            NotasBus.update { it.copy(error = getString(R.string.notas_perm_denied)) }
            NotasTileService.refresh(this)
            stopSelf()
            return
        }
        val materia = intent.getStringExtra(EXTRA_MATERIA).orEmpty()
        val titulo = intent.getStringExtra(EXTRA_TITULO).orEmpty()
        val segundo = intent.getBooleanExtra(EXTRA_SEGUNDO, false)
        if (materia.isNotBlank()) NotasArranque.recordar(this, materia)
        inicio = SystemClock.elapsedRealtime()
        marks = 0
        idx = 0
        finishing = false

        // Without a session id yet (no network) the folder is "pend-…" and the queue creates the session later.
        dir = NotasCola.get(this).newSessionDir().also {
            File(it, "meta.txt").writeText(materia + "\n" + titulo + "\n")
            File(it, UploadQueue.ACTIVA).writeText("1")
        }
        startForegroundCompat(buildNotification(true))
        wake = (getSystemService(POWER_SERVICE) as PowerManager)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "coucou:notas").apply { acquire(6 * 60 * 60 * 1000L) }
        watchNetwork()
        recording = true
        NotasBus.update { NotasEstado(grabando = true, inicio = inicio, pendientes = NotasCola.get(this).pendingCount()) }
        NotasTileService.refresh(this)
        handler.post { openSegment(segundo) }
        kick()
        handler.postDelayed(object : Runnable {
            override fun run() { if (recording) { notify(buildNotification(true)); handler.postDelayed(this, 5000) } }
        }, 5000)
    }

    private var segundoMic = false
    private val kickQueued = java.util.concurrent.atomic.AtomicBoolean(false)

    private fun openSegment(segundo: Boolean) {
        segundoMic = segundo
        val d = dir ?: return
        file1 = File(d, UploadQueue.chunkName(idx, 1) + ".part")
        rec1 = newRecorder(MediaRecorder.AudioSource.MIC, file1!!)
        if (rec1 == null) {
            NotasBus.update { it.copy(error = getString(R.string.notas_rec_failed)) }
            stopRecording()
            return
        }
        if (segundo) {
            file2 = File(d, UploadQueue.chunkName(idx, 2) + ".part")
            rec2 = newRecorder(MediaRecorder.AudioSource.CAMCORDER, file2!!)
            if (rec2 == null) file2 = null // many phones refuse two inputs at once: keep track 1 only
        }
        NotasBus.update { it.copy(segundoMicActivo = rec2 != null) }
        handler.postDelayed(rotate, SEGMENT_MS)
    }

    private val rotate = Runnable {
        if (!recording) return@Runnable
        closeSegment()
        idx++
        openSegment(segundoMic)
        kick()
    }

    private fun newRecorder(source: Int, out: File): MediaRecorder? {
        val r = if (Build.VERSION.SDK_INT >= 31) MediaRecorder(this) else @Suppress("DEPRECATION") MediaRecorder()
        return try {
            r.setAudioSource(source)
            r.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
            r.setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
            r.setAudioSamplingRate(16000)
            r.setAudioChannels(1)
            r.setAudioEncodingBitRate(64000)
            r.setOutputFile(out.absolutePath)
            r.prepare()
            r.start()
            r
        } catch (e: Exception) {
            runCatching { r.release() }
            out.delete()
            null
        }
    }

    /** Closes the current files and renames them so the queue sees them. */
    private fun closeSegment() {
        fun end(r: MediaRecorder?, f: File?) {
            if (r == null || f == null) return
            val ok = runCatching { r.stop() }.isSuccess
            runCatching { r.release() }
            if (ok && f.length() > 0) f.renameTo(File(f.parentFile, f.name.removeSuffix(".part"))) else f.delete()
        }
        end(rec1, file1); end(rec2, file2)
        rec1 = null; rec2 = null; file1 = null; file2 = null
    }

    private fun mark() {
        if (!recording) return
        val t = ((SystemClock.elapsedRealtime() - inicio) / 1000).toInt()
        val d = dir ?: return
        File(d, "%06d.mark".format(++marks)).writeText(t.toString())
        NotasBus.update { it.copy(marcas = marks) }
        kick()
    }

    private fun stopRecording() {
        if (!recording) return
        recording = false
        handler.removeCallbacksAndMessages(null)
        closeSegment()
        dir?.let {
            File(it, UploadQueue.TERMINAR).writeText("1")
            File(it, UploadQueue.ACTIVA).delete()
        }
        NotasBus.update { it.copy(grabando = false, segundoMicActivo = false) }
        NotasTileService.refresh(this)
        wake?.let { if (it.isHeld) it.release() }
        wake = null
        finishing = true
        notify(buildNotification(false))
        kick()
    }

    /** Upload on its own thread; when nothing is left and we are not recording, the service ends. */
    private fun kick() {
        if (!kickQueued.compareAndSet(false, true)) return
        upHandler.post {
            kickQueued.set(false)
            val q = NotasCola.get(this)
            val ok = q.drain { publishPending(q) }
            publishPending(q)
            if (ok && !recording && q.pendingCount() == 0) stopSelfFromWork()
        }
    }

    private fun publishPending(q: UploadQueue) {
        NotasBus.update { it.copy(pendientes = q.pendingCount()) }
        notify(buildNotification(recording))
    }

    private fun stopSelfFromWork() {
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun watchNetwork() {
        if (netCb != null) return
        val cm = getSystemService(ConnectivityManager::class.java)
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) { kick() }
        }
        netCb = cb
        runCatching { cm.registerDefaultNetworkCallback(cb) }
    }

    private fun buildNotification(rec: Boolean): Notification {
        val pend = NotasBus.estado.value.pendientes
        fun pi(action: String, code: Int) = PendingIntent.getService(
            this, code, Intent(this, NotasService::class.java).setAction(action),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val b = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_smart_island)
            .setContentTitle(getString(if (rec) R.string.notas_notif_recording else R.string.notas_notif_uploading))
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
        if (pend > 0) b.setContentText(getString(R.string.notas_notif_pending, pend))
        if (rec) {
            b.setUsesChronometer(true)
                .setWhen(System.currentTimeMillis() - (SystemClock.elapsedRealtime() - inicio))
                .addAction(0, getString(R.string.notas_stop), pi(ACTION_STOP, 1))
                .addAction(0, getString(R.string.notas_mark), pi(ACTION_MARK, 2))
        }
        return b.build()
    }

    private fun startForegroundCompat(n: Notification) {
        val type = if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0
        ServiceCompat.startForeground(this, NOTIF_ID, n, type)
    }

    private fun notify(n: Notification) = getSystemService(NotificationManager::class.java).notify(NOTIF_ID, n)

    override fun onDestroy() {
        recording = false
        handler.removeCallbacksAndMessages(null)
        runCatching { closeSegment() }
        netCb?.let { runCatching { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(it) } }
        wake?.let { if (it.isHeld) it.release() }
        work.quitSafely()
        up.quitSafely()
        NotasBus.update { it.copy(grabando = false) }
        NotasTileService.refresh(this)
        super.onDestroy()
    }

    companion object {
        const val CHANNEL = "notas"
        const val NOTIF_ID = 5281
        const val SEGMENT_MS = 10_000L
        const val ACTION_START = "com.agupta07505.smartisland.notas.START"
        const val ACTION_STOP = "com.agupta07505.smartisland.notas.STOP"
        const val ACTION_MARK = "com.agupta07505.smartisland.notas.MARK"
        const val EXTRA_MATERIA = "materia"
        const val EXTRA_TITULO = "titulo"
        const val EXTRA_SEGUNDO = "segundo"

        fun start(context: Context, materia: String, titulo: String, segundoMic: Boolean) {
            val i = Intent(context, NotasService::class.java).setAction(ACTION_START)
                .putExtra(EXTRA_MATERIA, materia).putExtra(EXTRA_TITULO, titulo).putExtra(EXTRA_SEGUNDO, segundoMic)
            ContextCompat.startForegroundService(context, i)
        }

        fun send(context: Context, action: String) {
            context.startService(Intent(context, NotasService::class.java).setAction(action))
        }
    }
}
