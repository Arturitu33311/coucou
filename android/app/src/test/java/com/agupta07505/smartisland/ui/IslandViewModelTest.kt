/*
 * Smart Island (2026)
 * © Animesh Gupta — github.com/agupta07505
 * Licensed under the GNU GPL v3 License
 * Do not remove or alter this notice. - Per GPL-3.0 Section 4 & Section 5
 */

package com.agupta07505.smartisland.ui

import com.agupta07505.smartisland.data.SmartIslandNotificationRepository
import com.agupta07505.smartisland.data.SmartIslandSettingsRepository
import com.agupta07505.smartisland.model.IslandMode
import com.agupta07505.smartisland.model.IslandNotification
import com.agupta07505.smartisland.model.YIELDED_MUSIC_PRIORITY
import com.agupta07505.smartisland.model.priority
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import io.mockk.mockk

@OptIn(ExperimentalCoroutinesApi::class)
class IslandViewModelTest {

    private val testDispatcher = StandardTestDispatcher()

    @Before
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun testMusicPillHidesInForegroundAndReappearsInBackground() = runTest {
        val settingsRepo = mockk<SmartIslandSettingsRepository>(relaxed = true)
        val notifRepo = SmartIslandNotificationRepository()
        val viewModel = IslandViewModel(settingsRepo, notifRepo)

        val musicNotif = IslandNotification(
            key = "music_1",
            packageName = "com.spotify.music",
            appName = "Spotify",
            title = "Blinding Lights",
            text = "The Weeknd",
            mode = IslandMode.Music,
            timeMillis = System.currentTimeMillis()
        )

        notifRepo.postNotification(musicNotif)
        testDispatcher.scheduler.advanceUntilIdle()

        // Initially Spotify is in background -> music notification is visible
        assertEquals(1, viewModel.visibleNotifications.value.size)
        assertEquals(IslandMode.Music, viewModel.mode.value)

        // User opens Spotify (Spotify in foreground) -> music notification hides from pill
        viewModel.foregroundPackage.value = "com.spotify.music"
        testDispatcher.scheduler.advanceUntilIdle()

        assertTrue(viewModel.visibleNotifications.value.isEmpty())
        assertEquals(IslandMode.Empty, viewModel.mode.value)

        // User leaves Spotify (Chrome in foreground) -> music notification reappears on pill!
        viewModel.foregroundPackage.value = "com.android.chrome"
        testDispatcher.scheduler.advanceUntilIdle()

        assertEquals(1, viewModel.visibleNotifications.value.size)
        assertEquals(IslandMode.Music, viewModel.mode.value)
    }

    @Test
    fun testInputActivePausesAutoCollapseAndCollapseResetsInputActive() = runTest {
        val settingsRepo = mockk<SmartIslandSettingsRepository>(relaxed = true)
        val notifRepo = SmartIslandNotificationRepository()
        val viewModel = IslandViewModel(settingsRepo, notifRepo)

        // Expand island
        viewModel.expand()
        testDispatcher.scheduler.runCurrent()
        assertTrue(viewModel.expanded.value)

        // Activate text input (e.g. user clicked reply)
        viewModel.setInputActive(true)
        testDispatcher.scheduler.runCurrent()
        assertTrue(viewModel.isInputActive.value)

        // Advance time past the 5000ms auto-collapse delay
        testDispatcher.scheduler.advanceTimeBy(6000L)
        testDispatcher.scheduler.runCurrent()

        // Island should STILL be expanded because input is active!
        assertTrue(viewModel.expanded.value)

        // Deactivate input
        viewModel.setInputActive(false)
        testDispatcher.scheduler.runCurrent()
        org.junit.Assert.assertFalse(viewModel.isInputActive.value)

        // Advance 5000ms now that input is inactive
        testDispatcher.scheduler.advanceTimeBy(5500L)
        testDispatcher.scheduler.runCurrent()

        // Island should now have auto-collapsed
        org.junit.Assert.assertFalse(viewModel.expanded.value)
    }

    @Test
    fun testExplicitCollapseResetsInputActive() = runTest {
        val settingsRepo = mockk<SmartIslandSettingsRepository>(relaxed = true)
        val notifRepo = SmartIslandNotificationRepository()
        val viewModel = IslandViewModel(settingsRepo, notifRepo)

        viewModel.expand()
        viewModel.setInputActive(true)
        testDispatcher.scheduler.runCurrent()
        assertTrue(viewModel.isInputActive.value)

        viewModel.collapse()
        testDispatcher.scheduler.runCurrent()
        org.junit.Assert.assertFalse(viewModel.expanded.value)
        org.junit.Assert.assertFalse(viewModel.isInputActive.value)
    }

    @Test
    fun testAutoExpandEnabledExpandsIslandOnNewNotification() = runTest {
        val settingsRepo = mockk<SmartIslandSettingsRepository>(relaxed = true)
        io.mockk.every { settingsRepo.settings } returns kotlinx.coroutines.flow.MutableStateFlow(
            com.agupta07505.smartisland.data.SmartIslandSettings(autoExpandOnNotification = true)
        )
        val notifRepo = SmartIslandNotificationRepository()
        val viewModel = IslandViewModel(settingsRepo, notifRepo)
        testDispatcher.scheduler.runCurrent()

        val notif = IslandNotification(
            key = "msg_1",
            packageName = "com.whatsapp",
            appName = "WhatsApp",
            title = "Alice",
            text = "Hello!",
            mode = IslandMode.Notification,
            timeMillis = System.currentTimeMillis()
        )

        notifRepo.postNotification(notif, autoExpand = true)
        testDispatcher.scheduler.runCurrent()

        assertTrue(viewModel.expanded.value)
        assertEquals(0, viewModel.selectedIndex.value)
    }

    @Test
    fun testAutoExpandDisabledKeepsIslandCollapsedOnNewNotification() = runTest {
        val settingsRepo = mockk<SmartIslandSettingsRepository>(relaxed = true)
        io.mockk.every { settingsRepo.settings } returns kotlinx.coroutines.flow.MutableStateFlow(
            com.agupta07505.smartisland.data.SmartIslandSettings(autoExpandOnNotification = false)
        )
        val notifRepo = SmartIslandNotificationRepository()
        val viewModel = IslandViewModel(settingsRepo, notifRepo)
        testDispatcher.scheduler.runCurrent()

        val notif = IslandNotification(
            key = "msg_2",
            packageName = "com.whatsapp",
            appName = "WhatsApp",
            title = "Bob",
            text = "Hey there",
            mode = IslandMode.Notification,
            timeMillis = System.currentTimeMillis()
        )

        notifRepo.postNotification(notif, autoExpand = true)
        testDispatcher.scheduler.runCurrent()

        // Should NOT be expanded because autoExpandOnNotification is false
        org.junit.Assert.assertFalse(viewModel.expanded.value)
        // Notification is still captured and selected for compact pill
        assertEquals(1, viewModel.visibleNotifications.value.size)
        assertEquals(0, viewModel.selectedIndex.value)
    }

    @Test
    fun testPausedMusicYieldsToNewsButPlayingKeepsThrone() = runTest {
        // A paused player camps like a playing one unless it yields: news goes first, so a stale
        // player can never hide everything behind it.
        val settingsRepo = mockk<SmartIslandSettingsRepository>(relaxed = true)
        val notifRepo = SmartIslandNotificationRepository()
        val viewModel = IslandViewModel(settingsRepo, notifRepo)
        val paused = IslandNotification(
            key = "music_paused",
            packageName = "com.spotify.music",
            appName = "Spotify",
            title = "Old Song",
            text = "Old Artist",
            mode = IslandMode.Music,
            mediaIsPlaying = false,
            timeMillis = System.currentTimeMillis() - 60_000,
        )
        val news = IslandNotification(
            key = "news_1",
            packageName = "org.kde.kdeconnect_tp",
            appName = "KDE Connect",
            title = "Ping",
            text = "pong",
            mode = IslandMode.Notification,
            timeMillis = System.currentTimeMillis(),
        )
        notifRepo.postNotification(paused)
        notifRepo.postNotification(news)
        testDispatcher.scheduler.advanceUntilIdle()

        val keys = viewModel.visibleNotifications.value.map { it.key }
        // News first, paused player after (still there, above the battery ring's 10).
        assertEquals(listOf("news_1", "music_paused"), keys)
        org.junit.Assert.assertTrue(YIELDED_MUSIC_PRIORITY < IslandMode.Notification.priority)

        // Playing again with only old news around: the throne is hers.
        notifRepo.removeNotification("news_1")
        notifRepo.postNotification(paused.copy(key = "music_playing", mediaIsPlaying = true, timeMillis = System.currentTimeMillis() + 1))
        notifRepo.postNotification(news.copy(key = "news_old", timeMillis = System.currentTimeMillis() - 60_000))
        testDispatcher.scheduler.advanceUntilIdle()
        assertEquals("music_playing", viewModel.visibleNotifications.value.first().key)

        // Fresh news surfaces over the song for a while, then music returns.
        notifRepo.postNotification(news.copy(key = "news_fresh", timeMillis = System.currentTimeMillis()))
        testDispatcher.scheduler.advanceUntilIdle()
        assertEquals("news_fresh", viewModel.visibleNotifications.value.first().key)
    }

    @Test
    fun testRemoteMirrorKeepsCardButYieldsThrone() = runTest {
        // KDE Connect mirrors the PC's player with an active session of its own: the card stays
        // fully Music (artwork, controls), but news still goes first.
        val settingsRepo = mockk<SmartIslandSettingsRepository>(relaxed = true)
        val notifRepo = SmartIslandNotificationRepository()
        val viewModel = IslandViewModel(settingsRepo, notifRepo)
        val mirror = IslandNotification(
            key = "kde_music",
            packageName = "org.kde.kdeconnect_tp",
            appName = "KDE Connect",
            title = "PC Song",
            text = "PC Artist",
            mode = IslandMode.Music,
            mediaIsPlaying = true,
            timeMillis = System.currentTimeMillis() - 60_000,
        )
        val news = IslandNotification(
            key = "news_2",
            packageName = "com.whatsapp",
            appName = "WhatsApp",
            title = "Ana",
            text = "hola",
            mode = IslandMode.Notification,
            timeMillis = System.currentTimeMillis(),
        )
        notifRepo.postNotification(mirror)
        notifRepo.postNotification(news)
        testDispatcher.scheduler.advanceUntilIdle()

        val first = viewModel.visibleNotifications.value.first()
        assertEquals("news_2", first.key)
        // The mirror is still there, still Music — only its priority yielded.
        val kept = viewModel.visibleNotifications.value.first { it.key == "kde_music" }
        assertEquals(IslandMode.Music, kept.mode)
    }
}

