<?php

declare(strict_types=1);

use App\Actions\Admin\ManageInstanceSettings;
use App\Enums\UserRole;
use App\Filament\Pages\InstanceSettings as InstanceSettingsPage;
use App\Models\InstanceSetting;
use App\Models\User;
use App\Support\FeatureAvailability;
use App\Support\InstanceSettingValue;
use App\Support\Theming\Theme;
use Filament\Facades\Filament;
use Illuminate\Support\Facades\File;
use Illuminate\Validation\ValidationException;
use Livewire\Livewire;

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

test('a failed SMTP save cannot publish an uncommitted theme fallback', function () {
    app()->instance(FeatureAvailability::class, new FeatureAvailability(['gd']));
    config()->set('smtp.managed', false);
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    app(ManageInstanceSettings::class)->update($admin, ['primary_color' => '#008877']);
    $this->actingAs($admin, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));
    Livewire::test(InstanceSettingsPage::class)
        ->fillForm(['primary_color' => '#cc5500', 'smtp' => [
            'enabled' => true, 'host' => '', 'port' => 587, 'security' => 'starttls',
            'username' => '', 'password' => '', 'clear_password' => false,
            'from_address' => 'sender@example.test', 'from_name' => 'Filebeam',
        ]])
        ->call('save')
        ->assertHasFormErrors(['smtp.host']);
    expect(app(Theme::class)->primary())->toBe('#008877')
        ->and(app(Theme::class)->primary(false))->toBe('#008877');
});
