/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.agents

import android.app.KeyguardManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.os.PowerManager
import androidx.core.app.NotificationCompat

/**
 * Jinx's permission requests as system notifications. The island only draws on a lit, unlocked screen, and Jinx can ask at any
 * hour: with the phone locked or the screen off the request would otherwise go unseen until it expired.
 *
 * It never carries the command (only the kind of action, "recursive delete"): what is being allowed is read in the island
 * sheet, after unlocking, whatever the lock screen is set to show. While the phone is in use the island is enough, so none is posted.
 */
object RequestNotifier {
    private const val CHANNEL = "jinx_requests"
    private const val TAG = "jinx-request"

    /** [asleep]: the screen is off or the keyguard is up (what [phoneAsleep] answers; a parameter so it can be tested). */
    fun wanted(p: Pending, asleep: Boolean) = p.agent == "hermes" && !p.isQuestion && asleep

    fun phoneAsleep(context: Context): Boolean =
        context.getSystemService(KeyguardManager::class.java)?.isKeyguardLocked == true ||
            context.getSystemService(PowerManager::class.java)?.isInteractive == false

    /** Posts the new ones, takes down the ones that are over. [before]: the ids seen last time; returns the ids seen now. */
    fun sync(context: Context, pending: List<Pending>, before: Set<String>): Set<String> {
        val nm = context.getSystemService(NotificationManager::class.java) ?: return before
        val mine = pending.filter { it.agent == "hermes" && !it.isQuestion }
        val ids = mine.map { it.id }.toSet()
        for (gone in before - ids) nm.cancel(TAG, gone.hashCode())
        val asleep = phoneAsleep(context)
        val fresh = mine.filter { it.id !in before && wanted(it, asleep) }
        if (fresh.isEmpty()) return ids
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, "Jinx's permission requests", NotificationManager.IMPORTANCE_HIGH).apply {
                lockscreenVisibility = android.app.Notification.VISIBILITY_PRIVATE
                enableVibration(true)
            },
        )
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
        val open = launch?.let { PendingIntent.getActivity(context, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT) }
        for (p in fresh) {
            val kind = p.tool.ifBlank { "a command" }
            val publicVersion = NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_dialog_alert)
                .setContentTitle("Jinx needs your OK")
                .setContentText("Unlock to see the request")
                .build()
            val b = NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_dialog_alert)
                .setContentTitle("Jinx needs your OK")
                .setContentText(kind)
                .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
                .setPublicVersion(publicVersion)
                .setCategory(NotificationCompat.CATEGORY_MESSAGE)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setAutoCancel(true)
                .setOnlyAlertOnce(true)
                .setTimeoutAfter((p.timeoutS + AgentModel.PENDING_GRACE_S) * 1000L)
            open?.let { b.setContentIntent(it) }
            try {
                nm.notify(TAG, p.id.hashCode(), b.build())
            } catch (_: SecurityException) {
                // Notifications are not allowed on this phone: the island still shows the request.
            }
        }
        return ids
    }
}
