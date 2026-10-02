<?php

declare(strict_types=1);

namespace App\Support\Theming;

use App\Enums\Feature;
use App\Support\FeatureAvailability;
use App\Support\InstanceSettings;
use App\Support\InstanceSettingValue;
use Illuminate\Database\QueryException;
use Illuminate\Support\Facades\File;
use InvalidArgumentException;

final class Theme
{
    public function __construct(private readonly FeatureAvailability $features, private readonly InstanceSettings $settings) {}

    public function primary(bool $database = true): string
    {
        if (! $this->features->available(Feature::CustomThemes)) {
            return Palette::DEFAULT_PRIMARY;
        }
        $value = $this->settings->environmentValue('primary_color');
        if ($value === null && $database) {
            try {
                $value = $this->settings->value('primary_color');
            } catch (QueryException) {
                // Error and setup pages must render even when settings storage is unavailable.
            }
        }
        if ($value === null) {
            $path = $this->directory().'/current.json';
            $saved = is_file($path) ? json_decode((string) file_get_contents($path), true) : null;
            $value = is_array($saved) ? ($saved['primary'] ?? null) : null;
        }
        try {
            return InstanceSettingValue::color($value ?? Palette::DEFAULT_PRIMARY);
        } catch (InvalidArgumentException) {
            return Palette::DEFAULT_PRIMARY;
        }
    }

    public function palette(bool $database = true): Palette
    {
        $resolved = app()->bound('request') ? request()->attributes->get(Palette::class) : null;
        if ($resolved instanceof Palette) {
            return $resolved;
        }

        return new Palette($this->primary($database));
    }

    public function directory(): string
    {
        return (string) config('theme.storage_directory', storage_path('app/themes'));
    }

    public function remember(): void
    {
        if (app()->bound('request')) {
            request()->attributes->remove(Palette::class);
        }
        File::ensureDirectoryExists($this->directory(), 0700);
        File::replace($this->directory().'/current.json', json_encode(['primary' => $this->primary()], JSON_THROW_ON_ERROR), 0600);
    }
}
