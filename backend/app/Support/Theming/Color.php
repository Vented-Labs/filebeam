<?php

declare(strict_types=1);

namespace App\Support\Theming;

use InvalidArgumentException;
use RuntimeException;

final class Color
{
    /** @return array{float, float, float} OKLCH */
    public static function oklch(string $hex): array
    {
        $rgb = array_map(static function (int $channel): float {
            $value = $channel / 255;

            return $value <= 0.04045 ? $value / 12.92 : (($value + 0.055) / 1.055) ** 2.4;
        }, self::channels($hex));
        [$r, $g, $b] = $rgb;
        $l = (0.4122214708 * $r + 0.5363325363 * $g + 0.0514459929 * $b) ** (1 / 3);
        $m = (0.2119034982 * $r + 0.6806995451 * $g + 0.1073969566 * $b) ** (1 / 3);
        $s = (0.0883024619 * $r + 0.2817188376 * $g + 0.6299787005 * $b) ** (1 / 3);
        $a = 1.9779984951 * $l - 2.428592205 * $m + 0.4505937099 * $s;
        $b = 0.0259040371 * $l + 0.7827717662 * $m - 0.808675766 * $s;

        return [0.2104542553 * $l + 0.793617785 * $m - 0.0040720468 * $s, hypot($a, $b), fmod(rad2deg(atan2($b, $a)) + 360, 360)];
    }

    /** @return array{int, int, int} */
    public static function channels(string $hex): array
    {
        [$r, $g, $b, $alpha] = self::parse($hex);
        if ($alpha !== 1.0) {
            throw new InvalidArgumentException('Composite translucent colors before calculating luminance: '.$hex);
        }

        return [$r, $g, $b];
    }

    /** @return array{int, int, int, float} Encoded sRGB and alpha. */
    public static function parse(string $value): array
    {
        if ($value === 'transparent') {
            return [0, 0, 0, 0.0];
        }
        if (preg_match('/^#(?:[a-f0-9]{3}|[a-f0-9]{4}|[a-f0-9]{6}|[a-f0-9]{8})$/iD', $value)) {
            $hex = substr($value, 1);
            if (strlen($hex) <= 4) {
                $hex = implode('', array_map(static fn (string $digit): string => $digit.$digit, str_split($hex)));
            }

            return [(int) hexdec(substr($hex, 0, 2)), (int) hexdec(substr($hex, 2, 2)), (int) hexdec(substr($hex, 4, 2)), strlen($hex) === 8 ? hexdec(substr($hex, 6, 2)) / 255 : 1.0];
        }
        if (preg_match('/^rgb\(\s*(\d{1,3})\s+(\d{1,3})\s+(\d{1,3})(?:\s*\/\s*(0(?:\.\d+)?|1(?:\.0+)?))?\s*\)$/D', $value, $m) && max((int) $m[1], (int) $m[2], (int) $m[3]) <= 255) {
            return [(int) $m[1], (int) $m[2], (int) $m[3], isset($m[4]) ? (float) $m[4] : 1.0];
        }

        throw new InvalidArgumentException('Unsupported sRGB color: '.$value);
    }

    public static function composite(string $foreground, string $background, float $opacity = 1): string
    {
        if (! is_finite($opacity) || $opacity < 0 || $opacity > 1) {
            throw new InvalidArgumentException('Opacity must be finite and within [0,1].');
        }
        [$r, $g, $b, $alpha] = self::parse($foreground);
        $alpha *= $opacity;
        $back = self::channels($background);
        $out = [];
        foreach ([$r, $g, $b] as $i => $channel) {
            $out[] = (int) round($alpha * $channel + (1 - $alpha) * $back[$i]);
        }

        return sprintf('#%02x%02x%02x', ...$out);
    }

    public static function mixOKLab(string $first, string $second, float $weight): string
    {
        if (! is_finite($weight) || $weight < 0 || $weight > 1) {
            throw new InvalidArgumentException('Color weight must be finite and within [0,1].');
        }
        [$l1, $c1, $h1] = self::oklch($first);
        [$l2, $c2, $h2] = self::oklch($second);
        $a = $weight * $c1 * cos(deg2rad($h1)) + (1 - $weight) * $c2 * cos(deg2rad($h2));
        $b = $weight * $c1 * sin(deg2rad($h1)) + (1 - $weight) * $c2 * sin(deg2rad($h2));

        return self::hex($weight * $l1 + (1 - $weight) * $l2, hypot($a, $b), rad2deg(atan2($b, $a)));
    }

    public static function hex(float $lightness, float $chroma, float $hue): string
    {
        if (! is_finite($lightness) || ! is_finite($chroma) || ! is_finite($hue)) {
            throw new InvalidArgumentException('OKLCH coordinates must be finite.');
        }
        $lightness = max(0, min(1, $lightness));
        $low = 0.0;
        $high = max(0, $chroma);
        $rgb = self::linear($lightness, $high, $hue);
        // Reduce chroma instead of clipping channels, preserving hue at the gamut boundary.
        if (min($rgb) < -0.000001 || max($rgb) > 1.000001) {
            for ($i = 0; $i < 20; $i++) {
                $middle = ($low + $high) / 2;
                $candidate = self::linear($lightness, $middle, $hue);
                if (min($candidate) >= 0 && max($candidate) <= 1) {
                    $low = $middle;
                } else {
                    $high = $middle;
                }
            }
            $rgb = self::linear($lightness, $low, $hue);
        }

        return '#'.implode('', array_map(static function (float $channel): string {
            $channel = max(0, min(1, $channel));
            $encoded = $channel <= 0.0031308 ? 12.92 * $channel : 1.055 * $channel ** (1 / 2.4) - 0.055;

            return sprintf('%02x', (int) round($encoded * 255));
        }, $rgb));
    }

    /** @return array{float, float, float} */
    private static function linear(float $lightness, float $chroma, float $hue): array
    {
        $a = $chroma * cos(deg2rad($hue));
        $b = $chroma * sin(deg2rad($hue));
        $l = ($lightness + 0.3963377774 * $a + 0.2158037573 * $b) ** 3;
        $m = ($lightness - 0.1055613458 * $a - 0.0638541728 * $b) ** 3;
        $s = ($lightness - 0.0894841775 * $a - 1.291485548 * $b) ** 3;

        return [4.0767416621 * $l - 3.3077115913 * $m + 0.2309699292 * $s, -1.2684380046 * $l + 2.6097574011 * $m - 0.3413193965 * $s, -0.0041960863 * $l - 0.7034186147 * $m + 1.707614701 * $s];
    }

    public static function contrast(string $first, string $second): float
    {
        $luminance = static function (string $hex): float {
            $values = array_map(static function (int $channel): float {
                $value = $channel / 255;

                return $value <= 0.04045 ? $value / 12.92 : (($value + 0.055) / 1.055) ** 2.4;
            }, self::channels($hex));

            return $values[0] * 0.2126 + $values[1] * 0.7152 + $values[2] * 0.0722;
        };
        $a = $luminance($first);
        $b = $luminance($second);

        return (max($a, $b) + 0.05) / (min($a, $b) + 0.05);
    }

    public static function legible(string $foreground, string $background, float $ratio, bool $light): string
    {
        return self::against(self::oklch($foreground), [$background], $ratio, $light);
    }

    /** @param array{float, float, float} $oklch
     * @param  list<string>  $backgrounds
     */
    public static function against(array $oklch, array $backgrounds, float $ratio, bool $darker): string
    {
        [$l, $c, $h] = $oklch;
        for ($step = 0; $step <= 1000; $step++) {
            $candidate = $l + ($darker ? -1 : 1) * $step * 0.001;
            if ($candidate < 0 || $candidate > 1) {
                break;
            }
            $foreground = self::hex($candidate, $c, $h);
            if (self::minimumContrast($foreground, $backgrounds) >= $ratio) {
                return $foreground;
            }
        }

        throw new RuntimeException('Cannot satisfy '.$ratio.':1 contrast against '.implode(', ', $backgrounds));
    }

    /** @param list<string> $backgrounds */
    public static function minimumContrast(string $foreground, array $backgrounds): float
    {
        if ($backgrounds === []) {
            throw new InvalidArgumentException('At least one contrast background is required.');
        }

        return min(array_map(static fn (string $background): float => self::contrast($foreground, $background), $backgrounds));
    }
}
