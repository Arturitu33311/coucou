/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.handoff

import android.app.Activity
import android.os.Bundle

/** The island card's tap lands here: it hands the tap to [HandoffFlow] and disappears at once. */
class HandoffActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        intent.getStringExtra(EXTRA_ID)?.let { HandoffFlow.tap(applicationContext, it) }
        finish()
    }

    companion object {
        const val EXTRA_ID = "id"
    }
}
