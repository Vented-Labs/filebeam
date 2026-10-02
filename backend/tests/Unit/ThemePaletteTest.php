<?php

declare(strict_types=1);

use App\Support\Theming\Color;
use App\Support\Theming\Palette;

test('the default dark palette preserves the existing design exactly', function () {
    $tokens = (new Palette)->tokens();
    expect($tokens['--fb-bg'])->toBe('#100e16')
        ->and($tokens['--fb-text'])->toBe('#f4f1fa')
        ->and($tokens['--fb-brand'])->toBe('#8b35ff')
        ->and($tokens['--fb-action'])->toBe('linear-gradient(180deg, #9250ec, #7e3dd8)')
        ->and($tokens['--fb-danger'])->toBe('#f3a9b6');
});

test('generated themes have distinct surfaces and legible main text', function (string $primary, string $mode) {
    $palette = new Palette($primary);
    $tokens = $palette->tokens($mode);
    expect(Color::contrast($tokens['--fb-text'], $tokens['--fb-bg']))->toBeGreaterThanOrEqual(4.5)
        ->and(Color::contrast($tokens['--fb-text-muted'], $tokens['--fb-surface']))->toBeGreaterThanOrEqual(4.5)
        ->and(Color::contrast($tokens['--fb-control-border'], $tokens['--fb-surface-sunken']))->toBeGreaterThanOrEqual(3)
        ->and(Color::contrast($tokens['--fb-focus'], $tokens['--fb-surface-sunken']))->toBeGreaterThanOrEqual(3)
        ->and($tokens['--fb-bg'])->not->toBe($tokens['--fb-surface'])
        ->and($tokens)->toBe((new Palette($primary))->tokens($mode));
})->with(['#008877', '#ff8800', '#ffff00', '#000000', '#ffffff', '#808080'])->with(['dark', 'light']);

test('OKLCH conversion round trips RGB without clipping', function (string $hex) {
    expect(Color::hex(...Color::oklch($hex)))->toBe($hex);
})->with(['#8b35ff', '#008877', '#ffffff', '#000000', '#ff0000', '#0000ff']);
