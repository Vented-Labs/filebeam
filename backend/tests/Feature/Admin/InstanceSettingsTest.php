<?php

declare(strict_types=1);

use App\Actions\Admin\ManageInstanceSettings;
use App\Enums\UserRole;
use App\Filament\Pages\InstanceSettings as InstanceSettingsPage;
use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\User;
use App\Support\InstanceSettings;
use Filament\Facades\Filament;
use Illuminate\Database\Events\QueryExecuted;
use Illuminate\Support\Facades\DB;
use Illuminate\Validation\ValidationException;
use Inertia\Testing\AssertableInertia as Assert;
use Livewire\Livewire;

beforeEach(function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
});

test('instance settings resolve environment then database then PHP fallback without caching', function (): void {
    config()->set('filebeam.features.registration', false);
    config()->set('filebeam.instance_settings.environment.registration', null);

    expect(app(InstanceSettings::class)->boolean('registration'))->toBeFalse();

    InstanceSetting::query()->create(['key' => 'registration', 'value' => true]);
    expect(app(InstanceSettings::class)->boolean('registration'))->toBeTrue();

    InstanceSetting::query()->whereKey('registration')->update(['value' => false]);
    expect(app(InstanceSettings::class)->boolean('registration'))->toBeFalse();

    config()->set('filebeam.instance_settings.environment.registration', true);
    expect(app(InstanceSettings::class)->boolean('registration'))->toBeTrue();
});

test('shared props resolve all database settings with one query and reflect edits on the next request', function (): void {
    Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan')]);
    config()->set('filebeam.instance_settings.environment', [
        'registration' => null,
        'anonymous_uploads' => null,
        'username_routing' => null,
    ]);
    InstanceSetting::query()->create(['key' => 'registration', 'value' => false]);
    InstanceSetting::query()->create(['key' => 'anonymous_uploads', 'value' => false]);
    InstanceSetting::query()->create(['key' => 'username_routing', 'value' => false]);
    $queries = [];
    DB::listen(function (QueryExecuted $query) use (&$queries): void {
        if (str_contains($query->sql, 'instance_settings') || str_contains($query->sql, 'plans')) {
            $queries[] = $query->sql;
        }
    });

    $this->get('/')->assertInertia(fn (Assert $page) => $page
        ->where('filebeam.registration_enabled', false)
        ->where('filebeam.anonymous_uploads_enabled', false)
        ->where('filebeam.username_routing_enabled', false));

    expect($queries)->toHaveCount(2);

    InstanceSetting::query()->whereKey('registration')->update(['value' => true]);

    $this->get('/')->assertInertia(fn (Assert $page) => $page->where('filebeam.registration_enabled', true));
});

test('environment settings take precedence over database settings in shared props', function (): void {
    Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan')]);
    InstanceSetting::query()->create(['key' => 'registration', 'value' => false]);
    config()->set('filebeam.instance_settings.environment.registration', true);

    $this->get('/')->assertInertia(fn (Assert $page) => $page->where('filebeam.registration_enabled', true));
});

test('shared authentication props are evaluated for the current user only', function (): void {
    Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan')]);
    $firstUser = User::factory()->create();
    $secondUser = User::factory()->create();

    $this->actingAs($firstUser)->get('/')->assertInertia(fn (Assert $page) => $page->where('auth.user.id', $firstUser->id));
    $this->actingAs($secondUser)->get('/')->assertInertia(fn (Assert $page) => $page->where('auth.user.id', $secondUser->id));
});

test('admins can save an override and inherit the PHP fallback again', function () {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    config()->set('filebeam.instance_settings.environment.registration', null);

    app(ManageInstanceSettings::class)->update($admin, ['registration' => false]);

    $this->assertDatabaseHas('instance_settings', ['key' => 'registration', 'value' => false]);
    $this->assertDatabaseHas('admin_audits', [
        'actor_id' => $admin->id,
        'action' => 'instance_setting.updated',
        'target_type' => InstanceSetting::class,
        'target_id' => 'registration',
    ]);

    app(ManageInstanceSettings::class)->update($admin, ['registration' => null]);

    $this->assertDatabaseMissing('instance_settings', ['key' => 'registration']);
    $this->assertDatabaseHas('admin_audits', [
        'actor_id' => $admin->id,
        'action' => 'instance_setting.updated',
        'target_id' => 'registration',
    ]);
});

test('environment controlled settings cannot be overridden by backend mutations', function () {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    config()->set('filebeam.instance_settings.environment.registration', false);

    expect(fn () => app(ManageInstanceSettings::class)->update($admin, ['registration' => true]))
        ->toThrow(ValidationException::class);

    $this->assertDatabaseMissing('instance_settings', ['key' => 'registration']);
});

test('only admins can access and update the instance settings page', function () {
    $user = User::factory()->create();
    $admin = User::factory()->create(['role' => UserRole::Admin]);

    $this->actingAs($user, 'admin');
    expect(InstanceSettingsPage::canAccess())->toBeFalse();
    Livewire::test(InstanceSettingsPage::class)->assertForbidden();

    $this->actingAs($admin, 'admin');
    Livewire::test(InstanceSettingsPage::class)
        ->fillForm(['registration' => '0'])
        ->call('save')
        ->assertHasNoFormErrors();

    $this->assertDatabaseHas('instance_settings', ['key' => 'registration', 'value' => false]);
});
