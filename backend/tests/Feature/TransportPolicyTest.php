<?php

declare(strict_types=1);

use App\Actions\Admin\ManageInstanceSettings;
use App\Actions\Admin\ManagePlan;
use App\Enums\TransferDriver;
use App\Enums\UserRole;
use App\Filament\Resources\Plans\Pages\EditPlan;
use App\Http\Middleware\HandleInertiaRequests;
use App\Models\AdminAudit;
use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\User;
use App\Support\TransportPolicy;
use Filament\Facades\Filament;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;
use Illuminate\Validation\ValidationException;
use Livewire\Livewire;

uses(RefreshDatabase::class);

function transportPolicyAdmin(): User
{
    return User::factory()->create(['role' => UserRole::Admin]);
}

/** @param  list<string>  $enabledDrivers */
function transportPolicySettings(array $enabledDrivers, string $defaultDriver): void
{
    InstanceSetting::query()->updateOrCreate(['key' => 'enabled_drivers'], ['value' => $enabledDrivers]);
    InstanceSetting::query()->updateOrCreate(['key' => 'default_driver'], ['value' => $defaultDriver]);
}

/** @return array<string, bool|int|null> */
function transportPlanAttributes(Plan $plan): array
{
    return [
        'maximum_transfer_bytes' => $plan->maximum_transfer_bytes,
        'maximum_file_count' => $plan->maximum_file_count,
        'maximum_note_bytes' => $plan->maximum_note_bytes,
        'webrtc_maximum_transfer_bytes' => $plan->webrtc_maximum_transfer_bytes,
        'webrtc_maximum_file_count' => $plan->webrtc_maximum_file_count,
        'webrtc_maximum_note_bytes' => $plan->webrtc_maximum_note_bytes,
        'default_file_retention_hours' => $plan->default_file_retention_hours,
        'maximum_file_retention_hours' => $plan->maximum_file_retention_hours,
        'default_note_retention_hours' => $plan->default_note_retention_hours,
        'maximum_note_retention_hours' => $plan->maximum_note_retention_hours,
        'is_active' => $plan->is_active,
    ];
}

/**
 * @param  array<string, string|null>  $values
 * @return array<string, mixed>
 */
function loadFilebeamConfigWithEnvironment(array $values): array
{
    $previous = [];
    foreach ($values as $key => $value) {
        $previous[$key] = getenv($key);
        putenv($value === null ? $key : "{$key}={$value}");
        if ($value === null) {
            unset($_ENV[$key], $_SERVER[$key]);
        } else {
            $_ENV[$key] = $value;
            $_SERVER[$key] = $value;
        }
    }

    try {
        return require config_path('filebeam.php');
    } finally {
        foreach ($previous as $key => $value) {
            putenv($value === false ? $key : "{$key}={$value}");
            if ($value === false) {
                unset($_ENV[$key], $_SERVER[$key]);
            } else {
                $_ENV[$key] = $value;
                $_SERVER[$key] = $value;
            }
        }
    }
}

beforeEach(function (): void {
    config()->set('app.key', 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=');
    config()->set('filebeam.instance_settings.environment.enabled_drivers', null);
    config()->set('filebeam.instance_settings.environment.default_driver', null);
});

test('transport policy defaults to HTTP and applies environment then database then fallback precedence', function (): void {
    $resolver = app(TransportPolicy::class);

    expect($resolver->resolve())->toBe(['enabled_drivers' => ['http'], 'default_driver' => 'http']);

    transportPolicySettings(['http', 'webrtc'], 'webrtc');
    expect($resolver->resolve())->toBe(['enabled_drivers' => ['http', 'webrtc'], 'default_driver' => 'webrtc']);

    config()->set('filebeam.instance_settings.environment.enabled_drivers', ['http']);
    config()->set('filebeam.instance_settings.environment.default_driver', 'http');
    expect($resolver->resolve())->toBe(['enabled_drivers' => ['http'], 'default_driver' => 'http']);
});

test('configuration rejects boolean and malformed JSON transport environment values', function (string $key, string $value, string $exception): void {
    expect(fn () => loadFilebeamConfigWithEnvironment([$key => $value]))->toThrow($exception);
})->with([
    'boolean enabled drivers' => ['FILEBEAM_ENABLED_TRANSFER_DRIVERS', 'true', InvalidArgumentException::class],
    'malformed enabled drivers JSON' => ['FILEBEAM_ENABLED_TRANSFER_DRIVERS', '[', JsonException::class],
    'boolean ICE servers' => ['FILEBEAM_WEBRTC_ICE_SERVERS', 'false', InvalidArgumentException::class],
    'malformed ICE servers JSON' => ['FILEBEAM_WEBRTC_ICE_SERVERS', '[', JsonException::class],
]);

test('WebRTC ICE configuration uses the Vented STUN fallback only when ICE and TURN are both unset', function (array $environment, array $expectedIceServers): void {
    $configuration = loadFilebeamConfigWithEnvironment([
        'FILEBEAM_WEBRTC_ICE_SERVERS' => $environment['ice_servers'],
        'FILEBEAM_WEBRTC_TURN_URLS' => $environment['turn_urls'],
    ]);

    expect($configuration['webrtc']['ice_servers'])->toBe($expectedIceServers);
})->with([
    'unset ICE and TURN' => [
        ['ice_servers' => null, 'turn_urls' => null],
        [['urls' => ['stun:stun.vented.com:3478']]],
    ],
    'explicit empty ICE list' => [
        ['ice_servers' => '[]', 'turn_urls' => null],
        [],
    ],
    'custom STUN list' => [
        ['ice_servers' => '[{"urls":["stun:stun.example.test:3478"]}]', 'turn_urls' => null],
        [['urls' => ['stun:stun.example.test:3478']]],
    ],
    'TURN URL without ICE list' => [
        ['ice_servers' => null, 'turn_urls' => 'turn:turn.example.test:3478?transport=udp'],
        [],
    ],
]);

test('transport policy admin changes are authorized, atomic, and audited', function (): void {
    $manager = app(ManageInstanceSettings::class);
    $user = User::factory()->create();
    $admin = transportPolicyAdmin();

    expect(fn () => $manager->update($user, ['enabled_drivers' => ['webrtc'], 'default_driver' => 'webrtc']))->toThrow(AuthorizationException::class);
    expect(fn () => $manager->update($admin, ['enabled_drivers' => [], 'default_driver' => 'webrtc']))->toThrow(ValidationException::class);
    expect(app(TransportPolicy::class)->resolve()['default_driver'])->toBe('http');

    $manager->update($admin, ['enabled_drivers' => ['webrtc'], 'default_driver' => 'webrtc']);

    expect(app(TransportPolicy::class)->resolve()['enabled_drivers'])->toBe(['webrtc'])
        ->and(AdminAudit::query()->where('action', 'instance_setting.updated')->where('actor_id', $admin->id)->exists())->toBeTrue();
});

test('environment controlled transport values cannot be overridden and invalid effective combinations are rejected', function (): void {
    $admin = transportPolicyAdmin();
    config()->set('filebeam.instance_settings.environment.enabled_drivers', ['http']);

    expect(fn () => app(ManageInstanceSettings::class)->update($admin, ['enabled_drivers' => ['webrtc']]))->toThrow(ValidationException::class)
        ->and(fn () => app(ManageInstanceSettings::class)->update($admin, ['default_driver' => 'webrtc']))->toThrow(ValidationException::class);

    config()->set('filebeam.instance_settings.environment.enabled_drivers', ['http']);
    config()->set('filebeam.instance_settings.environment.default_driver', 'webrtc');
    expect(fn () => app(TransportPolicy::class)->resolve())->toThrow(InvalidArgumentException::class);
});

test('transport migration backfills WebRTC limits from existing finite HTTP limits', function (): void {
    $plan = Plan::factory()->create([
        'maximum_transfer_bytes' => 123_456,
        'maximum_file_count' => 17,
        'maximum_note_bytes' => 789,
    ]);
    $migration = require database_path('migrations/2026_09_08_200000_add_transport_policy_and_webrtc_plan_limits.php');

    $migration->down();
    DB::table('plans')->where('id', $plan->id)->update([
        'maximum_transfer_bytes' => 123_456,
        'maximum_file_count' => 17,
        'maximum_note_bytes' => 789,
    ]);
    $migration->up();

    $limits = DB::table('plans')->where('id', $plan->id)->first(['webrtc_maximum_transfer_bytes', 'webrtc_maximum_file_count', 'webrtc_maximum_note_bytes']);
    if ($limits === null) {
        throw new RuntimeException('The migrated plan was not found.');
    }

    expect($limits->webrtc_maximum_transfer_bytes)->toBe(123_456)
        ->and($limits->webrtc_maximum_file_count)->toBe(17)
        ->and($limits->webrtc_maximum_note_bytes)->toBe(789)
        ->and(DB::table('instance_transport_policies')->where('id', 1)->value('enabled_drivers'))->toBe(json_encode(['http']));
    Schema::dropIfExists('instance_transport_policies');
});

test('public shared props expose resolved policy and independent effective plan limits', function (): void {
    $default = Plan::factory()->create([
        'slug' => config('filebeam.transfers.default_plan'),
        'maximum_transfer_bytes' => 100,
        'webrtc_maximum_transfer_bytes' => null,
        'webrtc_maximum_file_count' => 3,
        'webrtc_maximum_note_bytes' => null,
    ]);
    transportPolicySettings(['http', 'webrtc'], 'webrtc');
    $shared = app(HandleInertiaRequests::class)->share(Request::create('/'));
    $configuration = $shared['filebeam']['transport_policy']();

    expect($configuration)->toBe([
        'enabled_drivers' => ['http', 'webrtc'],
        'default_driver' => 'webrtc',
        'limits' => [
            'http' => ['maximum_transfer_bytes' => 100, 'maximum_file_count' => $default->maximum_file_count, 'maximum_note_bytes' => $default->maximum_note_bytes],
            'webrtc' => ['maximum_transfer_bytes' => null, 'maximum_file_count' => 3, 'maximum_note_bytes' => null],
        ],
    ])->and(array_key_exists('ice_servers', $configuration))->toBeFalse()
        ->and(array_key_exists('turn_secret', $configuration))->toBeFalse();

    expect(app(TransportPolicy::class)->allows(TransferDriver::WebRtc))->toBeTrue()
        ->and(app(TransportPolicy::class)->limits($default, TransferDriver::WebRtc)['maximum_note_bytes'])->toBeNull();
});

test('the plan editor serializes finite WebRTC limits and Unlimited independently', function (): void {
    $admin = transportPolicyAdmin();
    $plan = Plan::factory()->create([
        'webrtc_maximum_transfer_bytes' => 2 * 1024 * 1024,
        'webrtc_maximum_file_count' => 4,
        'webrtc_maximum_note_bytes' => 1024,
    ]);
    Filament::setCurrentPanel(Filament::getPanel('admin'));
    Filament::auth()->login($admin);

    Livewire::test(EditPlan::class, ['record' => $plan->getKey()])
        ->fillForm(fn (array $data): array => [...$data,
            'webrtc_maximum_transfer_unlimited' => true,
            'webrtc_maximum_file_count_unlimited' => false,
            'webrtc_maximum_file_count' => 8,
            'webrtc_maximum_note_unlimited' => true,
        ])
        ->call('save')
        ->assertHasNoFormErrors();

    expect($plan->refresh()->webrtc_maximum_transfer_bytes)->toBeNull()
        ->and($plan->webrtc_maximum_file_count)->toBe(8)
        ->and($plan->webrtc_maximum_note_bytes)->toBeNull();

    Livewire::test(EditPlan::class, ['record' => $plan->getKey()])
        ->fillForm(fn (array $data): array => [...$data,
            'webrtc_maximum_transfer_unlimited' => false,
            'webrtc_maximum_transfer_quantity' => 3,
            'webrtc_maximum_transfer_unit' => 'MiB',
            'webrtc_maximum_file_count_unlimited' => true,
            'webrtc_maximum_note_unlimited' => false,
            'webrtc_maximum_note_quantity' => 2,
            'webrtc_maximum_note_unit' => 'KiB',
        ])
        ->call('save')
        ->assertHasNoFormErrors();

    expect($plan->refresh()->webrtc_maximum_transfer_bytes)->toBe(3 * 1024 * 1024)
        ->and($plan->webrtc_maximum_file_count)->toBeNull()
        ->and($plan->webrtc_maximum_note_bytes)->toBe(2 * 1024);
});

test('WebRTC plan limits are independently nullable and WebRTC-only plans may clear filestores', function (): void {
    $admin = transportPolicyAdmin();
    $plan = Plan::factory()->create();
    transportPolicySettings(['webrtc'], 'webrtc');
    $attributes = transportPlanAttributes($plan);
    $attributes['webrtc_maximum_transfer_bytes'] = null;
    $attributes['webrtc_maximum_file_count'] = 7;
    $attributes['webrtc_maximum_note_bytes'] = null;
    $attributes['filestore_ids'] = [];
    $attributes['default_filestore_ids'] = [];

    app(ManagePlan::class)->update($admin, $plan, $attributes);

    expect($plan->refresh()->webrtc_maximum_transfer_bytes)->toBeNull()
        ->and($plan->webrtc_maximum_file_count)->toBe(7)
        ->and($plan->webrtc_maximum_note_bytes)->toBeNull()
        ->and($plan->filestores()->exists())->toBeFalse();

    $attributes['webrtc_maximum_file_count'] = 0;
    expect(fn () => app(ManagePlan::class)->update($admin, $plan, $attributes))->toThrow(ValidationException::class);
});
