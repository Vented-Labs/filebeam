import Foundation

/// Lightness, chroma and hue in OKLCH. Hue is in degrees.
public struct Oklch: Hashable, Sendable {
    public var l: Double
    public var c: Double
    public var h: Double
    public init(l: Double, c: Double, h: Double) { self.l = l; self.c = c; self.h = h }
}

/// An opaque sRGB colour packed as `0xRRGGBB`.
public typealias RGB = Int

/// Colour maths shared with the web theme generator. Results must stay identical to
/// `backend/app/Support/Theming/Color.php`, so every constant and rounding rule mirrors it.
public enum ColorMath {
    public static func parse(_ hex: String) -> RGB? {
        var value = hex.trimmingCharacters(in: .whitespacesAndNewlines)
        if value.hasPrefix("#") { value.removeFirst() }
        guard value.count == 6, let rgb = Int(value, radix: 16) else { return nil }
        return rgb & 0xFFFFFF
    }

    public static func format(_ rgb: RGB) -> String { String(format: "#%06x", rgb & 0xFFFFFF) }

    public static func red(_ rgb: RGB) -> Int { (rgb >> 16) & 0xFF }
    public static func green(_ rgb: RGB) -> Int { (rgb >> 8) & 0xFF }
    public static func blue(_ rgb: RGB) -> Int { rgb & 0xFF }

    private static func decode(_ channel: Int) -> Double {
        let value = Double(channel) / 255.0
        return value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4)
    }

    public static func oklch(_ rgb: RGB) -> Oklch {
        let r = decode(red(rgb)), g = decode(green(rgb)), b = decode(blue(rgb))
        let l = cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
        let m = cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
        let s = cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
        let a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s
        let bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s
        return Oklch(
            l: 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
            c: hypot(a, bb),
            h: (atan2(bb, a) * 180 / .pi + 360).truncatingRemainder(dividingBy: 360)
        )
    }

    private static func linear(_ lightness: Double, _ chroma: Double, _ hue: Double) -> (Double, Double, Double) {
        let a = chroma * cos(hue * .pi / 180)
        let b = chroma * sin(hue * .pi / 180)
        let l = pow(lightness + 0.3963377774 * a + 0.2158037573 * b, 3)
        let m = pow(lightness - 0.1055613458 * a - 0.0638541728 * b, 3)
        let s = pow(lightness - 0.0894841775 * a - 1.291485548 * b, 3)
        return (
            4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s
        )
    }

    private static func inGamut(_ rgb: (Double, Double, Double)) -> Bool {
        min(rgb.0, rgb.1, rgb.2) >= 0 && max(rgb.0, rgb.1, rgb.2) <= 1
    }

    /// Converts OKLCH to sRGB, reducing chroma at the gamut boundary so hue is preserved.
    public static func rgb(_ lightness: Double, _ chroma: Double, _ hue: Double) -> RGB {
        let l = min(max(lightness, 0), 1)
        var low = 0.0
        var high = max(0, chroma)
        var color = linear(l, high, hue)
        if min(color.0, color.1, color.2) < -0.000001 || max(color.0, color.1, color.2) > 1.000001 {
            for _ in 0..<20 {
                let middle = (low + high) / 2
                if inGamut(linear(l, middle, hue)) { low = middle } else { high = middle }
            }
            color = linear(l, low, hue)
        }
        func encode(_ value: Double) -> Int {
            let channel = min(max(value, 0), 1)
            let encoded = channel <= 0.0031308 ? 12.92 * channel : 1.055 * pow(channel, 1 / 2.4) - 0.055
            return min(max(Int((encoded * 255).rounded()), 0), 255)
        }
        return encode(color.0) << 16 | encode(color.1) << 8 | encode(color.2)
    }

    public static func rgb(_ color: Oklch) -> RGB { rgb(color.l, color.c, color.h) }

    /// Mixes in OKLab; `weight` is the share of `first`.
    public static func mix(_ first: RGB, _ second: RGB, _ weight: Double) -> RGB {
        let one = oklch(first), two = oklch(second)
        let a = weight * one.c * cos(one.h * .pi / 180) + (1 - weight) * two.c * cos(two.h * .pi / 180)
        let b = weight * one.c * sin(one.h * .pi / 180) + (1 - weight) * two.c * sin(two.h * .pi / 180)
        return rgb(weight * one.l + (1 - weight) * two.l, hypot(a, b), atan2(b, a) * 180 / .pi)
    }

    /// Flattens `foreground` at `alpha` over an opaque `background`.
    public static func composite(_ foreground: RGB, _ alpha: Double, _ background: RGB) -> RGB {
        func channel(_ front: Int, _ back: Int) -> Int { Int((alpha * Double(front) + (1 - alpha) * Double(back)).rounded()) }
        return channel(red(foreground), red(background)) << 16
            | channel(green(foreground), green(background)) << 8
            | channel(blue(foreground), blue(background))
    }

    public static func luminance(_ rgb: RGB) -> Double {
        decode(red(rgb)) * 0.2126 + decode(green(rgb)) * 0.7152 + decode(blue(rgb)) * 0.0722
    }

    public static func contrast(_ first: RGB, _ second: RGB) -> Double {
        let a = luminance(first), b = luminance(second)
        return (max(a, b) + 0.05) / (min(a, b) + 0.05)
    }

    public static func minimumContrast(_ foreground: RGB, _ backgrounds: [RGB]) -> Double {
        backgrounds.map { contrast(foreground, $0) }.min() ?? 0
    }

    /// Walks lightness in 0.001 steps until `color` reaches `ratio` against every background.
    /// Returns nil when no lightness satisfies it.
    public static func against(_ color: Oklch, _ backgrounds: [RGB], _ ratio: Double, darker: Bool) -> RGB? {
        for step in 0...1000 {
            let candidate = color.l + (darker ? -1.0 : 1.0) * Double(step) * 0.001
            if candidate < 0 || candidate > 1 { break }
            let foreground = rgb(candidate, color.c, color.h)
            if minimumContrast(foreground, backgrounds) >= ratio { return foreground }
        }
        return nil
    }
}
