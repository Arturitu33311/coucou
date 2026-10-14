/*
 * Coucou for Android — part of this SmartIsland fork (GNU GPL v3).
 */

package com.agupta07505.smartisland.coucou.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Coucou's own screens on the phone (Jinx, Pendientes, Ajustes) share one look: the island's dark, Mochi's
 * emerald for what is Jinx's or "on", amber for what needs you. The SmartIsland screens keep theirs.
 */
object CoucouColors {
    val Background = Color(0xFF0B0B10)
    val Surface = Color(0xFF15151B)
    val SurfaceHigh = Color(0xFF1D1D25)
    val Outline = Color.White.copy(alpha = 0.07f)
    val Text = Color(0xFFF5F6F8)
    val TextSecondary = Color.White.copy(alpha = 0.64f)
    val TextTertiary = Color.White.copy(alpha = 0.42f)
    val Accent = MochiEmerald
    val AccentSoft = MochiEmerald.copy(alpha = 0.16f)
    val Warning = Color(0xFFF5A524)
    val Danger = Color(0xFFF4505E)
    val Info = Color(0xFF4F9BFF)
}

private val CoucouScheme = darkColorScheme(
    primary = CoucouColors.Accent,
    onPrimary = Color(0xFF04140D),
    primaryContainer = CoucouColors.AccentSoft,
    onPrimaryContainer = CoucouColors.Accent,
    secondary = CoucouColors.Accent,
    background = CoucouColors.Background,
    onBackground = CoucouColors.Text,
    surface = CoucouColors.Surface,
    onSurface = CoucouColors.Text,
    surfaceVariant = CoucouColors.SurfaceHigh,
    onSurfaceVariant = CoucouColors.TextSecondary,
    surfaceContainer = CoucouColors.Surface,
    surfaceContainerHigh = CoucouColors.SurfaceHigh,
    outline = Color.White.copy(alpha = 0.18f),
    outlineVariant = CoucouColors.Outline,
    error = CoucouColors.Danger,
)

@Composable
fun CoucouTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = CoucouScheme, content = content)
}

/** A screen's title row: Mochi (or another mark), the title, a line under it, and actions at the end. */
@Composable
fun ScreenHeader(
    title: String,
    subtitle: String? = null,
    leading: (@Composable () -> Unit)? = null,
    actions: @Composable () -> Unit = {},
) {
    Row(Modifier.fillMaxWidth().padding(top = 4.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        if (leading != null) {
            leading()
            Spacer(Modifier.width(12.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(title, color = CoucouColors.Text, fontSize = 24.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.3).sp)
            if (!subtitle.isNullOrEmpty()) Text(subtitle, color = CoucouColors.TextSecondary, fontSize = 13.sp)
        }
        actions()
    }
}

/** A group of settings or content: a rounded surface with a title, an optional line under it, and its rows. */
@Composable
fun SectionCard(
    title: String? = null,
    subtitle: String? = null,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Column(
        modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(CoucouColors.Surface)
            .border(1.dp, CoucouColors.Outline, RoundedCornerShape(20.dp))
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (title != null || subtitle != null) {
            Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                if (title != null) Text(title, color = CoucouColors.Text, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                if (subtitle != null) Text(subtitle, color = CoucouColors.TextTertiary, fontSize = 12.5.sp, lineHeight = 17.sp)
            }
        }
        content()
    }
}

/** A small uppercase label above a group (Por hacer, Hechas, Conexiones…). */
@Composable
fun GroupLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text.uppercase(),
        color = CoucouColors.TextTertiary,
        fontSize = 11.5.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = 0.8.sp,
        modifier = modifier.padding(start = 4.dp, top = 6.dp, bottom = 2.dp),
    )
}

/** A setting that is on or off: what it does, a line on why, and the switch on the right. */
@Composable
fun SwitchRow(
    title: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
    description: String? = null,
    enabled: Boolean = true,
) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable(enabled) { onChange(!checked) }.padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f).padding(end = 12.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, color = CoucouColors.Text.copy(alpha = if (enabled) 1f else 0.45f), fontSize = 15.sp)
            if (description != null) Text(description, color = CoucouColors.TextTertiary, fontSize = 12.5.sp, lineHeight = 17.sp)
        }
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            enabled = enabled,
            colors = SwitchDefaults.colors(
                checkedThumbColor = Color.White,
                checkedTrackColor = CoucouColors.Accent,
                uncheckedThumbColor = Color.White.copy(alpha = 0.7f),
                uncheckedTrackColor = Color.White.copy(alpha = 0.1f),
                uncheckedBorderColor = Color.Transparent,
            ),
        )
    }
}

enum class Tone { Neutral, Good, Warn, Bad }

private fun Tone.color() = when (this) {
    Tone.Neutral -> CoucouColors.TextTertiary
    Tone.Good -> CoucouColors.Accent
    Tone.Warn -> CoucouColors.Warning
    Tone.Bad -> CoucouColors.Danger
}

/** A dot and a short line: how a connection or a sync is doing. */
@Composable
fun StatusLine(text: String, tone: Tone = Tone.Neutral) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(7.dp).clip(CircleShape).background(tone.color()))
        Spacer(Modifier.width(8.dp))
        Text(text, color = if (tone == Tone.Neutral) CoucouColors.TextSecondary else tone.color(), fontSize = 12.5.sp)
    }
}

/** A rounded chip with a label (a session, a source, a filter). */
@Composable
fun Chip(text: String, tint: Color = CoucouColors.TextSecondary, dot: Color? = null, onClick: (() -> Unit)? = null) {
    Row(
        Modifier
            .clip(RoundedCornerShape(50))
            .background(Color.White.copy(alpha = 0.06f))
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (dot != null) {
            Box(Modifier.size(6.dp).clip(CircleShape).background(dot))
            Spacer(Modifier.width(6.dp))
        }
        Text(text, color = tint, fontSize = 12.sp, maxLines = 1)
    }
}

/** When a list has nothing: Mochi, a line, and what to do next. */
@Composable
fun EmptyState(title: String, body: String, mood: MochiMood = MochiMood.Sleepy, size: Dp = 64.dp, modifier: Modifier = Modifier) {
    Column(
        modifier.fillMaxWidth().padding(vertical = 36.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        MochiGlyph(size = size, mood = mood)
        Spacer(Modifier.width(4.dp))
        Text(title, color = CoucouColors.Text, fontSize = 16.sp, fontWeight = FontWeight.Medium)
        Text(body, color = CoucouColors.TextTertiary, fontSize = 13.sp, lineHeight = 18.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
    }
}
