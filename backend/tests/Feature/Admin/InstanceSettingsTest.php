<?php

declare(strict_types=1);

use App\Actions\Admin\ManageInstanceSettings;
use App\Enums\UserRole;
use App\Filament\Pages\InstanceSettings as InstanceSettingsPage;
use App\Http\Middleware\HandleInertiaRequests;
use App\Models\AdminAudit;
use App\Models\InstanceSetting;
use App\Models\InstanceTransportPolicy;
use App\Models\Plan;
use App\Models\User;
use App\Support\InstanceSettings;
use App\Support\TransportPolicy;
use Filament\Facades\Filament;
use Illuminate\Database\Events\QueryExecuted;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Validation\ValidationException;
use Livewire\Livewire;

beforeEach(function (): void {
    config()->set('app.key', 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=');
    config()->set('installation.environment_path', base_path('composer.json'));
    config()->set('filebeam.transport_policy.environment', ['enabled_drivers' => null, 'default_driver' => null]);
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

    $shared = app(HandleInertiaRequests::class)->share(Request::create('/'));
    expect($shared['filebeam']['registration_enabled'])->toBeFalse()
        ->and($shared['filebeam']['anonymous_uploads_enabled'])->toBeFalse()
        ->and($shared['filebeam']['username_routing_enabled'])->toBeFalse();

    expect($queries)->toHaveCount(2);

    InstanceSetting::query()->whereKey('registration')->update(['value' => true]);

    $shared = app(HandleInertiaRequests::class)->share(Request::create('/'));
    expect($shared['filebeam']['registration_enabled'])->toBeTrue();
});

test('environment settings take precedence over database settings in shared props', function (): void {
    Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan')]);
    InstanceSetting::query()->create(['key' => 'registration', 'value' => false]);
    config()->set('filebeam.instance_settings.environment.registration', true);

    $shared = app(HandleInertiaRequests::class)->share(Request::create('/'));
    expect($shared['filebeam']['registration_enabled'])->toBeTrue();
});

test('shared authentication props are evaluated for the current user only', function (): void {
    Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan')]);
    $firstUser = User::factory()->create();
    $secondUser = User::factory()->create();

    $firstRequest = Request::create('/');
    $firstRequest->setUserResolver(fn (): User => $firstUser);
    $firstShared = app(HandleInertiaRequests::class)->share($firstRequest);
    $secondRequest = Request::create('/');
    $secondRequest->setUserResolver(fn (): User => $secondUser);
    $secondShared = app(HandleInertiaRequests::class)->share($secondRequest);

    expect($firstShared['auth']['user']()['id'])->toBe($firstUser->id)
        ->and($secondShared['auth']['user']()['id'])->toBe($secondUser->id);
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

test('an invalid transport policy rolls back feature flags and their audit entries', function (): void {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    config()->set('filebeam.instance_settings.environment.registration', null);
    $this->actingAs($admin, 'admin');
    Livewire::test(InstanceSettingsPage::class)
        ->fillForm(['registration' => '0', 'enabled_drivers' => ['http'], 'default_driver' => 'webrtc'])
        ->call('save')
        ->assertHasErrors();
    $this->assertDatabaseMissing('instance_settings', ['key' => 'registration']);
    $this->assertDatabaseMissing('admin_audits', ['action' => 'instance_setting.updated', 'target_id' => 'registration']);
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

test('the admin transport form saves every allowed driver mode and default', function (array $enabledDrivers, string $defaultDriver): void {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    $this->actingAs($admin, 'admin');

    Livewire::test(InstanceSettingsPage::class)
        ->set('data.enabled_drivers', $enabledDrivers)
        ->set('data.default_driver', $defaultDriver)
        ->assertSet('data.enabled_drivers', $enabledDrivers)
        ->call('save')
        ->assertHasNoFormErrors();

    expect(InstanceTransportPolicy::query()->findOrFail(1)->enabled_drivers)->toBe($enabledDrivers)
        ->and(InstanceTransportPolicy::query()->findOrFail(1)->default_driver)->toBe($defaultDriver)
        ->and(app(TransportPolicy::class)->resolve())->toBe(['enabled_drivers' => $enabledDrivers, 'default_driver' => $defaultDriver]);
})->with([
    'HTTP only' => [['http'], 'http'],
    'WebRTC only' => [['webrtc'], 'webrtc'],
    'both with HTTP default' => [['http', 'webrtc'], 'http'],
    'both with WebRTC default' => [['http', 'webrtc'], 'webrtc'],
]);

test('the admin transport form audits persisted changes', function (): void {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    $this->actingAs($admin, 'admin');

    Livewire::test(InstanceSettingsPage::class)
        ->set('data.enabled_drivers', ['http', 'webrtc'])
        ->set('data.default_driver', 'webrtc')
        ->call('save')
        ->assertHasNoFormErrors();

    expect(AdminAudit::query()->where('action', 'instance_transport_policy.updated')->latest()->value('changes'))->toMatchArray([
        'enabled_drivers' => ['from' => ['http'], 'to' => ['http', 'webrtc']],
        'default_driver' => ['from' => 'http', 'to' => 'webrtc'],
    ]);
});

test('environment locked transport form fields cannot change the persisted policy', function (): void {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    config()->set('filebeam.transport_policy.environment', ['enabled_drivers' => ['webrtc'], 'default_driver' => 'webrtc']);
    $this->actingAs($admin, 'admin');

    Livewire::test(InstanceSettingsPage::class)
        ->assertFormFieldIsDisabled('enabled_drivers')
        ->assertFormFieldIsDisabled('default_driver')
        ->set('data.enabled_drivers', ['http'])
        ->set('data.default_driver', 'http')
        ->call('save')
        ->assertHasNoFormErrors();

    expect(InstanceTransportPolicy::query()->findOrFail(1)->enabled_drivers)->toBe(['http'])
        ->and(InstanceTransportPolicy::query()->findOrFail(1)->default_driver)->toBe('http')
        ->and(app(TransportPolicy::class)->resolve())->toBe(['enabled_drivers' => ['webrtc'], 'default_driver' => 'webrtc'])
        ->and(AdminAudit::query()->where('action', 'instance_transport_policy.updated')->exists())->toBeFalse();
});
