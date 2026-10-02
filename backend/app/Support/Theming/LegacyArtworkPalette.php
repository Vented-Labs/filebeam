<?php

declare(strict_types=1);

namespace App\Support\Theming;

/** Compatibility recoloring for shipped raster pixels; never used for UI roles. */
final class LegacyArtworkPalette
{
    /** @var array{float, float, float} */
    private readonly array $seed;

    /** @var array{float, float, float} */
    private readonly array $anchor;

    public function __construct(private readonly Palette $palette)
    {
        $this->seed = Color::oklch($palette->primary);
        $this->anchor = Color::oklch(Palette::DEFAULT_PRIMARY);
    }

    public function color(string $hex): string
    {
        if ($this->palette->primary === Palette::DEFAULT_PRIMARY) {
            return $hex;
        }
        [$l, $c, $h] = Color::oklch($hex);
        if ($c > 0.002 && $h >= 270 && $h <= 345) {
            $h = fmod($h + $this->seed[2] - $this->anchor[2] + 360, 360);
            $c *= min(1.4, $this->seed[1] / $this->anchor[1]);
        }

        // A one-color seed substitution creates a visible seam in antialiased gradients.
        // Apply the same continuous transform to that pixel and its neighbors.
        return Color::hex($l, $c, $h);
    }
}
