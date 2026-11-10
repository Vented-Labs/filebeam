package io.filebeam.android.ui.design.color

import kotlin.math.atan2
import kotlin.math.cbrt
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sin

/** Lightness, chroma and hue in OKLCH. Hue is in degrees. */
internal data class Oklch(val l: Double, val c: Double, val h: Double)

/**
 * Colour maths shared with the web theme generator. Results must stay identical to
 * `backend/app/Support/Theming/Color.php`, so every constant and rounding rule mirrors it.
 * Colours are opaque sRGB packed as 0xRRGGBB.
 */
internal object ColorMath {
    fun parse(hex: String): Int = hex.removePrefix("#").toInt(16) and 0xFFFFFF

    fun format(rgb: Int): String = "#%06x".format(rgb and 0xFFFFFF)

    private fun red(rgb: Int) = rgb shr 16 and 0xFF
    private fun green(rgb: Int) = rgb shr 8 and 0xFF
    private fun blue(rgb: Int) = rgb and 0xFF

    private fun decode(channel: Int): Double {
        val value = channel / 255.0
        return if (value <= 0.04045) value / 12.92 else ((value + 0.055) / 1.055).pow(2.4)
    }

    fun oklch(rgb: Int): Oklch {
        val r = decode(red(rgb))
        val g = decode(green(rgb))
        val b = decode(blue(rgb))
        val l = cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
        val m = cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
        val s = cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
        val a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s
        val bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s
        return Oklch(
            0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
            hypot(a, bb),
            (Math.toDegrees(atan2(bb, a)) + 360.0).rem(360.0),
        )
    }

    private fun linear(lightness: Double, chroma: Double, hue: Double): DoubleArray {
        val a = chroma * cos(Math.toRadians(hue))
        val b = chroma * sin(Math.toRadians(hue))
        val l = (lightness + 0.3963377774 * a + 0.2158037573 * b).pow(3)
        val m = (lightness - 0.1055613458 * a - 0.0638541728 * b).pow(3)
        val s = (lightness - 0.0894841775 * a - 1.291485548 * b).pow(3)
        return doubleArrayOf(
            4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
        )
    }

    /** Converts OKLCH to sRGB, reducing chroma at the gamut boundary so hue is preserved. */
    fun rgb(lightness: Double, chroma: Double, hue: Double): Int {
        val l = lightness.coerceIn(0.0, 1.0)
        var low = 0.0
        var high = max(0.0, chroma)
        var rgb = linear(l, high, hue)
        if (rgb.min() < -0.000001 || rgb.max() > 1.000001) {
            repeat(20) {
                val middle = (low + high) / 2
                val candidate = linear(l, middle, hue)
                if (candidate.min() >= 0 && candidate.max() <= 1) low = middle else high = middle
            }
            rgb = linear(l, low, hue)
        }
        fun encode(value: Double): Int {
            val channel = value.coerceIn(0.0, 1.0)
            val encoded = if (channel <= 0.0031308) 12.92 * channel else 1.055 * channel.pow(1 / 2.4) - 0.055
            return (encoded * 255).roundToInt().coerceIn(0, 255)
        }
        return encode(rgb[0]) shl 16 or (encode(rgb[1]) shl 8) or encode(rgb[2])
    }

    fun rgb(color: Oklch): Int = rgb(color.l, color.c, color.h)

    /** Mixes in OKLab; [weight] is the share of [first]. */
    fun mix(first: Int, second: Int, weight: Double): Int {
        val one = oklch(first)
        val two = oklch(second)
        val a = weight * one.c * cos(Math.toRadians(one.h)) + (1 - weight) * two.c * cos(Math.toRadians(two.h))
        val b = weight * one.c * sin(Math.toRadians(one.h)) + (1 - weight) * two.c * sin(Math.toRadians(two.h))
        return rgb(weight * one.l + (1 - weight) * two.l, hypot(a, b), Math.toDegrees(atan2(b, a)))
    }

    /** Flattens [foreground] at [alpha] over an opaque [background]. */
    fun composite(foreground: Int, alpha: Double, background: Int): Int {
        fun channel(front: Int, back: Int) = (alpha * front + (1 - alpha) * back).roundToInt()
        return channel(red(foreground), red(background)) shl 16 or
            (channel(green(foreground), green(background)) shl 8) or
            channel(blue(foreground), blue(background))
    }

    fun luminance(rgb: Int): Double =
        decode(red(rgb)) * 0.2126 + decode(green(rgb)) * 0.7152 + decode(blue(rgb)) * 0.0722

    fun contrast(first: Int, second: Int): Double {
        val a = luminance(first)
        val b = luminance(second)
        return (max(a, b) + 0.05) / (min(a, b) + 0.05)
    }

    fun minimumContrast(foreground: Int, backgrounds: List<Int>): Double =
        backgrounds.minOf { contrast(foreground, it) }

    /**
     * Walks lightness in 0.001 steps until [color] reaches [ratio] against every background.
     * Returns null when no lightness satisfies it.
     */
    fun against(color: Oklch, backgrounds: List<Int>, ratio: Double, darker: Boolean): Int? {
        for (step in 0..1000) {
            val candidate = color.l + (if (darker) -1 else 1) * step * 0.001
            if (candidate < 0 || candidate > 1) break
            val foreground = rgb(candidate, color.c, color.h)
            if (minimumContrast(foreground, backgrounds) >= ratio) return foreground
        }
        return null
    }
}
