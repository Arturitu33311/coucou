package com.agupta07505.smartisland.util

import androidx.compose.foundation.border
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.dp
import com.agupta07505.smartisland.data.SmartIslandSettings

/**
 * Keeps a dark island readable on a dark screen. A black pill on a black status bar has no edge (a
 * black shadow does nothing there), so only its icons show. When this is on, the pill keeps its
 * fill and gets a hairline of light that is strongest along the top, like light catching the rim
 * of a glass object, and eases off along the sides but never vanishes: the bottom edge has to be
 * seen as well, or the pill seems to melt into the screen below it.
 */
object EdgeContrast {
    /** Below this relative luminance the pill counts as dark (a chosen colour like navy still does). */
    const val DARK_LUMINANCE = 0.08f

    /** The rim's strength at the top, middle and bottom of the pill. */
    const val RIM_TOP_ALPHA = 0.34f
    const val RIM_MID_ALPHA = 0.20f
    const val RIM_BOTTOM_ALPHA = 0.17f

    /** [mode] is one of [SmartIslandSettings.EDGE_CONTRAST_AUTO]/ALWAYS/NEVER (unknown reads as auto). */
    fun shouldApply(mode: String, systemDark: Boolean, pillLuminance: Float): Boolean = when (mode) {
        SmartIslandSettings.EDGE_CONTRAST_ALWAYS -> true
        SmartIslandSettings.EDGE_CONTRAST_NEVER -> false
        else -> systemDark && pillLuminance < DARK_LUMINANCE
    }
}

/** A graded 0.75 dp rim around [shape] when [on]; a no-op otherwise. Apply after clip and background. */
fun Modifier.edgeContrast(on: Boolean, shape: Shape): Modifier =
    if (!on) this else this.border(
        width = 0.75.dp,
        brush = Brush.verticalGradient(
            0f to Color.White.copy(alpha = EdgeContrast.RIM_TOP_ALPHA),
            0.5f to Color.White.copy(alpha = EdgeContrast.RIM_MID_ALPHA),
            1f to Color.White.copy(alpha = EdgeContrast.RIM_BOTTOM_ALPHA),
        ),
        shape = shape,
    )
