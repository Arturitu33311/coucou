/*
 * Smart Island (2026)
 * © Animesh Gupta — github.com/agupta07505
 * Licensed under the GNU GPL v3 License
 * Do not remove or alter this notice. - Per GPL-3.0 Section 4 & Section 5
 */

package com.agupta07505.smartisland.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import com.agupta07505.smartisland.coucou.presence.DeviceActivity
import com.agupta07505.smartisland.data.INotificationRepository
import com.agupta07505.smartisland.model.isCoucou
import com.agupta07505.smartisland.data.SmartIslandCommand
import com.agupta07505.smartisland.data.SmartIslandSettings
import com.agupta07505.smartisland.data.SmartIslandSettingsRepository
import com.agupta07505.smartisland.model.IslandMode
import com.agupta07505.smartisland.model.IslandNotification
import com.agupta07505.smartisland.model.AGENT_NEEDS_YOU
import com.agupta07505.smartisland.model.ATTENTION_PRIORITY
import com.agupta07505.smartisland.model.YIELDED_MUSIC_PRIORITY
import com.agupta07505.smartisland.model.WAITING_PRIORITY
import com.agupta07505.smartisland.model.priority
import com.agupta07505.smartisland.util.runSuspendCatchingLogged
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * What this device draws: everything on the one in use, and on the other everything but Coucou's own
 * cards (Mochi, tasks, Claude Code, Jinx). Dropping them from the drawn list changes nothing about
 * them: a request keeps its timeout and its answer.
 */
fun shownOnThisDevice(inUse: Boolean, list: List<IslandNotification>): List<IslandNotification> =
    if (inUse) list else list.filterNot { it.mode.isCoucou }

@OptIn(ExperimentalCoroutinesApi::class)
class IslandViewModel(
    private val settingsRepo: SmartIslandSettingsRepository,
    val notificationRepo: INotificationRepository
) : ViewModel() {

    val settings = settingsRepo.settings.stateIn(
        scope = viewModelScope,
        started = SharingStarted.Eagerly,
        initialValue = SmartIslandSettings.Default
    )

    val notifications = notificationRepo.notifications
    val foregroundPackage = MutableStateFlow<String?>(null)

    /**
     * Mochi and Coucou's cards belong to the device that is being used. On the other one they are
     * not drawn: nothing about them is dropped (a request keeps its timeout and its answer), the
     * island simply does not show them. Everything that is not Coucou's shows as ever.
     */
    val visibleNotifications: StateFlow<List<IslandNotification>> = combine(
        notifications,
        foregroundPackage,
        settings,
        DeviceActivity.active
    ) { list, fgPkg, s, mine ->
        arrange(shownOn(mine, list), fgPkg, s)
    }.stateIn(
        scope = viewModelScope,
        started = SharingStarted.Eagerly,
        initialValue = emptyList()
    )

    val expanded = MutableStateFlow(false)
    val selectedIndex = MutableStateFlow(0)
    val isLocked = MutableStateFlow(false)
    val isInputActive = MutableStateFlow(false)

    val mode: StateFlow<IslandMode> = combine(visibleNotifications, selectedIndex) { list, idx ->
        list.getOrNull(idx)?.mode ?: IslandMode.Empty
    }.stateIn(
        scope = viewModelScope,
        started = SharingStarted.Eagerly,
        initialValue = IslandMode.Empty
    )

    /** The selection follows the notification, not its place: the order changes as things come and go. */
    private var selectedKey: String? = null

    /** Someone chose this one (a swipe, a tap, an event that took the front): it stays until it goes away or the island folds. */
    private var pinned = false
    private var unpinJob: Job? = null

    /** A pin is a few seconds of attention, not a lease: if nobody uses the island, it goes back to the most important one. */
    private fun pin(key: String) {
        selectedKey = key
        pinned = true
        unpinJob?.cancel()
        unpinJob = viewModelScope.launch {
            delay(PIN_MS)
            if (!expanded.value) restOnTop()
        }
    }

    private fun restOnTop() {
        unpinJob?.cancel()
        pinned = false
        val list = visibleNotifications.value
        val rest = restingIndex(list)
        selectedKey = list.getOrNull(rest)?.key
        selectedIndex.value = rest
    }

    /**
     * Where the island rests: the most important thing it holds, except Mochi's desk, which has his own
     * bubble and never rests on the main pill when there is anything else (the overlay would move it off
     * the desk anyway, and every round trip showed as the pill pulsing).
     */
    private fun restingIndex(list: List<IslandNotification>): Int {
        if (list.size < 2) return 0
        val i = list.indexOfFirst { it.key != com.agupta07505.smartisland.coucou.IslandDesk.KEY }
        return if (i >= 0) i else 0
    }

    private fun shownOn(mine: Boolean, list: List<IslandNotification>) = shownOnThisDevice(mine, list)

    private fun arrange(list: List<IslandNotification>, fgPkg: String?, s: SmartIslandSettings): List<IslandNotification> =
        list.filterNot { notif ->
            (!s.enableBatteryMode && notif.mode == IslandMode.Battery) ||
            (!fgPkg.isNullOrEmpty() && notif.mode == IslandMode.Music && notif.packageName == fgPkg)
        }.let { filtered ->
            // News must surface over a playing song: a plain notification (40) otherwise hides
            // behind music (50) until it pauses, while with music off it lands on top and shows.
            val now = System.currentTimeMillis()
            val freshNews = filtered.any {
                it.mode == IslandMode.Notification && now - it.timeMillis < FRESH_NEWS_MS
            }
            filtered.sortedWith(compareByDescending<IslandNotification> { effectivePriority(it, freshNews) }.thenByDescending { it.timeMillis })
        }

    /** A paused player or a remote mirror keeps its Music card and controls, but never camps
     * above news: only music actually sounding on this phone keeps the throne, so a stale player
     * or the PC's mirror can never hide everything behind it. While fresh news (< 30 s) waits,
     * even sounding music steps aside so the news shows instead of hiding behind the song. */
    private fun effectivePriority(notif: IslandNotification, freshNews: Boolean = false) =
        if (notif.mode == IslandMode.Music && notif.mediaIsPlaying && notif.packageName != "org.kde.kdeconnect_tp" && !freshNews) {
            notif.mode.priority
        } else if (notif.mode == IslandMode.Music) {
            YIELDED_MUSIC_PRIORITY
        } else if (notif.mode == IslandMode.Agent && notif.category == AGENT_NEEDS_YOU) {
            ATTENTION_PRIORITY
        } else {
            notif.mode.priority
        }

    /**
     * A new event takes the front, as it always did, except over the person typing or over something that
     * waits for them (a call, a permission, a question, a reminder) and matters more than it does.
     */
    private fun mayTakeFront(incoming: IslandNotification): Boolean {
        val current = visibleNotifications.value.getOrNull(selectedIndex.value) ?: return true
        if (current.key == incoming.key) return true
        if (isInputActive.value) return false
        return !(current.mode.priority >= WAITING_PRIORITY && incoming.mode.priority < current.mode.priority)
    }

    private var autoCollapseJob: Job? = null

    init {
        viewModelScope.launch {
            runSuspendCatchingLogged(TAG, "Notifications collector failed") {
                visibleNotifications.collect { list ->
                    // While someone is using the island (or chose this one) stay on it; otherwise, and when it is
                    // gone, rest on the most important thing there is.
                    val keep = pinned || expanded.value
                    val kept = if (keep) selectedKey?.let { k -> list.indexOfFirst { it.key == k } } ?: -1 else -1
                    if (kept < 0) pinned = false
                    selectedIndex.value = if (kept >= 0) kept else restingIndex(list)
                    selectedKey = list.getOrNull(selectedIndex.value)?.key
                }
            }
        }
        viewModelScope.launch {
            runSuspendCatchingLogged(TAG, "Auto-expand collector failed") {
                notificationRepo.autoExpandEvent.collect { key ->
                    // Hidden ones (battery off, the music of the app on screen) are not in the arranged list.
                    val arranged = arrange(shownOn(DeviceActivity.active.value, notifications.value), foregroundPackage.value, settings.value)
                    val index = arranged.indexOfFirst { it.key == key }
                    if (index < 0 || !mayTakeFront(arranged[index])) return@collect
                    pin(key)
                    selectedIndex.value = index
                    val isDemo = key.startsWith("demo_")
                    // The switch is for other apps' notifications. What Coucou itself raises on purpose (a permission
                    // request, a question, Claude Code asking for the person, Jinx) always opens when it arrives.
                    if (settings.value.autoExpandOnNotification || isDemo || com.agupta07505.smartisland.coucou.IslandAgents.opensByItself(key)) {
                        expand()
                    }
                }
            }
        }
        viewModelScope.launch {
            runSuspendCatchingLogged(TAG, "Timer-reset collector failed") {
                notificationRepo.resetTimerEvent.collect {
                    resetAutoCollapseTimer()
                }
            }
        }
        viewModelScope.launch {
            runSuspendCatchingLogged(TAG, "Expanded-state collector failed") {
                expanded.collect { isExpanded ->
                    if (isExpanded) {
                        if (!isInputActive.value) {
                            startAutoCollapseTimer()
                        }
                    } else {
                        stopAutoCollapseTimer()
                        isInputActive.value = false
                        // Folded: the island goes back to resting on the most important thing it holds.
                        restOnTop()
                    }
                }
            }
        }
        viewModelScope.launch {
            runSuspendCatchingLogged(TAG, "Input-active collector failed") {
                isInputActive.collect { active ->
                    if (active) {
                        stopAutoCollapseTimer()
                    } else if (expanded.value) {
                        startAutoCollapseTimer()
                    }
                }
            }
        }
    }

    fun expand() {
        expanded.value = true
    }

    fun collapse() {
        isInputActive.value = false
        expanded.value = false
    }

    fun setInputActive(active: Boolean) {
        if (isInputActive.value != active) {
            isInputActive.value = active
        }
    }

    private var lastToggleTimeMs = 0L

    fun toggleExpanded() {
        val now = System.currentTimeMillis()
        if (now - lastToggleTimeMs < 350L) return
        lastToggleTimeMs = now
        expanded.update { !it }
    }

    private fun startAutoCollapseTimer() {
        autoCollapseJob?.cancel()
        if (isInputActive.value) return
        // Coucou: what waits for an answer (a permission, a question) stays open until it is answered or
        // expires; Mochi's desk and a reminder get time to be used. Everything else folds after 5 s as before.
        val waitMs = when (visibleNotifications.value.getOrNull(selectedIndex.value)?.mode) {
            IslandMode.Approval, IslandMode.Question -> 115_000L
            IslandMode.Desk, IslandMode.Reminder, IslandMode.JinxMsg -> 25_000L
            else -> AUTO_COLLAPSE_DELAY_MS
        }
        autoCollapseJob = viewModelScope.launch {
            delay(waitMs)
            collapse()
        }
    }

    private fun stopAutoCollapseTimer() {
        autoCollapseJob?.cancel()
        autoCollapseJob = null
    }

    fun resetAutoCollapseTimer() {
        if (expanded.value) {
            startAutoCollapseTimer()
        }
    }

    fun setSelectedNotificationIndex(index: Int) {
        val list = visibleNotifications.value
        if (index in list.indices) {
            pin(list[index].key)
            selectedIndex.value = index
            resetAutoCollapseTimer()
        }
    }

    fun dismissCurrentNotification() {
        val list = visibleNotifications.value
        val index = selectedIndex.value
        if (index in list.indices) {
            val notification = list[index]
            // Mochi is not dismissed: he is where the island rests.
            if (notification.mode != IslandMode.Desk) {
                notificationRepo.removeNotification(notification.key)
                notificationRepo.sendCommand(SmartIslandCommand.CancelNotification(notification.key))
            }
        }
        collapse()
    }

    fun dismissAllNotifications() {
        val list = visibleNotifications.value
        for (notification in list) {
            if (notification.mode == IslandMode.Desk) continue
            notificationRepo.sendCommand(SmartIslandCommand.CancelNotification(notification.key))
            notificationRepo.removeNotification(notification.key)
        }
        collapse()
    }

    companion object {
        private const val TAG = "IslandViewModel"
        private const val AUTO_COLLAPSE_DELAY_MS = 5000L
        private const val PIN_MS = 8000L
        private const val FRESH_NEWS_MS = 30_000L

        fun provideFactory(
            settingsRepo: SmartIslandSettingsRepository,
            notificationRepo: INotificationRepository
        ): ViewModelProvider.Factory = object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <T : ViewModel> create(modelClass: Class<T>): T {
                return IslandViewModel(settingsRepo, notificationRepo) as T
            }
        }
    }
}
