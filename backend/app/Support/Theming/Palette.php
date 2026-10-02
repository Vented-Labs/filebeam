<?php

declare(strict_types=1);

namespace App\Support\Theming;

use App\Support\InstanceSettingValue;

final class Palette
{
    public const DEFAULT_PRIMARY = '#8b35ff';

    public const VERSION = '1';

    public readonly string $primary;

    /** @var array{float, float, float} */
    private array $seed;

    /** @var array{float, float, float} */
    private array $anchor;

    /** @var array<string, array<string, string>> */
    private array $palettes = [];

    public function __construct(string $primary = self::DEFAULT_PRIMARY)
    {
        $this->primary = InstanceSettingValue::color($primary);
        $this->seed = Color::oklch($this->primary);
        $this->anchor = Color::oklch(self::DEFAULT_PRIMARY);
    }

    /** @return array<string, string> */
    public function tokens(string $mode = 'dark'): array
    {
        if (isset($this->palettes[$mode])) {
            return $this->palettes[$mode];
        }
        $css = (string) file_get_contents(__DIR__.'/../../../resources/themes/default.css');
        preg_match_all('/(--fb-[a-z0-9-]+):\s*([^;]+);/s', $css, $matches, PREG_SET_ORDER);
        $tokens = [];
        foreach ($matches as $match) {
            $tokens[$match[1]] = $this->value(trim($match[2]), $mode, $match[1]);
        }
        $tokens['--fb-color-scheme'] = $mode === 'light' ? 'light' : 'dark';
        if ($mode === 'light' || $this->primary !== self::DEFAULT_PRIMARY) {
            foreach (['--fb-control-border', '--fb-focus'] as $name) {
                $tokens[$name] = Color::legible($tokens[$name], $tokens['--fb-surface-sunken'], 3, $mode === 'light');
            }
        }

        return $this->palettes[$mode] = $tokens;
    }

    public function css(): string
    {
        $declarations = static fn (array $tokens): string => implode('', array_map(static fn (string $name, string $value): string => $name.':'.$value.';', array_keys($tokens), array_values($tokens)));
        $dark = $this->tokens();
        $light = $declarations(array_diff_assoc($this->tokens('light'), $dark));

        return ':root:root{'.$declarations($dark).'}:root[data-fb-theme="light"]{'.$light.'}@media(prefers-color-scheme:light){:root:not([data-fb-theme]){'.$light.'}}';
    }

    public function value(string $value, string $mode = 'dark', string $role = ''): string
    {
        if ($mode !== 'light' && $this->primary === self::DEFAULT_PRIMARY) {
            return $value;
        }

        $value = (string) preg_replace_callback('/#[0-9a-f]{3,8}\b/i', fn (array $match): string => $this->color($match[0], $mode, $role), $value);

        $value = (string) preg_replace_callback('/rgba?\(([^)]+)\)/i', function (array $match) use ($mode, $role): string {
            $parts = preg_split('/[\s,\/]+/', trim($match[1]));
            if (! is_array($parts) || count($parts) < 3 || ! is_numeric($parts[0]) || ! is_numeric($parts[1]) || ! is_numeric($parts[2])) {
                return $match[0];
            }
            $hex = sprintf('#%02x%02x%02x', (int) $parts[0], (int) $parts[1], (int) $parts[2]);
            $channels = Color::channels($this->color($hex, $mode, $role));
            $alpha = $parts[3] ?? '1';

            return 'rgb('.implode(' ', $channels).' / '.$alpha.')';
        }, $value);

        return (string) preg_replace_callback('/oklch\(([\d.]+)(%?)\s+([\d.]+)\s+([\d.]+)\)/', function (array $match) use ($mode, $role): string {
            $hex = Color::hex((float) $match[1] / ($match[2] === '%' ? 100 : 1), (float) $match[3], (float) $match[4]);

            return $this->color($hex, $mode, $role);
        }, $value);
    }

    public function color(string $hex, string $mode = 'dark', string $role = ''): string
    {
        $hex = strtolower($hex);
        if (strlen($hex) === 4 || strlen($hex) === 5) {
            $hex = '#'.implode('', array_map(static fn (string $digit): string => $digit.$digit, str_split(substr($hex, 1))));
        }
        if ($mode !== 'light' && $this->primary === self::DEFAULT_PRIMARY) {
            return $hex;
        }
        $alpha = substr($hex, 7);
        $base = substr($hex, 0, 7);
        [$lightness, $chroma, $hue] = Color::oklch($base);
        $brand = $chroma > 0.002 && $hue >= 270 && $hue <= 345;
        if ($brand) {
            $hue = fmod($hue + $this->seed[2] - $this->anchor[2] + 360, 360);
            $chroma *= min(1.4, $this->seed[1] / $this->anchor[1]);
        }
        $shadow = str_contains($role, 'shadow') || str_contains($role, 'scrim');
        $action = in_array($role, ['--fb-action', '--fb-action-hover', '--fb-action-active'], true);
        if ($role === '--fb-on-action') {
            return '#ffffff';
        }
        if ($mode === 'light' && $role === '--fb-selection') {
            $lightness = 0.86;
            $chroma = min($chroma, 0.08);
        } elseif ($action) {
            $lightness = min($lightness, 0.55);
        } elseif ($mode === 'light' && ! $shadow) {
            // Dark surfaces become near-white; pale labels become ink. Saturated marks retain depth.
            $lightness = $chroma > 0.15 && $lightness > 0.4 && $lightness < 0.75
                ? min($lightness, 0.52)
                : max(0.18, min(0.985, 1.15 - $lightness));
        }
        if ($base === self::DEFAULT_PRIMARY && $mode !== 'light' && ! $action && $this->seed[0] >= 0.45 && $this->seed[0] <= 0.85) {
            return $this->primary.$alpha;
        }

        return Color::hex($lightness, $chroma, $hue).$alpha;
    }
}
