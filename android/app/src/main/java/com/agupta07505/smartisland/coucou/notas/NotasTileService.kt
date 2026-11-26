package com.agupta07505.smartisland.coucou.notas

import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.widget.Toast
import com.agupta07505.smartisland.MainActivity
import com.agupta07505.smartisland.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Quick-settings tile: one tap starts recording the class that is on now (or the last subject used),
 * another stops it. While recording the tile stays active, even with the screen off.
 */
class NotasTileService : TileService() {

    override fun onStartListening() {
        paint()
    }

    override fun onClick() {
        if (NotasBus.estado.value.grabando) {
            NotasService.send(this, NotasService.ACTION_STOP)
            return
        }
        if (!NotasArranque.tieneMicrofono(this)) {
            Toast.makeText(this, R.string.notas_tile_need_permission, Toast.LENGTH_LONG).show()
            openNotes()
            return
        }
        Toast.makeText(this, R.string.notas_tile_starting, Toast.LENGTH_SHORT).show()
        scope.launch {
            val started = withContext(Dispatchers.IO) { NotasArranque.iniciarConDeteccion(applicationContext) }
            if (!started) {
                Toast.makeText(applicationContext, R.string.notas_tile_no_class, Toast.LENGTH_LONG).show()
                openNotes()
            }
            paint()
        }
    }

    private fun paint() {
        val tile = qsTile ?: return
        val recording = NotasBus.estado.value.grabando
        tile.state = if (recording) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = getString(R.string.notas_tile_label)
        if (Build.VERSION.SDK_INT >= 29) {
            tile.subtitle = getString(if (recording) R.string.notas_tile_recording else R.string.notas_tile_idle)
        }
        tile.updateTile()
    }

    /** Opens the Notas screen so the user can pick a subject or grant the microphone. */
    private fun openNotes() {
        val intent = Intent(this, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        val pending = PendingIntent.getActivity(
            this, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        if (Build.VERSION.SDK_INT >= 34) {
            startActivityAndCollapse(pending)
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }

    companion object {
        /** Not tied to the tile's short life: starting the service must survive the panel closing. */
        private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

        /** Asks the system to redraw the tile (recording started or stopped). */
        fun refresh(context: Context) {
            runCatching {
                TileService.requestListeningState(context, ComponentName(context, NotasTileService::class.java))
            }
        }
    }
}
