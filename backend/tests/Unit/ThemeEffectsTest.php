<?php

declare(strict_types=1);

use App\Support\Theming\Color;
use App\Support\Theming\Palette;
use App\Support\Theming\ThemeDefinition;

/** @return list<string> */
function effectColors(string $value): array
{
    preg_match_all('/#[0-9a-f]{6}(?:[0-9a-f]{2})?|rgb\([^)]+\)/i', $value, $matches);

    return $matches[0];
}

function effectSample(string $first, string $last, float $position, string $backing): string
{
    $a = Color::parse($first);
    $b = Color::parse($last);
    $alpha = (1 - $position) * $a[3] + $position * $b[3];
    if ($alpha === 0.0) {
        return $backing;
    }
    $channels = [];
    for ($i = 0; $i < 3; $i++) {
        $channels[] = (int) round(((1 - $position) * $a[$i] * $a[3] + $position * $b[$i] * $b[3]) / $alpha);
    }

    return Color::composite('rgb('.implode(' ', $channels).' / '.sprintf('%.8F', $alpha).')', $backing);
}

test('decorative composites retain readable text and toolbar boundaries', function (string $seed, string $mode) {
    $t = (new Palette($seed))->tokens($mode);
    $check = function (string $role, string $background, float $minimum, string $composition) use ($t, $seed, $mode): void {
        $ratio = Color::contrast($t[$role], $background);
        expect($ratio, "$seed $mode $role ({$t[$role]}) on $composition ($background): $ratio:1")->toBeGreaterThanOrEqual($minimum);
    };
    $ambient = effectColors($t['--fb-ambient-glow']);
    $secondary = $ambient[$mode === 'light' ? 3 : 1];
    for ($a = 0; $a <= 8; $a++) {
        for ($b = 0; $b <= 8; $b++) {
            $background = Color::composite($ambient[0], Color::composite($secondary, $t['--fb-bg'], $b / 8), $a / 8);
            foreach (['--fb-text', '--fb-text-muted', '--fb-text-subtle', '--fb-accent-text'] as $role) {
                $check($role, $background, 4.5, "ambient overlap $a/$b");
            }
        }
    }
    $dropzone = effectColors($t['--fb-dropzone-glow'])[0];
    $toolbar = effectColors($t['--fb-note-surface']);
    $emblem = effectColors($t['--fb-share-mark-surface']);
    for ($sample = 0; $sample <= 16; $sample++) {
        foreach ([0.65, 1.0] as $opacity) {
            $background = Color::composite($dropzone, $t['--fb-surface'], $opacity * $sample / 16);
            foreach (['--fb-text', '--fb-text-muted', '--fb-text-subtle', '--fb-accent-text'] as $role) {
                $check($role, $background, 4.5, "dropzone $opacity/$sample");
            }
        }
        $background = Color::composite($toolbar[0], $toolbar[array_key_last($toolbar)], $sample / 16);
        foreach (['--fb-text', '--fb-text-muted', '--fb-text-subtle'] as $role) {
            $check($role, $background, 4.5, "toolbar $sample");
        }
        foreach (['--fb-control-border', '--fb-focus'] as $role) {
            $check($role, $background, 3, "toolbar boundary $sample");
        }
        $background = $mode === 'light'
            ? Color::composite($emblem[0], $emblem[array_key_last($emblem)], $sample / 16)
            : effectSample($emblem[0], $emblem[1], $sample / 16, $t['--fb-surface']);
        $check('--fb-share-mark-text', $background, 4.5, "emblem $sample");
        $background = Color::composite($t['--fb-og-glow'], $t['--fb-bg'], $sample / 16);
        foreach (['--fb-text', '--fb-text-muted', '--fb-accent-text'] as $role) {
            $check($role, $background, 4.5, "OG $sample");
        }
    }
})->with([...array_values(ThemeDefinition::SEEDS), '#000000', '#ffffff', '#808080', '#010101', '#fefefe', '#ffff00', '#00ff00', '#00ffff', '#0000ff', '#ff00ff', '#008877', '#cc5500', '#f5e6d3', '#271339'])->with(['light', 'dark']);

test('the original default dark atmosphere and card geometry are preserved exactly', function () {
    $t = (new Palette)->tokens();
    expect($t['--fb-ambient-glow'])->toBe('radial-gradient(ellipse at 49% 12%, #5731821f, transparent 57%), radial-gradient(ellipse at 90% 39%, #34204313, transparent 45%)')
        ->and($t['--fb-dropzone-glow'])->toBe('radial-gradient(ellipse at 50% 24%, #a679ff13, transparent 55%)')
        ->and($t['--fb-note-surface'])->toBe('radial-gradient(ellipse at 30% 0%, #a679ff12, transparent 72%), #211b2b')
        ->and($t['--fb-share-mark-surface'])->toBe('radial-gradient(circle at 30% 0%, #a679ff28, #342740)')
        ->and($t['--fb-dropzone-mark'])->toBe('linear-gradient(145deg, #2e2639, #211b2b)')
        ->and($t['--fb-dropzone-mark-hover'])->toBe('linear-gradient(140deg, #3d2c4c, #2c2138)')
        ->and($t['--fb-og-glow'])->toBe('rgb(139 53 255 / 24%)')
        ->and($t['--fb-og-shadow'])->toBe('rgb(139 53 255 / 30%)')
        ->and($t['--fb-og-orbit'])->toBe('rgb(199 91 250 / 12%)');
});
