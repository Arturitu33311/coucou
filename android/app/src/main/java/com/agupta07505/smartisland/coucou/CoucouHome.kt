/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Chat
import androidx.compose.material.icons.rounded.Checklist
import androidx.compose.material.icons.rounded.Tune
import androidx.compose.material.icons.rounded.Widgets
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.agupta07505.smartisland.coucou.assist.AssistModel
import com.agupta07505.smartisland.coucou.assist.AssistScreen
import com.agupta07505.smartisland.coucou.phone.Setup
import com.agupta07505.smartisland.coucou.ui.CoucouColors
import com.agupta07505.smartisland.coucou.ui.CoucouTheme
import com.agupta07505.smartisland.ui.SmartIslandHomeScreen
import com.agupta07505.smartisland.ui.SmartIslandTheme

/**
 * The Coucou app's tabs. Their names travel in intents (`CoucouTasksActivity.EXTRA_TAB`): keep them.
 * [Island] is SmartIsland's own home screen, as it always looked; it pads the status bar itself ([ownsTopInset]).
 */
enum class CoucouTab(val label: String, val icon: ImageVector, val ownsTopInset: Boolean = false) {
    Jinx("Jinx", Icons.AutoMirrored.Rounded.Chat),
    Tasks("Pendientes", Icons.Rounded.Checklist),
    Island("Isla", Icons.Rounded.Widgets, ownsTopInset = true),
    Settings("Ajustes", Icons.Rounded.Tune),
}

/**
 * Coucou on the phone, in one place: Jinx (the assistant), the pendientes, the island (SmartIsland's screen:
 * permissions, what it shows, its look) and the settings of the link with the computers. One launcher icon.
 * The side key opens it on Jinx, the launcher and the reminders on the pendientes, the island's notification
 * on the island.
 */
@Composable
fun CoucouHome(initial: CoucouTab) {
    var tab by rememberSaveable { mutableStateOf(initial) }
    CoucouTheme {
        CoucouScaffold(tab = tab, onTab = { tab = it }) { modifier, shown ->
            when (shown) {
                CoucouTab.Jinx -> AssistScreen(viewModel<AssistModel>(), modifier.imePadding())
                CoucouTab.Tasks -> TasksScreen(modifier)
                // Its own dark theme, next to Coucou's dark tabs.
                CoucouTab.Island -> Box(modifier) { SmartIslandTheme(darkTheme = true) { SmartIslandHomeScreen() } }
                CoucouTab.Settings -> Setup(modifier)
            }
        }
    }
}

/** The frame of the app: the bar to change tab, and the shown tab's page. Stateless, so tests can draw it. */
@Composable
internal fun CoucouScaffold(tab: CoucouTab, onTab: (CoucouTab) -> Unit, page: @Composable (Modifier, CoucouTab) -> Unit) {
    Scaffold(
        containerColor = CoucouColors.Background,
        bottomBar = {
            NavigationBar(containerColor = CoucouColors.Surface, tonalElevation = 0.dp) {
                for (t in CoucouTab.entries) {
                    NavigationBarItem(
                        selected = t == tab,
                        onClick = { onTab(t) },
                        icon = { Icon(t.icon, contentDescription = null) },
                        label = { Text(t.label) },
                        colors = NavigationBarItemDefaults.colors(
                            selectedIconColor = CoucouColors.Accent,
                            selectedTextColor = CoucouColors.Text,
                            indicatorColor = CoucouColors.AccentSoft,
                            unselectedIconColor = CoucouColors.TextTertiary,
                            unselectedTextColor = CoucouColors.TextTertiary,
                        ),
                    )
                }
            }
        },
    ) { inner ->
        AnimatedContent(targetState = tab, transitionSpec = { fadeIn() togetherWith fadeOut() }, label = "tab") { shown ->
            // SmartIsland's screen pads the status bar on its own: give it only the bottom bar's room.
            val pad = if (shown.ownsTopInset) PaddingValues(bottom = inner.calculateBottomPadding()) else inner
            page(Modifier.fillMaxSize().padding(pad).consumeWindowInsets(pad), shown)
        }
    }
}
