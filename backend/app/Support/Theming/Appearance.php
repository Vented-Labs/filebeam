<?php

declare(strict_types=1);

namespace App\Support\Theming;

use App\Enums\AppearanceMode;
use App\Enums\Feature;
use App\Enums\ThemePreset;
use App\Models\User;
use App\Support\FeatureAvailability;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Fluent;

final class Appearance
{
    public const COOKIE = 'filebeam_appearance';

    public function __construct(private readonly Theme $theme, private readonly Assets $assets, private readonly FeatureAvailability $features) {}

    /** @return array{mode: string, preset: string} */
    public function normalize(mixed $value): array
    {
        $value = is_array($value) ? $value : [];

        return [
            'mode' => is_string($value['mode'] ?? null) ? (AppearanceMode::tryFrom($value['mode']) ?? AppearanceMode::System)->value : 'system',
            'preset' => is_string($value['preset'] ?? null) ? (ThemePreset::tryFrom($value['preset']) ?? ThemePreset::Instance)->value : 'instance',
        ];
    }

    /** @return array{mode: string, preset: string} */
    public function guest(Request $request): array
    {
        $cookie = $request->cookie(self::COOKIE);

        return $this->normalize(is_string($cookie) && strlen($cookie) <= 512 ? json_decode($cookie, true) : null);
    }

    public function actor(Request $request): ?User
    {
        $user = $request->user('web') ?? $request->user('admin');

        return $user instanceof User && $user->suspended_at === null ? $user : null;
    }

    /** @return array{account: int|null, preference: array{mode: string, preset: string}, needs_adoption: bool} */
    public function preferences(Request $request, bool $database = true): array
    {
        $user = $database ? $this->actor($request) : null;
        $stored = $user?->settings?->get('appearance');
        $hasAppearance = is_array($stored) && $stored !== [];

        return [
            'account' => $user?->id,
            'preference' => $hasAppearance ? $this->normalize($stored) : $this->guest($request),
            'needs_adoption' => $user !== null && ! $hasAppearance,
        ];
    }

    /** @param array{mode: string, preset: string} $preference
     * @return array{mode: string, preset: string}
     */
    public function save(User $user, array $preference, bool $adopt = false): array
    {
        return DB::transaction(function () use ($user, $preference, $adopt): array {
            $current = User::query()->whereKey($user->id)->lockForUpdate()->firstOrFail();
            abort_if($current->suspended_at !== null, 403);
            $settings = $current->settings ?? new Fluent;
            $stored = $settings->get('appearance');
            if ($adopt && is_array($stored) && $stored !== []) {
                return $this->normalize($stored);
            }
            $normalized = $this->normalize($preference);
            $settings->set('appearance', $normalized);
            $current->settings = $settings;
            $current->save();

            return $normalized;
        }, 3);
    }

    public function palette(Request $request, bool $database = true): Palette
    {
        $instance = $this->theme->palette($database);
        $preset = ThemePreset::from($this->preset($request, $database));

        return $preset === ThemePreset::Instance ? $instance : new Palette($preset->primary($instance->primary));
    }

    public function preset(Request $request, bool $database = true): string
    {
        return $this->customColorsAvailable() ? $this->preferences($request, $database)['preference']['preset'] : 'instance';
    }

    public function customColorsAvailable(): bool
    {
        return $this->features->available(Feature::CustomThemes) && filled(config('app.key'));
    }

    /** @return array<string, mixed> */
    public function payload(Request $request, bool $database = true): array
    {
        $instance = $this->theme->palette($database);
        $catalog = [];
        $available = $this->customColorsAvailable();
        foreach (ThemePreset::cases() as $preset) {
            $palette = $preset === ThemePreset::Instance ? $instance : new Palette($preset->primary($instance->primary));
            $assetPalette = $available ? $palette : $instance;
            $icons = [];
            foreach (['favicon.ico', 'favicon-16.png', 'favicon-32.png', 'favicon.svg', 'apple-touch-icon.png'] as $name) {
                $icons[$name] = config('filebeam.branding.favicon_url') ?: $this->assets->url($name, $assetPalette);
            }
            $catalog[] = [
                'id' => $preset->value, 'label' => $preset->label(), 'primary' => $palette->primary,
                'on_color' => Color::contrast('#ffffff', $palette->primary) >= Color::contrast('#000000', $palette->primary) ? '#ffffff' : '#000000',
                'branding' => $this->assets->branding($assetPalette), 'favicons' => $icons,
                'chrome' => ['dark' => $palette->tokens()['--fb-browser-chrome'], 'light' => $palette->tokens('light')['--fb-browser-chrome']],
            ];
        }

        return [
            ...$this->preferences($request, $database),
            'custom_colors' => $available,
            'catalog' => $catalog,
            'save_url' => route('appearance.update', absolute: false),
            'csrf' => $request->hasSession() ? $request->session()->token() : null,
            'styles_url' => route('theme.stylesheet', ['version' => self::revision()], absolute: false),
        ];
    }

    public static function revision(): string
    {
        return substr(hash('sha256', Palette::VERSION.'|'.implode('|', array_map(static fn (string $path): string => (string) hash_file('sha256', $path), [__FILE__, ...Palette::sources(), __DIR__.'/../../Enums/ThemePreset.php']))), 0, 24);
    }

    public static function stylesheet(): string
    {
        $css = '';
        foreach (ThemePreset::cases() as $preset) {
            if ($preset !== ThemePreset::Instance) {
                $css .= (new Palette($preset->primary(Palette::DEFAULT_PRIMARY)))->css('[data-fb-preset="'.$preset->value.'"]');
            }
        }

        return $css;
    }
}
