<?php

declare(strict_types=1);

use App\Support\Theming\BrandArtwork;
use App\Support\Theming\Color;
use App\Support\Theming\LegacyArtworkPalette;
use App\Support\Theming\Palette;
use App\Support\Theming\ThemeDefinition;

test('SVG artwork retains geometry and universal glyphs with explicit wordmark inks', function (string $seed) {
    $palette = new Palette($seed);
    $dark = BrandArtwork::render('logo.svg', $palette, 'dark');
    $light = BrandArtwork::render('logo.svg', $palette, 'light');
    $parse = static function (string $svg): DOMXPath {
        $doc = new DOMDocument;
        $doc->loadXML($svg, LIBXML_NONET);
        $xpath = new DOMXPath($doc);
        $xpath->registerNamespace('svg', 'http://www.w3.org/2000/svg');

        return $xpath;
    };
    $base = $parse((string) file_get_contents(__DIR__.'/../../public/brand/filebeam-logo-header.svg'));
    foreach (['light' => $light, 'dark' => $dark] as $mode => $svg) {
        $xpath = $parse($svg);
        expect($xpath->evaluate('string(//svg:path[@data-part="wordmark"]/@fill)'))->toBe($mode === 'light' ? '#262230' : '#f4f1fa');
        foreach (['viewBox', 'd', 'transform', 'x1', 'y1', 'x2', 'y2', 'offset', 'fill-rule', 'gradientUnits'] as $attribute) {
            $values = static fn (DOMXPath $x): array => array_map(static fn (DOMNode $node): string => $node->nodeValue, iterator_to_array($x->query('//@'.$attribute)));
            expect($values($xpath), "$seed $mode $attribute")->toBe($values($base));
        }
    }
    expect(str_replace('#262230', '#f4f1fa', $light))->toBe($dark)
        ->and(BrandArtwork::render('mark.svg', $palette, 'light'))->toBe(BrandArtwork::render('mark.svg', $palette, 'dark'));
})->with([...array_values(ThemeDefinition::SEEDS), '#000000', '#ffffff', '#cc5500']);

test('the default universal glyph retains all authored face stops', function () {
    $svg = BrandArtwork::render('mark.svg', new Palette);
    foreach (['#6421d8', '#8b35ff', '#c75bfa', '#c568f5', '#e7a1ff', '#5020b5', '#7228e8', '#9d3aff', '#963bff'] as $color) {
        expect($svg)->toContain($color);
    }
    expect(fn () => BrandArtwork::render('../untrusted.svg', new Palette))->toThrow(InvalidArgumentException::class);
});

test('legacy raster gradients do not jump in lightness at the exact identity pixel', function (string $seed) {
    $adapter = new LegacyArtworkPalette(new Palette($seed));
    $center = Color::oklch($adapter->color(Palette::DEFAULT_PRIMARY))[0];
    foreach (['#8b34ff', '#8b36ff', '#8a35ff', '#8c35ff'] as $neighbor) {
        expect(abs(Color::oklch($adapter->color($neighbor))[0] - $center))->toBeLessThan(0.01);
    }
})->with(array_values(ThemeDefinition::SEEDS));
