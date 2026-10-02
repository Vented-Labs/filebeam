<?php

declare(strict_types=1);

namespace App\Support\Theming;

use App\Enums\Feature;
use App\Support\FeatureAvailability;
use GdImage;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\URL;
use RuntimeException;

final class Assets
{
    /** @var array<string, string> */
    private const FILES = [
        'logo.svg' => 'brand/filebeam-logo-header.svg',
        'mark.svg' => 'brand/filebeam-mark.svg',
        'email.png' => 'brand/filebeam-mark-email.png',
        'favicon.svg' => 'favicon.svg',
        'favicon.ico' => 'favicon.ico',
        'favicon-16.png' => 'favicon-16x16.png',
        'favicon-32.png' => 'favicon-32x32.png',
        'apple-touch-icon.png' => 'apple-touch-icon.png',
    ];

    public function __construct(private readonly Theme $theme, private readonly FeatureAvailability $features) {}

    public function url(string $name, ?Palette $palette = null, string $mode = 'dark'): string
    {
        $palette ??= $this->theme->palette();
        $source = $this->source($name);
        if ($palette->primary === Palette::DEFAULT_PRIMARY) {
            if ($name === 'logo.svg' && $mode === 'light') {
                return asset('brand/filebeam-logo-header-on-light.svg');
            }

            return asset($source);
        }

        return url(URL::signedRoute('theme.asset', [
            'version' => $this->revision($name),
            'primary' => substr($palette->primary, 1),
            'mode' => $mode,
            'asset' => $name,
        ], absolute: false));
    }

    /** @return array{primary_color: string, default_logo_url: string, default_light_logo_url: string, default_mark_url: string} */
    public function branding(?Palette $palette = null): array
    {
        $palette ??= $this->theme->palette();

        return [
            'primary_color' => $palette->primary,
            'default_logo_url' => $this->url('logo.svg', $palette),
            'default_light_logo_url' => $this->url('logo.svg', $palette, 'light'),
            'default_mark_url' => $this->url('mark.svg', $palette),
        ];
    }

    public function source(string $name): string
    {
        if (isset(self::FILES[$name])) {
            return self::FILES[$name];
        }
        abort_unless(in_array($name, ['social-home.png', 'social-receive.png', 'social-transfer.png'], true), 404);
        $manifestPath = config('social.manifest_path');
        $manifest = is_string($manifestPath) && is_file($manifestPath) ? json_decode((string) file_get_contents($manifestPath), true) : null;
        $variant = substr($name, 7, -4);
        $filename = is_array($manifest) ? ($manifest[$variant] ?? null) : null;
        abort_unless(is_string($filename) && basename($filename) === $filename && str_ends_with($filename, '.png'), 404);

        return 'build/og/'.$filename;
    }

    public function revision(string $name): string
    {
        $source = public_path($this->source($name));

        $sources = [$source, ...Palette::sources(), __DIR__.'/BrandArtwork.php', __DIR__.'/LegacyArtworkPalette.php'];
        if ($name === 'favicon.ico') {
            $sources = [...$sources, public_path('favicon-16x16.png'), public_path('favicon-32x32.png'), public_path('apple-touch-icon.png')];
        }
        $hashes = array_map(static fn (string $path): string => is_file($path) ? (string) hash_file('sha256', $path) : 'missing', $sources);

        return substr(hash('sha256', implode('|', [Palette::VERSION, hash_file('sha256', __FILE__), ...$hashes])), 0, 24);
    }

    public function render(string $name, Palette $palette, string $mode, ?string $version = null): string
    {
        $source = public_path($this->source($name));
        $version ??= $this->revision($name);
        $key = hash('sha256', implode('|', [$version, $palette->primary, $mode, $name]));
        $directory = $this->theme->directory().'/assets';
        File::ensureDirectoryExists($directory, 0700);
        $path = $directory.'/'.$key;
        if (is_file($path)) {
            return (string) file_get_contents($path);
        }
        abort_unless(is_file($source) && hash_equals($this->revision($name), $version), 404);
        if ($palette->primary === Palette::DEFAULT_PRIMARY && ($mode === 'dark' || pathinfo($name, PATHINFO_EXTENSION) !== 'svg')) {
            return (string) file_get_contents($source);
        }
        abort_unless($this->features->available(Feature::CustomThemes), 503, 'Custom artwork requires PHP GD.');
        $lock = fopen($path.'.lock', 'c');
        if ($lock === false) {
            throw new RuntimeException('Unable to open the theme asset lock.');
        }
        try {
            if (! flock($lock, LOCK_EX)) {
                throw new RuntimeException('Unable to lock the theme asset.');
            }
            if (! is_file($path)) {
                $bytes = match (pathinfo($name, PATHINFO_EXTENSION)) {
                    'svg' => BrandArtwork::render($name, $palette, $mode),
                    'ico' => $this->icon($palette, $mode),
                    default => $this->png($source, $palette, $mode),
                };
                File::replace($path, $bytes, 0600);
            }

            return (string) file_get_contents($path);
        } finally {
            flock($lock, LOCK_UN);
            fclose($lock);
        }
    }

    private function png(string $source, Palette $palette, string $mode): string
    {
        $image = imagecreatefrompng($source);
        if (! $image instanceof GdImage) {
            throw new RuntimeException('Unable to read the theme artwork.');
        }
        imagepalettetotruecolor($image);
        imagealphablending($image, false);
        imagesavealpha($image, true);
        $colors = [];
        $artwork = new LegacyArtworkPalette($palette);
        $width = imagesx($image);
        $height = imagesy($image);
        // The shipped raster carries the original typography, gradients and antialiasing.
        // Transform each distinct RGB value once and retain its original alpha coverage.
        for ($y = 0; $y < $height; $y++) {
            for ($x = 0; $x < $width; $x++) {
                $pixel = imagecolorat($image, $x, $y);
                $rgb = $pixel & 0xFFFFFF;
                $colors[$rgb] ??= (int) hexdec(substr($artwork->color(sprintf('#%06x', $rgb)), 1));
                imagesetpixel($image, $x, $y, ($pixel & 0x7F000000) | $colors[$rgb]);
            }
        }

        return $this->encode($image);
    }

    private function encode(GdImage $image): string
    {
        ob_start();
        try {
            if (! imagepng($image)) {
                throw new RuntimeException('Unable to encode the theme artwork.');
            }

            return (string) ob_get_contents();
        } finally {
            ob_end_clean();
        }
    }

    private function icon(Palette $palette, string $mode): string
    {
        $frames = [];
        foreach ([16, 32, 48, 64] as $size) {
            $source = public_path($size <= 32 ? 'favicon-'.$size.'x'.$size.'.png' : 'apple-touch-icon.png');
            $image = imagecreatefromstring($this->png($source, $palette, $mode));
            if (! $image instanceof GdImage) {
                throw new RuntimeException('Unable to create the favicon frame.');
            }
            $scaled = imagescale($image, $size, $size, IMG_BICUBIC_FIXED);
            if (! $scaled instanceof GdImage) {
                throw new RuntimeException('Unable to resize the favicon frame.');
            }
            imagesavealpha($scaled, true);
            $frames[$size] = $this->encode($scaled);
        }
        $directory = pack('vvv', 0, 1, count($frames));
        $offset = 6 + 16 * count($frames);
        foreach ($frames as $size => $bytes) {
            $directory .= pack('CCCCvvVV', $size, $size, 0, 0, 1, 32, strlen($bytes), $offset);
            $offset += strlen($bytes);
        }

        return $directory.implode('', $frames);
    }
}
