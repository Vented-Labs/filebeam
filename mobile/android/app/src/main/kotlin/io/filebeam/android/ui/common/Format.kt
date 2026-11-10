package io.filebeam.android.ui.common

import android.content.Context
import io.filebeam.android.R
import java.text.DateFormat
import java.time.Instant
import java.util.Date
import java.util.Locale
import kotlin.math.abs

/** Sizes in binary units with one decimal where it helps, e.g. "2.0 GiB". */
fun formatBytes(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val units = listOf("KiB", "MiB", "GiB", "TiB")
    var value = bytes / 1024.0
    var unit = 0
    while (value >= 1024 && unit < units.lastIndex) {
        value /= 1024
        unit++
    }
    return if (value >= 100) "%.0f %s".format(Locale.US, value, units[unit]) else "%.1f %s".format(Locale.US, value, units[unit])
}

/** "1 hour", "3 days", "1 year": retention options read as durations, never as raw hour counts. */
fun formatRetention(context: Context, hours: Long): String {
    val resources = context.resources
    return when {
        hours % 8760 == 0L -> resources.getQuantityString(R.plurals.duration_years, (hours / 8760).toInt(), (hours / 8760).toInt())
        hours % 24 == 0L -> resources.getQuantityString(R.plurals.duration_days, (hours / 24).toInt(), (hours / 24).toInt())
        else -> resources.getQuantityString(R.plurals.duration_hours, hours.toInt(), hours.toInt())
    }
}

/** Parses the server's ISO-8601 timestamps; null when absent or malformed. */
fun parseInstant(value: String?): Instant? = value?.takeIf(String::isNotBlank)?.let { runCatching { Instant.parse(it) }.getOrNull() }

fun formatAbsolute(instant: Instant): String =
    DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date.from(instant))

/** "in 3 hours", "2 days ago", "moments ago". */
fun formatRelative(context: Context, instant: Instant, now: Instant = Instant.now()): String {
    val seconds = instant.epochSecond - now.epochSecond
    val future = seconds > 0
    val magnitude = abs(seconds)
    val resources = context.resources
    val amount = when {
        magnitude < 60 -> return context.getString(if (future) R.string.relative_soon else R.string.relative_now)
        magnitude < 3600 -> resources.getQuantityString(R.plurals.duration_minutes, (magnitude / 60).toInt(), (magnitude / 60).toInt())
        magnitude < 86_400 -> resources.getQuantityString(R.plurals.duration_hours, (magnitude / 3600).toInt(), (magnitude / 3600).toInt())
        else -> resources.getQuantityString(R.plurals.duration_days, (magnitude / 86_400).toInt(), (magnitude / 86_400).toInt())
    }
    return context.getString(if (future) R.string.relative_future else R.string.relative_past, amount)
}
