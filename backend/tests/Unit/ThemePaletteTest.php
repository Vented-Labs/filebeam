<?php

declare(strict_types=1);

use App\Support\Theming\Color;
use App\Support\Theming\Palette;
use App\Support\Theming\ThemeDefinition;
use App\Support\Theming\TokenContract;

test('the default dark palette preserves identity and uses accessible role colors', function () {
    $tokens = (new Palette)->tokens();
    expect($tokens['--fb-bg'])->toBe('#100e16')
        ->and($tokens['--fb-text'])->toBe('#f4f1fa')
        ->and($tokens['--fb-brand'])->toBe('#8b35ff')
        ->and($tokens['--fb-surface'])->toBe('#191621')
        ->and($tokens['--fb-surface-raised'])->toBe('#211d2a')
        ->and($tokens['--fb-surface-sunken'])->toBe('#120f19')
        ->and($tokens['--fb-action'])->toBe('#7c3aed')
        ->and($tokens['--fb-action-bg'])->toBe('linear-gradient(180deg, #8545e6, #7c3aed)')
        ->and($tokens['--fb-danger'])->toBe('#f0a7b9');
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

test('authored profiles match the independent concrete output oracle', function (string $preset, string $mode) {
    $fixture = json_decode((string) file_get_contents(__DIR__.'/../Fixtures/theme-color-contract.json'), true, flags: JSON_THROW_ON_ERROR);
    $palette = new Palette($fixture['preset_seeds'][$preset]);
    $expected = $fixture['profiles'][$preset][$mode];
    $actual = $palette->tokens($mode);
    expect(array_keys($actual))->toEqualCanonicalizing(array_keys($expected));
    foreach ($expected as $name => $value) {
        expect(strtolower($actual[$name]), "$preset $mode $name")->toBe(strtolower($value));
    }
    foreach ($fixture['aliases'] as $alias => $target) {
        expect($actual[$alias], "$preset $mode $alias -> $target")->toBe($actual[$target]);
    }
    expect($palette->css())->toBe((new Palette($palette->primary))->css());
})->with(array_keys(ThemeDefinition::SEEDS))->with(['light', 'dark']);

test('role contrasts survive enabled states overlays and quantization', function (string $seed, string $mode) {
    $palette = new Palette($seed);
    $t = $palette->tokens($mode);
    $check = function (string $fg, string $bg, float $minimum, string $roles) use ($seed, $mode): void {
        $ratio = Color::contrast($fg, $bg);
        expect($ratio, "$seed $mode $roles ($fg on $bg): $ratio:1")->toBeGreaterThanOrEqual($minimum);
    };
    $backgrounds = ['bg', 'surface', 'surface-raised', 'surface-sunken', 'surface-hover', 'surface-active', 'selected-surface', 'accent-surface', 'settings-surface', 'footer-surface'];
    foreach ($backgrounds as $bg) {
        foreach (['text', 'text-muted', 'text-subtle', 'accent-text'] as $fg) {
            $check($t['--fb-'.$fg], $t['--fb-'.$bg], 4.5, "$fg / $bg");
            $check($t['--fb-'.$fg], Color::composite($t['--fb-wash-hover'], $t['--fb-'.$bg]), 4.5, "$fg / wash over $bg");
        }
        foreach (['control-border', 'control-border-hover', 'choice-border', 'focus'] as $fg) {
            $check($t['--fb-'.$fg], $t['--fb-'.$bg], 3, "$fg / $bg");
        }
    }
    foreach (['action', 'action-hover', 'action-active'] as $bg) {
        $check($t['--fb-on-action'], $t['--fb-'.$bg], 4.8, "on-action / $bg");
    }
    preg_match_all('/#[0-9a-f]{6}/', $t['--fb-action-bg'], $stops);
    [$top, $bottom] = count($stops[0]) === 2 ? $stops[0] : [$t['--fb-action'], $t['--fb-action']];
    for ($sample = 0; $sample <= 256; $sample++) {
        $check($t['--fb-on-action'], Color::composite($top, $bottom, $sample / 256), 4.8, "action gradient sample $sample");
    }
    $check($t['--fb-progress-fill'], $t['--fb-progress-track'], 3, 'progress');
    $check($t['--fb-switch-thumb-off'], $t['--fb-switch-track-off'], 3, 'off switch');
    $check($t['--fb-switch-thumb-checked'], $t['--fb-action'], 3, 'checked switch');
    foreach (['success', 'warning', 'danger', 'info'] as $meaning) {
        $check($t['--fb-'.$meaning], $t['--fb-'.$meaning.'-surface'], 4.5, $meaning);
        $check($t['--fb-'.$meaning.'-border'], $t['--fb-'.$meaning.'-surface'], 3, $meaning.' border');
    }
    $syntax = explode(' ', 'fg keyword string number constant function property type tag attribute comment punctuation operator meta regexp escape link heading invalid');
    foreach ($syntax as $fg) {
        $check($t['--fb-editor-'.$fg], $t['--fb-editor-bg'], 5.5, "editor $fg plain");
        foreach (['bg', 'selection', 'selection-inactive', 'matching-bracket', 'search-match'] as $bg) {
            $check($t['--fb-editor-'.$fg], $t['--fb-editor-'.$bg], 4.8, "editor $fg / $bg");
            $check($t['--fb-editor-'.$fg], Color::composite($t['--fb-editor-active-line'], $t['--fb-editor-'.$bg]), 4.8, "editor $fg / active line over $bg");
        }
    }
    $default = (new Palette)->tokens($mode);
    foreach ($t as $name => $value) {
        if (preg_match('/^--fb-(editor-|success|warning|danger|info)/', $name)) {
            expect($value, "$seed $mode $name remains semantic")->toBe($default[$name]);
        }
    }
    expect($palette->primary)->toBe(strtolower($seed));
    expect(array_keys($palette->filamentPrimary()))->toBe([50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950]);
})->with([...array_values(ThemeDefinition::SEEDS), '#000000', '#ffffff', '#808080', '#010101', '#fefefe', '#ffff00', '#00ff00', '#00ffff', '#0000ff', '#ff00ff', '#008877', '#cc5500', '#f5e6d3', '#271339'])->with(['light', 'dark']);

test('color parsing rejects unsupported values and uncomposited alpha', function (string $value) {
    expect(fn () => Color::contrast($value, '#ffffff'))->toThrow(InvalidArgumentException::class);
})->with(['garbage', '#12', '#gggggg', '#12345', 'var(--fb-text)', 'linear-gradient(black, white)', '#ffffff80', 'rgb(256 0 0)', 'rgb(0 0 0 / 0.5)']);

test('alpha and OKLab mixing have explicit independent semantics', function () {
    expect(Color::composite('#ffffff80', '#000000'))->toBe('#808080')
        ->and(Color::composite('rgb(255 255 255 / 0.5)', '#000000', 0.5))->toBe('#404040')
        ->and(Color::mixOKLab('#ffffff', '#000000', 0.5))->toBe('#636363')
        ->and(fn () => (new Palette)->tokens('system'))->toThrow(InvalidArgumentException::class)
        ->and(fn () => TokenContract::resolve([], ['--fb-a' => '--fb-b', '--fb-b' => '--fb-a']))->toThrow(RuntimeException::class, 'cycle')
        ->and(fn () => TokenContract::resolve([], ['--fb-a' => '--fb-missing']))->toThrow(RuntimeException::class, 'Undefined');
});

test('achromatic custom seeds have no arbitrary hue and retain highlight polarity', function (string $seed, string $mode) {
    $t = (new Palette($seed))->tokens($mode);
    foreach (['brand', 'accent-text', 'action', 'bg', 'selected-surface', 'progress-fill'] as $role) {
        $rgb = Color::channels($t['--fb-'.$role]);
        expect(max($rgb) - min($rgb))->toBe(0);
    }
    expect($t['--fb-progress-sheen'])->toBe($mode === 'light' ? 'transparent' : 'rgb(255 255 255 / 0.04)');
})->with(['#000000', '#ffffff', '#808080', '#010101', '#fefefe'])->with(['light', 'dark']);
