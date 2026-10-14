/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import android.app.KeyguardManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.runtime.mutableStateOf
import com.agupta07505.smartisland.coucou.CoucouRuntime
import com.agupta07505.smartisland.coucou.agents.AgentUi
import com.agupta07505.smartisland.coucou.agents.SessionState
import com.agupta07505.smartisland.coucou.presence.DeviceActivity
import com.agupta07505.smartisland.di.SmartIslandRepositories
import com.agupta07505.smartisland.model.IslandMode
import com.agupta07505.smartisland.model.IslandNotification
import kotlinx.coroutines.delay

/** What Mochi is reacting to right now: the music on the device, news, low battery, or a hello. */
data class MochiScene(
    /** Something is playing: he dances. */
    val musicPlaying: Boolean = false,
    /** News arrived a moment ago: first he is surprised… */
    val surprised: Boolean = false,
    /** …then he keeps watching the main pill for a while. */
    val glancing: Boolean = false,
    /** The battery is low: he is tired. */
    val batteryLow: Boolean = false,
    /** The phone was just unlocked: he says hello. */
    val greeted: Boolean = false,
    /** The task list just emptied: he celebrates. */
    val celebrating: Boolean = false,
    /** Claude Code is working on the laptop: the same blue dots he wears there. */
    val working: Boolean = false,
    /** Jinx is saying something right now, in this mood: it is her state he shows. */
    val jinxMood: MochiMood? = null,
    /** Claude Code just finished what it was doing: he is glad (a short happy moment). */
    val finished: Boolean = false,
    /** A tool of Claude Code just failed: flat eyes and a red dot for a moment (as on the laptop). */
    val errored: Boolean = false,
    /** What Claude Code is doing is a search (Grep, Glob, the web): indigo dots, as on the laptop. */
    val searching: Boolean = false,
    /** Claude Code got a prompt and has not used a tool yet: violet dots, as on the laptop. */
    val thinking: Boolean = false,
)

/**
 * How Mochi reads the room. A newcomer among the island's notifications (anything but his own
 * cards) surprises him for a few seconds; music playing anywhere makes him dance. The logic is
 * pure so it can be tried without a phone.
 */
object MochiReactions {
    /** How long after a notification's arrival it still counts as news. */
    const val FRESH_MS = 6_000L

    /** How long the pop lasts: a beat, not a stare (the glance carries the rest). */
    const val SURPRISE_MS = 1_800L

    /** How long the hello lasts after an unlock. */
    const val GREET_MS = 4_000L

    /** How long he keeps watching the main pill after the pop passes. */
    const val GLANCE_MS = 4_000L

    /** How long the celebration lasts after the list empties. */
    const val CELEBRATE_MS = 3_000L

    /** How long he stays glad after Claude Code finishes. */
    const val FINISHED_MS = 3_000L

    /** How long a failed tool shows on his face. */
    const val ERROR_MS = 3_000L

    /** The tools that read as "searching" (the laptop's Mochi scans with his eyes for these). */
    val SEARCH_TOOLS = setOf("Grep", "Glob", "WebSearch", "WebFetch")

    /**
     * Jinx's own card on the island ("mochi:<Mood>:<kind>") gives his face its mood while it is there;
     * null when she is not saying anything. Her messages carry the mood the server sent.
     */
    fun jinxMood(notifications: List<IslandNotification>): MochiMood? =
        notifications.firstOrNull { it.mode == IslandMode.JinxMsg }
            ?.let { com.agupta07505.smartisland.coucou.presence.jinxMoodOf(it.category) }

    fun isMusicPlaying(notifications: List<IslandNotification>): Boolean =
        notifications.any { it.mode == IslandMode.Music && it.mediaIsPlaying }

    /**
     * Low battery, as the system itself announces it (category "battery_low"), or anything at 20%
     * or less just in case. Same parsing as the island's battery glyph, so both agree on "low".
     */
    fun isBatteryLow(notifications: List<IslandNotification>): Boolean =
        notifications.any { n ->
            n.mode == IslandMode.Battery && (
                n.category == "battery_low" ||
                    n.title.contains("low", ignoreCase = true) ||
                    (n.text.replace("%", "").trim().toFloatOrNull()?.let { it <= 20f } == true)
                )
        }

    /**
     * News: a fresh notification that is not one of Mochi's own cards. His own cards (desk, agent,
     * approvals, questions) already carry a Mochi reacting to them — surprising him for those too
     * would double the reaction.
     */
    fun freshArrival(notifications: List<IslandNotification>, now: Long, windowMs: Long = FRESH_MS): Boolean =
        notifications.any { n ->
            n.mode != IslandMode.Desk && n.mode != IslandMode.Empty &&
                !n.key.startsWith("coucou_") && n.timeMillis >= now - windowMs
        }

    /**
     * News latches the surprise open; silence lets it close. It never moves backwards, because the
     * notification list is re-emitted on every change and a recompute must not shorten a surprise
     * that is already showing.
     */
    fun latchSurprise(previousUntil: Long, notifications: List<IslandNotification>, now: Long): Long =
        if (freshArrival(notifications, now)) maxOf(previousUntil, now + SURPRISE_MS) else previousUntil

    /** Same latch shape for the unlock hello: every unlock re-opens it, never shortens it. */
    fun latchGreeting(previousUntil: Long, now: Long): Long = maxOf(previousUntil, now + GREET_MS)

    /**
     * The second half of news: while the surprise is the pop, the glance is the stare that
     * follows it on the main pill. Latched separately so it outlives the surprise.
     */
    fun latchGlance(previousUntil: Long, notifications: List<IslandNotification>, now: Long): Long =
        if (freshArrival(notifications, now)) maxOf(previousUntil, now + GLANCE_MS) else previousUntil

    /**
     * The list went from something to nothing: open the celebration. Any other change (or none)
     * keeps whatever was latched, so only an actual emptying triggers it.
     */
    fun latchCelebration(previousUntil: Long, previousOpen: Int, open: Int, now: Long): Long =
        if (previousOpen > 0 && open == 0) now + CELEBRATE_MS else previousUntil

    /** Night (00:00–05:59 local): he naps through calm tasks, overdue ones still wake him. */
    fun isNight(nowMs: Long = System.currentTimeMillis()): Boolean {
        val hour = java.util.Calendar.getInstance().apply { timeInMillis = nowMs }.get(java.util.Calendar.HOUR_OF_DAY)
        return hour in 0..5
    }

    fun scene(
        notifications: List<IslandNotification>,
        now: Long,
        surprisedUntil: Long,
        greetedUntil: Long,
        glanceUntil: Long,
        celebratedUntil: Long,
        peerMusic: Boolean = false,
        working: Boolean = false,
        finishedUntil: Long = 0L,
        errorUntil: Long = 0L,
        peerLimit: Boolean = false,
        searching: Boolean = false,
        thinking: Boolean = false,
        jinxMood: MochiMood? = null,
    ): MochiScene =
        MochiScene(
            jinxMood = jinxMood,
            errored = errorUntil > now,
            searching = working && searching,
            thinking = working && thinking && !searching,
            // The other device's music counts too: whichever one Mochi is on, he dances to it.
            musicPlaying = isMusicPlaying(notifications) || peerMusic,
            working = working,
            finished = finishedUntil > now,
            surprised = surprisedUntil > now,
            glancing = glanceUntil > now,
            // Rate-limited on the laptop reads here as the same tired face with the bead of sweat.
            batteryLow = isBatteryLow(notifications) || peerLimit,
            greeted = greetedUntil > now,
            celebrating = celebratedUntil > now,
        )
}

/**
 * Every unlock's timestamp, watched live. One receiver serves every caller (the hello and the
 * padlock), all registered on the application context so they work from the overlay service.
 * Starts at 0 (no unlock seen yet this process).
 *
 * Two signals: USER_PRESENT (a dismissed lockscreen) and SCREEN_ON while nothing is locked
 * (this phone often lives on Smart Lock trust, so waking it dismisses no keyguard and emits no
 * USER_PRESENT — without the second signal the greeting would never show here).
 */
@Composable
fun rememberUnlockMs(): Long {
    val app = LocalContext.current.applicationContext
    val keyguard = remember(app) { app.getSystemService(KeyguardManager::class.java) }
    var unlockMs by remember { mutableLongStateOf(0L) }
    DisposableEffect(app) {
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(c: Context?, intent: Intent?) {
                android.util.Log.d("MochiUnlock", "broadcast=${intent?.action} locked=${keyguard?.isDeviceLocked}")
                val unlock = intent?.action == Intent.ACTION_USER_PRESENT ||
                    (intent?.action == Intent.ACTION_SCREEN_ON && keyguard?.isDeviceLocked == false)
                if (unlock) {
                    unlockMs = System.currentTimeMillis()
                    android.util.Log.d("MochiUnlock", "latched unlockMs=$unlockMs")
                }
            }
        }
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_USER_PRESENT)
            addAction(Intent.ACTION_SCREEN_ON)
        }
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            app.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            app.registerReceiver(receiver, filter)
        }
        onDispose { runCatching { app.unregisterReceiver(receiver) } }
    }
    return unlockMs
}

/**
 * The room, watched live: what the island holds, whether the surprise/hello should still show.
 * Mochi greets every time the phone wakes up (the padlock badge uses the same unlock moment).
 */
@Composable
fun rememberMochiScene(): MochiScene {
    val context = LocalContext.current
    val app = context.applicationContext
    val repo = remember(context) { SmartIslandRepositories.notificationRepository(app) }
    val notifications by repo.notifications.collectAsState()
    var surprisedUntil by remember { mutableLongStateOf(0L) }
    var greetedUntil by remember { mutableLongStateOf(0L) }
    var glanceUntil by remember { mutableLongStateOf(0L) }

    // Latch on every list change (a newcomer extends the surprise and re-opens the stare)…
    LaunchedEffect(notifications) {
        val now = System.currentTimeMillis()
        surprisedUntil = MochiReactions.latchSurprise(surprisedUntil, notifications, now)
        glanceUntil = MochiReactions.latchGlance(glanceUntil, notifications, now)
    }
    // …and close each once its time runs out, so the glyph goes back to calm without new input.
    LaunchedEffect(surprisedUntil) {
        val remaining = surprisedUntil - System.currentTimeMillis()
        if (remaining > 0) {
            delay(remaining)
            surprisedUntil = 0L
        }
    }
    LaunchedEffect(glanceUntil) {
        val remaining = glanceUntil - System.currentTimeMillis()
        if (remaining > 0) {
            delay(remaining)
            glanceUntil = 0L
        }
    }
    // The other device's news surprises him here as well (he is only drawn on one of them).
    val peerNews by DeviceActivity.peerNews.collectAsState()
    LaunchedEffect(peerNews) {
        if (peerNews) {
            val now = System.currentTimeMillis()
            surprisedUntil = maxOf(surprisedUntil, now + MochiReactions.SURPRISE_MS)
            glanceUntil = maxOf(glanceUntil, now + MochiReactions.GLANCE_MS)
        }
    }
    // (Claude Code's work is not shown on him: the Mochi of the island is Jinx's, and its own card on the
    // main pill says what Claude Code is doing. Only what Jinx herself says moves his face.)
    val peerMusic by DeviceActivity.peerMusic.collectAsState()
    // Every unlock re-opens the hello (same moment the padlock badge shows)…
    val unlockMs = rememberUnlockMs()
    LaunchedEffect(unlockMs) {
        if (unlockMs > 0) greetedUntil = MochiReactions.latchGreeting(greetedUntil, unlockMs)
    }
    // …and it too closes by itself.
    LaunchedEffect(greetedUntil) {
        val remaining = greetedUntil - System.currentTimeMillis()
        if (remaining > 0) {
            delay(remaining)
            greetedUntil = 0L
        }
    }
    return MochiReactions.scene(
        notifications, System.currentTimeMillis(), surprisedUntil, greetedUntil, glanceUntil, 0L,
        peerMusic = peerMusic,
        jinxMood = MochiReactions.jinxMood(notifications),
    )
}

/**
 * The celebration, watched live: the open count dropping to zero opens it, time closes it.
 * Kept outside [rememberMochiScene] because the count lives with the tasks, not the island.
 */
@Composable
fun rememberCelebration(open: Int): Boolean {
    var celebratedUntil by remember { mutableLongStateOf(0L) }
    var previousOpen by remember { mutableIntStateOf(open) }
    LaunchedEffect(open) {
        celebratedUntil = MochiReactions.latchCelebration(celebratedUntil, previousOpen, open, System.currentTimeMillis())
        previousOpen = open
    }
    LaunchedEffect(celebratedUntil) {
        val remaining = celebratedUntil - System.currentTimeMillis()
        if (remaining > 0) {
            delay(remaining)
            celebratedUntil = 0L
        }
    }
    return celebratedUntil > System.currentTimeMillis()
}
