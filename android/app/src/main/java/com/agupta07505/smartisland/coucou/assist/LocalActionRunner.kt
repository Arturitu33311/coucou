/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.assist

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings

/** What came of an action: [handled] false means "not for me, ask a model"; [message] is what Jinx says. */
data class ActionResult(val handled: Boolean, val message: String = "")

/** Runs a [LocalAction] with plain intents: no extra permission, nothing leaves the phone. */
class LocalActionRunner(private val context: Context) {
    fun run(action: LocalAction): ActionResult = try {
        when (action) {
            is LocalAction.OpenApp -> openApp(action.name)
            is LocalAction.OpenSettings -> start(Intent(settingsAction(action.page)), "Abrí los ajustes.")
            is LocalAction.WebSearch -> start(Intent(Intent.ACTION_WEB_SEARCH).putExtra("query", action.query), "Buscando «${action.query}».")
            is LocalAction.Navigate -> start(
                Intent(Intent.ACTION_VIEW, Uri.parse("google.navigation:q=" + Uri.encode(action.place))),
                "Te llevo a ${action.place}.",
            )
            is LocalAction.Dial -> start(Intent(Intent.ACTION_DIAL, Uri.parse("tel:" + Uri.encode(action.number))), "Marcando ${action.number}: solo falta que le des llamar.")
        }
    } catch (_: ActivityNotFoundException) {
        ActionResult(false)
    } catch (_: SecurityException) {
        ActionResult(false)
    }

    private fun openApp(name: String): ActionResult {
        val pm = context.packageManager
        val launcher = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        val apps = pm.queryIntentActivities(launcher, 0).map { AppEntry(it.loadLabel(pm).toString(), it.activityInfo.packageName) }
        val app = AppMatcher.best(name, apps) ?: return ActionResult(false)
        val intent = pm.getLaunchIntentForPackage(app.packageName) ?: return ActionResult(false)
        return start(intent, "Abriendo ${app.label}.")
    }

    private fun start(intent: Intent, done: String): ActionResult {
        context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        return ActionResult(true, done)
    }

    private fun settingsAction(page: SettingsPage): String = when (page) {
        SettingsPage.General -> Settings.ACTION_SETTINGS
        SettingsPage.Wifi -> Settings.ACTION_WIFI_SETTINGS
        SettingsPage.Bluetooth -> Settings.ACTION_BLUETOOTH_SETTINGS
        SettingsPage.Display -> Settings.ACTION_DISPLAY_SETTINGS
        SettingsPage.Battery -> Intent.ACTION_POWER_USAGE_SUMMARY
        SettingsPage.Notifications -> Settings.ACTION_SETTINGS
        SettingsPage.Accessibility -> Settings.ACTION_ACCESSIBILITY_SETTINGS
        SettingsPage.Sound -> Settings.ACTION_SOUND_SETTINGS
        SettingsPage.Location -> Settings.ACTION_LOCATION_SOURCE_SETTINGS
        SettingsPage.Apps -> Settings.ACTION_APPLICATION_SETTINGS
        SettingsPage.Data -> Settings.ACTION_DATA_ROAMING_SETTINGS
        SettingsPage.DoNotDisturb -> Settings.ACTION_ZEN_MODE_PRIORITY_SETTINGS
        SettingsPage.Storage -> Settings.ACTION_INTERNAL_STORAGE_SETTINGS
    }
}
