<?php

declare(strict_types=1);

use App\Actions\Admin\ManageInstanceSettings;
use App\Enums\UserRole;
use App\Models\InstanceSetting;
use App\Models\User;
use App\Support\FeatureAvailability;
use App\Support\InstanceSettingValue;
use App\Support\Theming\Theme;
use Illuminate\Support\Facades\File;
use Illuminate\Validation\ValidationException;

beforeEach(function () {
    config()->set('theme.storage_directory', storage_path('framework/testing/themes-'.bin2hex(random_bytes(8))));
});

afterEach(function () {
    File::deleteDirectory(config('theme.storage_directory'));
});

test('color settings use environment database fallback precedence and reset', function () {
    app()->instance(FeatureAvailability::class, new FeatureAvailability(['gd']));
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    $theme = app(Theme::class);
    expect($theme->primary())->toBe('#8b35ff');
    app(ManageInstanceSettings::class)->update($admin, ['primary_color' => '#0A8']);
    expect($theme->primary())->toBe('#00aa88')->and($theme->primary(false))->toBe('#00aa88');
    config()->set('filebeam.instance_settings.environment.primary_color', '#ff6600');
    expect($theme->primary())->toBe('#ff6600');
    expect(fn () => app(ManageInstanceSettings::class)->update($admin, ['primary_color' => '#aabbcc']))->toThrow(ValidationException::class);
    config()->set('filebeam.instance_settings.environment.primary_color', null);
    app(ManageInstanceSettings::class)->update($admin, ['primary_color' => null]);
    expect($theme->primary())->toBe('#8b35ff');
});

test('missing GD disables custom themes without losing the stored setting', function () {
    InstanceSetting::query()->create(['key' => 'primary_color', 'value' => '#008877']);
    app()->instance(FeatureAvailability::class, new FeatureAvailability([]));
    expect(app(Theme::class)->primary())->toBe('#8b35ff');
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    expect(fn () => app(ManageInstanceSettings::class)->update($admin, ['primary_color' => '#abcdef']))->toThrow(ValidationException::class);
    app()->instance(FeatureAvailability::class, new FeatureAvailability(['gd']));
    expect(app(Theme::class)->primary())->toBe('#008877');
});

test('colors cannot inject CSS or markup', function (mixed $value) {
    expect(fn () => InstanceSettingValue::color($value))->toThrow(InvalidArgumentException::class);
})->with(['red', '#1234', '#12345678', '#abcdef; color:red', '</style>', 'var(--color)', ['array'], 123]);
