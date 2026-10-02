<?php

declare(strict_types=1);

namespace App\Support\Theming;

use App\Support\InstanceSettingValue;
use RuntimeException;

final class Palette
{
    public const DEFAULT_PRIMARY = '#8b35ff';

    public const VERSION = '2';

    public readonly string $primary;

    /** @var array<string, array<string, string>> */
    private array $palettes = [];

    public function __construct(string $primary = self::DEFAULT_PRIMARY)
    {
        $this->primary = InstanceSettingValue::color($primary);
    }

    /** @return array<string, string> */
    public function tokens(string $mode = 'dark'): array
    {
        ThemeDefinition::mode($mode);
        if (isset($this->palettes[$mode])) {
            return $this->palettes[$mode];
        }
        $preset = array_search($this->primary, ThemeDefinition::SEEDS, true);
        if ($preset === false) {
            $custom = new CustomPalette($this->primary);
            $roles = $custom->tokens($mode);
            $artwork = ThemeDefinition::artwork($custom->anchor, $custom->endpoint, $mode, false);
        } else {
            $roles = ThemeDefinition::authored($preset, $mode);
            $artwork = ThemeDefinition::artwork($this->primary, ThemeDefinition::ENDPOINTS[$preset], $mode, $preset === 'purple');
        }
        $tokens = [
            ...ThemeDefinition::common(), ...ThemeDefinition::fixed($mode), ...$roles,
            ...EditorPalette::tokens($this->primary, $mode, $roles),
            ...ThemeEffects::tokens($this->primary, $mode, $roles),
            ...$artwork,
            '--fb-brand' => $this->primary,
            '--fb-progress-track' => $roles[$mode === 'light' ? '--fb-surface-active' : '--fb-border'],
        ];
        $tokens = TokenContract::resolve($tokens, ThemeDefinition::aliases());
        $tokens['--fb-focus-ring'] = '0 0 0 2px '.$tokens['--fb-focus'];
        TokenContract::validate($tokens, $this->primary.' '.$mode);

        return $this->palettes[$mode] = $tokens;
    }

    public function css(string $scope = '', bool $fallback = false): string
    {
        $declarations = static fn (array $tokens): string => implode('', array_map(static fn (string $name, string $value): string => $name.':'.$value.';', array_keys($tokens), array_values($tokens)));
        $dark = $this->tokens();
        $light = $declarations(array_diff_assoc($this->tokens('light'), $dark));
        $root = ':root'.$scope;

        // The bundled fallback must remain weaker than late or early runtime CSS.
        $base = $fallback ? $root : $root.':root';
        $select = static fn (string $selector): string => $root.($fallback ? ':where('.$selector.')' : $selector);

        return $base.','.$select('[data-fb-theme="dark"]').'{'.$declarations($dark).'}'.$select('[data-fb-theme="light"]').'{'.$light.'}@media(prefers-color-scheme:light){'.$select(':not([data-fb-theme])').'{'.$light.'}}';
    }

    /** @return array<int, string> */
    public function filamentPrimary(): array
    {
        $seed = in_array($this->primary, ThemeDefinition::SEEDS, true) ? $this->primary : (new CustomPalette($this->primary))->anchor;
        $ink = $this->tokens('light')['--fb-accent-text'];
        $ramp = [];
        foreach ([50 => 0.055, 100 => 0.12, 200 => 0.24, 300 => 0.40, 400 => 0.64] as $shade => $weight) {
            $ramp[$shade] = Color::mixOKLab($seed, '#ffffff', $weight);
        }
        $ramp[500] = $seed;
        $ramp[600] = $ink;
        foreach ([700 => 0.90, 800 => 0.80, 900 => 0.69, 950 => 0.55] as $shade => $weight) {
            $ramp[$shade] = Color::mixOKLab($ink, '#000000', $weight);
        }
        $previous = 1.0;
        foreach ($ramp as $color) {
            $l = Color::oklch($color)[0];
            if ($l > $previous) {
                throw new RuntimeException('Non-monotonic Filament primary ramp for '.$this->primary);
            }
            $previous = $l;
        }

        return $ramp;
    }

    /** @return list<string> */
    public static function sources(): array
    {
        return [__FILE__, __DIR__.'/ThemeDefinition.php', __DIR__.'/ThemeEffects.php', __DIR__.'/EditorPalette.php', __DIR__.'/CustomPalette.php', __DIR__.'/TokenContract.php', __DIR__.'/Color.php'];
    }
}
