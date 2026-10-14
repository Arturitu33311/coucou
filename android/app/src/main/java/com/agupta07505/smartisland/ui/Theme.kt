/*
 * Smart Island (2026)
 * © Animesh Gupta — github.com/agupta07505
 * Licensed under the GNU GPL v3 License
 * Do not remove or alter this notice. - Per GPL-3.0 Section 4 & Section 5
 */

package com.agupta07505.smartisland.ui

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

// High-contrast, clean Black & White palette accented with #D84315 (Deep Orange 800) and its warm variations
private val LightColorScheme = lightColorScheme(
    primary = Color(0xFFD84315),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFFFEDE6),
    onPrimaryContainer = Color(0xFFBF360C),
    secondary = Color(0xFFBF360C),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFFCEBE6),
    onSecondaryContainer = Color(0xFF3E1204),
    tertiary = Color(0xFFE64A19),
    onTertiary = Color.White,
    tertiaryContainer = Color(0xFFFFE0D6),
    onTertiaryContainer = Color(0xFF381003),
    background = Color(0xFFFAFAFA),
    onBackground = Color(0xFF121212),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF121212),
    surfaceVariant = Color(0xFFF1F1F1),
    onSurfaceVariant = Color(0xFF5A5A5A),
    outline = Color(0xFFE0E0E0),
    outlineVariant = Color(0xFFEDEDED)
)

private val DarkColorScheme = darkColorScheme(
    primary = Color(0xFFFF7043),
    onPrimary = Color(0xFF2B0B02),
    primaryContainer = Color(0xFF3D1307),
    onPrimaryContainer = Color(0xFFFFDBCF),
    secondary = Color(0xFFFF8A65),
    onSecondary = Color(0xFF2B0B02),
    secondaryContainer = Color(0xFF2A1008),
    onSecondaryContainer = Color(0xFFFFCCBC),
    tertiary = Color(0xFFFFAB91),
    onTertiary = Color(0xFF2B0B02),
    tertiaryContainer = Color(0xFF240D06),
    onTertiaryContainer = Color(0xFFFFE0D6),
    background = Color(0xFF0A0A0A),
    onBackground = Color(0xFFFFFFFF),
    surface = Color(0xFF141414),
    onSurface = Color(0xFFF7F7F7),
    surfaceVariant = Color(0xFF1F1F1F),
    onSurfaceVariant = Color(0xFFA6A6A6),
    outline = Color(0xFF2E2E2E),
    outlineVariant = Color(0xFF202020)
)

@Composable
fun SmartIslandTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    dynamicColor: Boolean = false, // Use our tailored premium palette by default for maximum brand consistency
    content: @Composable () -> Unit
) {
    val context = LocalContext.current
    val colorScheme = when {
        dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }
        darkTheme -> DarkColorScheme
        else -> LightColorScheme
    }

    MaterialTheme(
        colorScheme = colorScheme,
        content = content
    )
}

