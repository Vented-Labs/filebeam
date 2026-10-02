<?php

declare(strict_types=1);

use App\Enums\ThemePreset;
use App\Support\Theming\Appearance;
use App\Support\Theming\Color;
use App\Support\Theming\Palette;

require dirname(__DIR__, 2).'/backend/vendor/autoload.php';

if (($argv[1] ?? '') === '--asset') {
    $preset = ThemePreset::from($argv[2]);
    $name = $argv[3];
    $source = match ($name) {
        'logo', 'logo-light' => 'filebeam-logo-header.svg',
        'mark' => 'filebeam-mark.svg',
    };
    echo (new Palette($preset->primary(Palette::DEFAULT_PRIMARY)))->value((string) file_get_contents(dirname(__DIR__, 2).'/backend/public/brand/'.$source), $name === 'logo-light' ? 'light' : 'dark');
    exit;
}

if (($argv[1] ?? '') === '--gallery') {
    $catalog = [];
    foreach (ThemePreset::cases() as $preset) {
        $palette = new Palette($preset->primary(Palette::DEFAULT_PRIMARY));
        $base = '/_theme-preview/'.$preset->value.'/';
        $catalog[] = [
            'id' => $preset->value, 'label' => $preset->label(), 'primary' => $palette->primary,
            'on_color' => Color::contrast('#ffffff', $palette->primary) > Color::contrast('#000000', $palette->primary) ? '#ffffff' : '#000000',
            'branding' => [
                'default_logo_url' => $base.'logo.svg', 'default_light_logo_url' => $base.'logo-light.svg', 'default_mark_url' => $base.'mark.svg',
            ],
            'favicons' => [],
            'chrome' => ['dark' => $palette->tokens()['--fb-browser-chrome'], 'light' => $palette->tokens('light')['--fb-browser-chrome']],
        ];
    }
    echo json_encode([
        'css' => (new Palette)->css().Appearance::stylesheet(),
        'appearance' => [
            'account' => null, 'preference' => ['mode' => 'system', 'preset' => 'instance'],
            'needs_adoption' => false, 'custom_colors' => true, 'catalog' => $catalog,
            'csrf' => null, 'save_url' => '', 'styles_url' => '',
        ],
    ], JSON_HEX_TAG | JSON_HEX_AMP | JSON_THROW_ON_ERROR);
    exit;
}

echo (new Palette($argv[1] ?? Palette::DEFAULT_PRIMARY))->css();
