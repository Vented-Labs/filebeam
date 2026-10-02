<?php

declare(strict_types=1);

namespace App\Support\Theming;

use DOMDocument;
use DOMElement;
use DOMXPath;
use InvalidArgumentException;
use RuntimeException;

final class BrandArtwork
{
    public static function render(string $name, Palette $palette, string $mode = 'dark'): string
    {
        ThemeDefinition::mode($mode);
        $file = match ($name) {
            'logo.svg' => 'brand/filebeam-logo-header.svg',
            'mark.svg' => 'brand/filebeam-mark.svg',
            'favicon.svg' => 'favicon.svg',
            default => throw new InvalidArgumentException('Unknown bundled SVG artwork: '.$name),
        };
        $document = new DOMDocument;
        if (! $document->load(__DIR__.'/../../../public/'.$file, LIBXML_NONET)) {
            throw new RuntimeException('Unable to load bundled artwork.');
        }
        $xpath = new DOMXPath($document);
        $xpath->registerNamespace('svg', 'http://www.w3.org/2000/svg');
        $default = $palette->primary === Palette::DEFAULT_PRIMARY;
        $preset = array_search($palette->primary, ThemeDefinition::SEEDS, true);
        if ($preset === false) {
            $custom = new CustomPalette($palette->primary);
            $s = $custom->anchor;
            $e = $custom->endpoint;
        } else {
            $s = $palette->primary;
            $e = ThemeDefinition::ENDPOINTS[$preset];
        }
        $stops = $default ? [
            'top' => ['#6421d8', '#8b35ff', '#c75bfa'], 'fold' => ['#c568f5', '#e7a1ff'],
            'beam' => ['#5020b5', '#7228e8', '#9d3aff'], 'stem' => ['#8b35ff', '#963bff'],
        ] : [
            'top' => [Color::mixOKLab($s, '#000000', 0.78), $s, $e],
            'fold' => [Color::mixOKLab($e, '#ffffff', 0.82), Color::mixOKLab($e, '#ffffff', 0.56)],
            'beam' => [Color::mixOKLab($s, '#000000', 0.68), Color::mixOKLab($s, '#000000', 0.88), Color::mixOKLab($s, $e, 0.78)],
            'stem' => [$s, Color::mixOKLab($s, $e, 0.84)],
        ];
        foreach ($stops as $part => $colors) {
            $nodes = $xpath->query('//svg:linearGradient[substring(@id, string-length(@id) - '.(strlen($part) - 1).') = "'.$part.'"]/svg:stop');
            foreach ($nodes ?: [] as $index => $node) {
                if ($node instanceof DOMElement) {
                    self::set($node, 'stop-color', $colors[$index]);
                }
            }
            // The small favicon uses flat named faces rather than gradients.
            if ($name === 'favicon.svg' && ! $default) {
                foreach ($xpath->query('//svg:path[@data-part="'.$part.'"]') ?: [] as $node) {
                    if ($node instanceof DOMElement) {
                        self::set($node, 'fill', $colors[min(1, count($colors) - 1)]);
                    }
                }
            }
        }
        foreach ($xpath->query('//svg:g/svg:path[not(@data-part)][1]') ?: [] as $node) {
            if ($node instanceof DOMElement) {
                self::set($node, 'fill', $s);
            }
        }
        foreach ($xpath->query('//svg:path[@data-part="wordmark"]') ?: [] as $node) {
            if ($node instanceof DOMElement) {
                self::set($node, 'fill', ThemeDefinition::fixed($mode)['--fb-text']);
            }
        }
        if ($name === 'favicon.svg') {
            foreach ($xpath->query('/svg:svg/svg:rect') ?: [] as $node) {
                if ($node instanceof DOMElement) {
                    self::set($node, 'fill', $palette->tokens('dark')['--fb-browser-chrome']);
                }
            }
        }

        return $document->saveXML($document->documentElement)."\n";
    }

    private static function set(DOMElement $node, string $attribute, string $color): void
    {
        Color::channels($color);
        $node->setAttribute($attribute, $color);
    }
}
