package com.agupta07505.smartisland.coucou

import com.agupta07505.smartisland.coucou.phone.InstallSource
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class InstallSourceTest {
    @Test fun `only a store install keeps banking apps quiet`() {
        assertFalse(InstallSource.bankAppsWillBlock("com.android.vending"))
        assertTrue(InstallSource.bankAppsWillBlock(null))                       // adb install
        assertTrue(InstallSource.bankAppsWillBlock("com.google.android.packageinstaller"))  // a copied APK
        assertTrue(InstallSource.bankAppsWillBlock("com.android.shell"))
    }
}
