<?php

declare(strict_types=1);

namespace App\Support\Theming;

use RuntimeException;

final class CustomPalette
{
    private readonly float $chroma;

    private readonly float $hue;

    public readonly string $anchor;

    public readonly string $endpoint;

    public function __construct(string $seed)
    {
        [, $chroma, $hue] = Color::oklch($seed);
        $this->chroma = $chroma < 0.01 ? 0 : $chroma;
        $this->hue = $chroma < 0.01 ? 0 : $hue;
        $this->anchor = Color::hex(0.65, min($this->chroma, 0.14), $this->hue);
        $this->endpoint = Color::hex(0.72, min($this->chroma, 0.15), $this->hue);
    }

    /** @return array<string, string> */
    public function tokens(string $mode): array
    {
        $light = ThemeDefinition::mode($mode) === 'light';
        $targets = [
            'browser-chrome' => [0.977, 0.004], 'bg' => [0.977, 0.004], 'surface' => [0.996, 0.001],
            'surface-raised' => [1.0, 0.0], 'surface-sunken' => [0.961, 0.006],
            'footer-surface' => [0.970, 0.004], 'settings-surface' => [0.970, 0.004],
            'border' => [0.867, 0.009], 'control-border' => [0.565, 0.012],
            'control-border-hover' => [0.485, 0.016], 'surface-hover' => [0.944, 0.009], 'surface-active' => [0.924, 0.011],
        ];
        $tokens = [];
        foreach (ThemeDefinition::neutrals('purple', 'dark') as $name => $source) {
            [$l, $c] = Color::oklch($source);
            $border = str_contains($name, 'border');
            if ($light) {
                [$l, $cap] = $targets[substr($name, 5)];
                $c = min($cap, $this->chroma * 0.15);
            } else {
                $c = min($c, $border ? 0.018 : 0.012, $this->chroma * ($border ? 0.20 : 0.15));
            }
            $tokens[$name] = Color::hex($l, $c, $this->hue);
        }
        $tokens['--fb-accent-surface'] = Color::mixOKLab($this->anchor, $tokens['--fb-surface'], $light ? 0.07 : 0.12);
        $tokens['--fb-selected-surface'] = Color::mixOKLab($this->anchor, $tokens['--fb-surface'], $light ? 0.12 : 0.21);
        $backgrounds = array_values(array_diff_key($tokens, array_flip(['--fb-border', '--fb-control-border', '--fb-control-border-hover'])));
        $tokens['--fb-accent-text'] = Color::against([$light ? 0.43 : 0.79, min($this->chroma, $light ? 0.11 : 0.10), $this->hue], $backgrounds, 4.8, $light);
        foreach (['--fb-control-border', '--fb-control-border-hover'] as $name) {
            $tokens[$name] = Color::against(Color::oklch($tokens[$name]), $backgrounds, 3, $light);
        }
        $warm = $this->hue >= 45 && $this->hue <= 105 && $this->chroma >= 0.04;
        $ink = $warm ? '#2b2318' : '#ffffff';
        $levels = $warm ? [0.78, 0.80, 0.75, 0.72] : [0.50, 0.525, 0.46, 0.42];
        $c = min($this->chroma, $warm ? 0.14 : 0.19);
        $fills = [];
        for ($step = 0; $step <= 1000; $step++) {
            $shift = ($warm ? 1 : -1) * $step * 0.001;
            if (min($levels) + $shift < 0 || max($levels) + $shift > 1) {
                throw new RuntimeException('Cannot derive an ordered, legible custom action family.');
            }
            $fills = array_map(fn (float $l): string => Color::hex($l + $shift, $c, $this->hue), $levels);
            if (Color::minimumContrast($ink, $fills) >= 4.8) {
                break;
            }
        }
        [$solid, $top, $hover, $active] = $fills;
        $border = $light ? $tokens['--fb-accent-text'] : Color::against([0.67, min($this->chroma, 0.09), $this->hue], $backgrounds, 3, false);
        $track = $tokens[$light ? '--fb-surface-active' : '--fb-border'];

        return [...$tokens,
            '--fb-action' => $solid, '--fb-action-hover' => $hover, '--fb-action-active' => $active, '--fb-on-action' => $ink,
            '--fb-action-bg' => $light ? $solid : "linear-gradient(180deg, $top, $solid)",
            '--fb-action-border' => $border, '--fb-choice-border' => $border,
            '--fb-progress-fill' => $light ? $tokens['--fb-accent-text'] : Color::against([0.74, min($this->chroma, 0.10), $this->hue], [$track], 3.5, false),
        ];
    }
}
