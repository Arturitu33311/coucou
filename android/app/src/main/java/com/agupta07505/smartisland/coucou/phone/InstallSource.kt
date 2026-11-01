/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.phone

import android.content.Context
import android.os.Build

/**
 * Who installed this app. Banking apps (Nu) refuse to run beside an accessibility service from an app that did
 * not come from a store, and name it in a full-screen "desinstala esta aplicación" (docs/BANK-APPS.md). A plain
 * `adb install` leaves no installer; `scripts/install-phone.sh` records Play Store. Ajustes says so when it is wrong.
 */
object InstallSource {
    const val STORE = "com.android.vending"

    fun installer(context: Context): String? = runCatching {
        val pm = context.packageManager
        if (Build.VERSION.SDK_INT >= 30) {
            pm.getInstallSourceInfo(context.packageName).installingPackageName
        } else {
            @Suppress("DEPRECATION")
            pm.getInstallerPackageName(context.packageName)
        }
    }.getOrNull()

    /** True when banking apps would block us: installed by anything but a store. Pure, tested. */
    fun bankAppsWillBlock(installer: String?): Boolean = installer != STORE
}
