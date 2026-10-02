<?php

declare(strict_types=1);

use App\Support\Theming\Color;
use App\Support\Theming\MailTheme;
use App\Support\Theming\Palette;
use App\Support\Theming\ThemeDefinition;

test('email slots use explicit mixed-mode roles and literal colors', function (string $seed) {
    $palette = new Palette($seed);
    $slots = MailTheme::slots($palette);
    $css = MailTheme::css($palette);
    expect($css)->not->toContain('{{', 'var(', 'color-mix(', 'oklch(', 'gradient(')
        ->and($slots['SHELL_BG'])->toBe($palette->tokens('dark')['--fb-bg'])
        ->and($slots['CARD_BG'])->toBe('#ffffff')
        ->and($slots['BODY_TEXT'])->toBe('#262230')
        ->and($slots['SUCCESS_BG'])->toBe('#21693e')
        ->and($slots['DANGER_BG'])->toBe('#b4233f')
        ->and(Color::contrast($slots['ACTION_INK'], $slots['ACTION_BG']))->toBeGreaterThanOrEqual(4.8);
    foreach (['bottom' => 11, 'left' => 22, 'right' => 22, 'top' => 11] as $side => $size) {
        expect($css)->toContain("border-$side: {$size}px solid {$slots['ACTION_BG']}");
    }
    if (in_array($seed, [ThemeDefinition::SEEDS['amber'], ThemeDefinition::SEEDS['orange']], true)) {
        expect($slots['ACTION_INK'])->not->toBe('#ffffff');
    }
})->with([...array_values(ThemeDefinition::SEEDS), '#000000', '#ffffff', '#008877', '#cc5500']);
