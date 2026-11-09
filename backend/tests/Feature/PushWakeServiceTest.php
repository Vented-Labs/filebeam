<?php

declare(strict_types=1);

use App\Enums\DeviceStatus;
use App\Models\Device;
use App\Models\DevicePushToken;
use App\Models\User;
use App\Services\PushSettings;
use App\Services\PushWakeService;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Http;
use Illuminate\Support\Facades\RateLimiter;
use Illuminate\Support\Str;

beforeEach(function (): void {
    config()->set('cache.default', 'array');
    config()->set('filebeam.push.enabled', true);
});

/** @return array{private: string, public: string} */
function fcmKey(): array
{
    $key = openssl_pkey_new(['private_key_type' => OPENSSL_KEYTYPE_RSA, 'private_key_bits' => 2048]);
    assert($key !== false);
    openssl_pkey_export($key, $private);
    $details = openssl_pkey_get_details($key);
    assert(is_array($details));

    return ['private' => $private, 'public' => $details['key']];
}

/** @return array{private: string, public: string} */
function apnsKey(): array
{
    $key = openssl_pkey_new(['private_key_type' => OPENSSL_KEYTYPE_EC, 'curve_name' => 'prime256v1']);
    assert($key !== false);
    openssl_pkey_export($key, $private);
    $details = openssl_pkey_get_details($key);
    assert(is_array($details));

    return ['private' => $private, 'public' => $details['key']];
}

function activePushDevice(): Device
{
    return Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => User::factory()->create()->id, 'display_name' => 'Phone', 'platform' => 'ios', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1]);
}

/** @return array<string, mixed> */
function jwtClaims(string $jwt): array
{
    $payload = explode('.', $jwt)[1] ?? '';
    $payload .= str_repeat('=', (4 - strlen($payload) % 4) % 4);
    $claims = json_decode((string) base64_decode(strtr($payload, '-_', '+/'), true), true);
    assert(is_array($claims));

    return $claims;
}

test('FCM exchanges a signed service-account assertion and sends only an opaque wake hint', function (): void {
    $key = fcmKey();
    app(PushSettings::class)->save(['enabled' => true, 'fcm' => ['project_id' => 'project-123', 'client_email' => 'service@example.test', 'private_key' => $key['private']]]);
    $device = activePushDevice();
    app(PushWakeService::class)->register($device, 'fcm', 'fcm-token');
    Http::preventStrayRequests();
    Http::fake([
        'https://oauth2.googleapis.com/token' => Http::response(['access_token' => 'short-lived-access-token', 'expires_in' => 3600]),
        'https://fcm.googleapis.com/v1/projects/project-123/messages:send' => Http::response(['name' => 'projects/project-123/messages/1']),
    ]);

    expect(app(PushWakeService::class)->wake($device, (string) Str::ulid()))->toBeTrue();
    Http::assertSent(function ($request): bool {
        if ($request->url() !== 'https://oauth2.googleapis.com/token') {
            return false;
        }
        $claims = jwtClaims((string) $request['assertion']);

        return $claims['scope'] === 'https://www.googleapis.com/auth/firebase.messaging'
            && $claims['aud'] === 'https://oauth2.googleapis.com/token'
            && $claims['exp'] - $claims['iat'] === 3600;
    });
    Http::assertSent(function ($request): bool {
        $message = $request->data()['message'] ?? [];

        return $request->url() === 'https://fcm.googleapis.com/v1/projects/project-123/messages:send'
            && $request->hasHeader('Authorization', 'Bearer short-lived-access-token')
            && $message['data']['connection_id'] !== null
            && array_keys($message['data']) === ['connection_id', 'registry_revision']
            && ! isset($message['notification']);
    });
    expect(DB::table('instance_settings')->where('key', 'push')->value('value'))->not->toContain('PRIVATE KEY');
});

test('FCM access tokens are cached until their provider expiry and invalid credentials make no request', function (): void {
    $key = fcmKey();
    app(PushSettings::class)->save(['enabled' => true, 'fcm' => ['project_id' => 'project-123', 'client_email' => 'service@example.test', 'private_key' => $key['private']]]);
    $device = activePushDevice();
    app(PushWakeService::class)->register($device, 'fcm', 'fcm-token');
    Http::fake([
        'https://oauth2.googleapis.com/token' => Http::response(['access_token' => 'cached-token', 'expires_in' => 120]),
        'https://fcm.googleapis.com/*' => Http::response([]),
    ]);

    $service = app(PushWakeService::class);
    expect($service->wake($device, (string) Str::ulid()))->toBeTrue();
    RateLimiter::clear('filebeam:push:wake:'.$device->id);
    expect($service->wake($device, (string) Str::ulid()))->toBeTrue();
    Http::assertSentCount(3);

    Cache::flush();
    Http::fake();
    Http::preventStrayRequests();
    app(PushSettings::class)->save(['enabled' => true, 'fcm' => ['project_id' => 'project-123', 'client_email' => 'service@example.test', 'private_key' => 'not a private key']]);
    expect($service->wake($device, (string) Str::ulid()))->toBeFalse();
    Http::assertNothingSent();
});

test('APNs uses a cached ES256 provider token and background-only opaque payload', function (): void {
    $key = apnsKey();
    app(PushSettings::class)->save(['enabled' => true, 'apns' => ['team_id' => 'TEAM123456', 'key_id' => 'KEY1234567', 'bundle_id' => 'io.filebeam.app', 'private_key' => $key['private'], 'sandbox' => true]]);
    $device = activePushDevice();
    app(PushWakeService::class)->register($device, 'apns', 'apns-token');
    Http::fake(['https://api.sandbox.push.apple.com/3/device/apns-token' => Http::response('', 200)]);
    $service = app(PushWakeService::class);

    expect($service->wake($device, (string) Str::ulid()))->toBeTrue();
    RateLimiter::clear('filebeam:push:wake:'.$device->id);
    expect($service->wake($device, (string) Str::ulid()))->toBeTrue();
    Http::assertSentCount(2);
    $authorizations = [];
    Http::assertSent(function ($request) use (&$authorizations): bool {
        $authorization = (string) $request->header('Authorization')[0];
        $authorizations[] = $authorization;
        $jwt = substr($authorization, 7);
        $header = explode('.', $jwt)[0] ?? '';
        $header .= str_repeat('=', (4 - strlen($header) % 4) % 4);
        $decoded = json_decode((string) base64_decode(strtr($header, '-_', '+/'), true), true);
        $payload = $request->data();

        return $request->url() === 'https://api.sandbox.push.apple.com/3/device/apns-token'
            && $request->hasHeader('apns-push-type', 'background')
            && $request->hasHeader('apns-priority', '5')
            && $decoded === ['alg' => 'ES256', 'kid' => 'KEY1234567']
            && $payload['aps'] === ['content-available' => 1]
            && array_keys($payload) === ['aps', 'connection_id', 'registry_revision'];
    });
    expect($authorizations[0])->toBe($authorizations[1]);
});

test('disabled and revoked registrations do not send a wake', function (): void {
    $device = activePushDevice();
    app(PushWakeService::class)->register($device, 'fcm', 'fcm-token');
    Http::preventStrayRequests();
    expect(app(PushWakeService::class)->wake($device, (string) Str::ulid()))->toBeFalse();
    Http::assertNothingSent();

    app(PushWakeService::class)->revoke($device);
    expect(DevicePushToken::query()->firstOrFail()->revoked_at)->not->toBeNull();
    expect(app(PushWakeService::class)->wake($device, (string) Str::ulid()))->toBeFalse();
    Http::assertNothingSent();
});
