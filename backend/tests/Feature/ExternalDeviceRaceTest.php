<?php

declare(strict_types=1);

use App\Enums\DeviceStatus;
use App\Models\Device;
use App\Models\User;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Str;
use Tests\Support\ExternalDeviceRace;

beforeEach(function (): void {
    $external = ExternalDeviceRace::connection($this->app);
    if ($external === null) {
        $this->markTestSkipped('Set FILEBEAM_EXTERNAL_DB_CONNECTION and FILEBEAM_EXTERNAL_DB_URL to run external device race tests.');
    }
    $this->externalDeviceRace = $external;
    $this->externalDeviceRaceDefaultConnection = config('database.default');
    ExternalDeviceRace::migrate($this->app, $external['connection'], $external['config']);
    config(['database.default' => $external['connection']]);
});

afterEach(function (): void {
    if (isset($this->externalDeviceRace)) {
        config(['database.default' => $this->externalDeviceRaceDefaultConnection]);
        DB::purge($this->externalDeviceRace['connection']);
    }
});

/** @return array{id: string, secret: string, public: string} */
function externalDeviceRaceIdentity(): array
{
    $pair = sodium_crypto_sign_keypair();

    return ['id' => (string) Str::ulid(), 'secret' => sodium_crypto_sign_secretkey($pair), 'public' => base64_encode(sodium_crypto_sign_publickey($pair))];
}

/** @param array{id: string, secret: string, public: string} $identity */
function externalDeviceRaceProof(array $identity, string $challenge): string
{
    $raw = base64_decode($challenge, true);
    assert(is_string($raw));

    return base64_encode(sodium_crypto_sign_detached("filebeam-device-enrollment-v1\0{$identity['id']}\0".'1'."\0".$raw, $identity['secret']));
}

/** @param list<array{mode: string, payload: array<string, mixed>}> $jobs @return list<array{status: int, error: string|null}> */
function runExternalDeviceRace(array $jobs): array
{
    $directory = sys_get_temp_dir().'/filebeam-external-device-races-'.bin2hex(random_bytes(8));
    mkdir($directory, 0700, true);
    try {
        return ExternalDeviceRace::release(array_map(fn (array $job): array => ExternalDeviceRace::worker($job['mode'], $job['payload'], $directory), $jobs), $directory);
    } finally {
        app('files')->deleteDirectory($directory);
    }
}

test('external database first enrollment race issues exactly one bootstrap credential', function (): void {
    $user = User::factory()->create();
    $identities = [externalDeviceRaceIdentity(), externalDeviceRaceIdentity()];
    $payloads = [];
    foreach ($identities as $identity) {
        $challenge = random_bytes(32);
        Device::query()->create(['id' => $identity['id'], 'user_id' => $user->id, 'display_name' => 'Race device', 'platform' => 'linux', 'public_key' => base64_decode($identity['public'], true), 'key_version' => 1, 'status' => DeviceStatus::Pending, 'generation' => 1, 'enrollment_challenge_hash' => hash('sha256', $challenge), 'enrollment_expires_at' => now()->addMinutes(10)]);
        $payloads[] = ['user_id' => $user->id, 'device_id' => $identity['id'], 'challenge' => base64_encode($challenge), 'signature' => externalDeviceRaceProof($identity, base64_encode($challenge))];
    }

    $results = runExternalDeviceRace(array_map(fn (array $payload): array => ['mode' => 'first-enrollment', 'payload' => $payload], $payloads));

    expect(collect($results)->pluck('status')->sort()->values()->all())->toBe([201, 409])
        ->and(Device::query()->where('status', DeviceStatus::Active)->count())->toBe(1)
        ->and(DB::table('device_credentials')->count())->toBe(1);
});

test('external database revoke wins over concurrent credential renewal', function (): void {
    $user = User::factory()->create();
    $device = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $user->id, 'display_name' => 'Race device', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1]);
    $credential = $device->credentials()->create(['token_hash' => hash('sha256', Str::random(64)), 'expires_at' => now()->addDay()]);

    $results = runExternalDeviceRace([
        ['mode' => 'renew', 'payload' => ['credential_id' => $credential->id, 'idempotency_key' => Str::random(32)]],
        ['mode' => 'revoke', 'payload' => ['user_id' => $user->id, 'device_id' => $device->id]],
    ]);

    $statuses = collect($results)->pluck('status')->sort()->values()->all();
    expect($statuses[0])->toBe(200)
        ->and($statuses[1])->toBeIn([201, 403])
        ->and($device->fresh()->status)->toBe(DeviceStatus::Revoked)
        ->and(DB::table('device_credentials')->whereNull('revoked_at')->count())->toBe(0);
});
