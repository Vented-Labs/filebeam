//! Colour maths shared with the web theme generator.
//!
//! Results must stay identical to `backend/app/Support/Theming/Color.php`, so every
//! constant and rounding rule mirrors it. Colours are opaque sRGB packed as `0xRRGGBB`.

pub type Rgb = u32;

/// Lightness, chroma and hue in OKLCH. Hue is in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

pub fn parse(hex: &str) -> Option<Rgb> {
    let value = hex.trim();
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(value, 16).ok()
}

pub fn format(rgb: Rgb) -> String {
    format!("#{:06x}", rgb & 0xFF_FFFF)
}

fn red(rgb: Rgb) -> u32 {
    (rgb >> 16) & 0xFF
}

fn green(rgb: Rgb) -> u32 {
    (rgb >> 8) & 0xFF
}

fn blue(rgb: Rgb) -> u32 {
    rgb & 0xFF
}

fn decode(channel: u32) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub fn oklch(rgb: Rgb) -> Oklch {
    let (r, g, b) = (decode(red(rgb)), decode(green(rgb)), decode(blue(rgb)));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let bb = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    Oklch {
        l: 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        c: a.hypot(bb),
        h: (bb.atan2(a).to_degrees() + 360.0) % 360.0,
    }
}

fn linear(lightness: f64, chroma: f64, hue: f64) -> (f64, f64, f64) {
    let a = chroma * hue.to_radians().cos();
    let b = chroma * hue.to_radians().sin();
    let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powf(3.0);
    let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powf(3.0);
    let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powf(3.0);
    (
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
    )
}

fn in_gamut(color: (f64, f64, f64)) -> bool {
    color.0.min(color.1).min(color.2) >= 0.0 && color.0.max(color.1).max(color.2) <= 1.0
}

/// Converts OKLCH to sRGB, reducing chroma at the gamut boundary so hue is preserved.
pub fn rgb(lightness: f64, chroma: f64, hue: f64) -> Rgb {
    let l = lightness.clamp(0.0, 1.0);
    let mut low = 0.0;
    let mut high = chroma.max(0.0);
    let mut color = linear(l, high, hue);
    if color.0.min(color.1).min(color.2) < -0.000_001
        || color.0.max(color.1).max(color.2) > 1.000_001
    {
        for _ in 0..20 {
            let middle = (low + high) / 2.0;
            if in_gamut(linear(l, middle, hue)) {
                low = middle;
            } else {
                high = middle;
            }
        }
        color = linear(l, low, hue);
    }
    let encode = |value: f64| -> u32 {
        let channel = value.clamp(0.0, 1.0);
        let encoded = if channel <= 0.003_130_8 {
            12.92 * channel
        } else {
            1.055 * channel.powf(1.0 / 2.4) - 0.055
        };
        ((encoded * 255.0).round() as i64).clamp(0, 255) as u32
    };
    encode(color.0) << 16 | encode(color.1) << 8 | encode(color.2)
}

pub fn from_oklch(color: Oklch) -> Rgb {
    rgb(color.l, color.c, color.h)
}

/// Mixes in OKLab; `weight` is the share of `first`.
pub fn mix(first: Rgb, second: Rgb, weight: f64) -> Rgb {
    let (one, two) = (oklch(first), oklch(second));
    let a = weight * one.c * one.h.to_radians().cos()
        + (1.0 - weight) * two.c * two.h.to_radians().cos();
    let b = weight * one.c * one.h.to_radians().sin()
        + (1.0 - weight) * two.c * two.h.to_radians().sin();
    rgb(
        weight * one.l + (1.0 - weight) * two.l,
        a.hypot(b),
        b.atan2(a).to_degrees(),
    )
}

/// Flattens `foreground` at `alpha` over an opaque `background`.
pub fn composite(foreground: Rgb, alpha: f64, background: Rgb) -> Rgb {
    let channel = |front: u32, back: u32| -> u32 {
        (alpha * f64::from(front) + (1.0 - alpha) * f64::from(back)).round() as u32
    };
    channel(red(foreground), red(background)) << 16
        | channel(green(foreground), green(background)) << 8
        | channel(blue(foreground), blue(background))
}

pub fn luminance(rgb: Rgb) -> f64 {
    decode(red(rgb)) * 0.2126 + decode(green(rgb)) * 0.7152 + decode(blue(rgb)) * 0.0722
}

pub fn contrast(first: Rgb, second: Rgb) -> f64 {
    let (a, b) = (luminance(first), luminance(second));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

pub fn minimum_contrast(foreground: Rgb, backgrounds: &[Rgb]) -> f64 {
    backgrounds
        .iter()
        .map(|background| contrast(foreground, *background))
        .reduce(f64::min)
        .unwrap_or(0.0)
}

/// Walks lightness in 0.001 steps until `color` reaches `ratio` against every background.
/// Returns `None` when no lightness satisfies it.
pub fn against(color: Oklch, backgrounds: &[Rgb], ratio: f64, darker: bool) -> Option<Rgb> {
    let direction = if darker { -1.0 } else { 1.0 };
    for step in 0..=1000 {
        let candidate = color.l + direction * f64::from(step) * 0.001;
        if !(0.0..=1.0).contains(&candidate) {
            break;
        }
        let foreground = rgb(candidate, color.c, color.h);
        if minimum_contrast(foreground, backgrounds) >= ratio {
            return Some(foreground);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_rejects_malformed_values() {
        assert_eq!(parse("#8b35ff"), Some(0x8b35ff));
        assert_eq!(parse(" 8B35FF "), Some(0x8b35ff));
        assert_eq!(parse("#fff"), None);
        assert_eq!(parse("#gggggg"), None);
        assert_eq!(format(0x8b35ff), "#8b35ff");
    }

    #[test]
    fn srgb_round_trips_through_oklch() {
        for value in [0x8b35ff, 0x000000, 0xffffff, 0x808080, 0x12ab9f] {
            assert_eq!(from_oklch(oklch(value)), value);
        }
    }

    #[test]
    fn contrast_matches_the_wcag_definition() {
        assert!((contrast(0x000000, 0xffffff) - 21.0).abs() < 1e-9);
        assert!((contrast(0x777777, 0x777777) - 1.0).abs() < 1e-9);
    }
}
