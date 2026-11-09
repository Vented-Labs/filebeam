<?php

declare(strict_types=1);

use App\Enums\DeviceStatus;
use App\Http\Middleware\RequireInstallation;
use App\Models\Device;
use App\Models\DeviceCredentialRotation;
use App\Models\DevicePushToken;
use App\Models\InstanceSetting;
use App\Models\User;
use App\Services\PushSettings;
use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Http;
use Illuminate\Support\Str;

beforeEach(function (): void {
    $this->withoutMiddleware(RequireInstallation::class);
    config()->set('cache.default', 'array');
    config()->set('cache.stores.array.serialize', true);
    InstanceSetting::query()->create(['key' => 'device_rendezvous', 'value' => true]);
});

/** @return array{id: string, secret: string, public: string} */
function deviceIdentity(): array
{
    $pair = sodium_crypto_sign_keypair();

    return ['id' => (string) Str::ulid(), 'secret' => sodium_crypto_sign_secretkey($pair), 'public' => base64_encode(sodium_crypto_sign_publickey($pair))];
}

/** @param array{id: string, secret: string, public: string} $identity @return array{challenge: string, signature: string} */
function enrollmentProof(array $identity, string $challenge, int $generation = 1): array
{
    $raw = base64_decode($challenge, true);
    assert(is_string($raw));

    return ['challenge' => $challenge, 'signature' => base64_encode(sodium_crypto_sign_detached("filebeam-device-enrollment-v1\0{$identity['id']}\0{$generation}\0".$raw, $identity['secret']))];
}

/** @param array{id: string, secret: string, public: string} $candidate @param array{id: string, secret: string, public: string} $approver */
function approvalProof(User $user, array $candidate, array $approver): string
{
    $device = Device::query()->findOrFail($candidate['id']);
    $digest = hex2bin((string) $device->enrollment_challenge_hash);
    assert(is_string($digest) && $device->enrollment_expires_at !== null);
    $message = implode("\0", [
        'filebeam-device-approval-v2', rtrim((string) config('app.url'), '/'), (string) config('filebeam.devices.deployment_id'), (string) $user->id,
        (string) $device->generation, $device->id, (string) $device->key_version, $approver['id'], '1', (string) $device->enrollment_expires_at->getTimestamp(),
    ])."\0".$digest.base64_decode($candidate['public'], true);

    return base64_encode(sodium_crypto_sign_detached($message, $approver['secret']));
}

/** @param array{id: string, secret: string, public: string} $identity @return array{challenge: string, credential?: string} */
function enrollDevice(User $user, array $identity): array
{
    $challenge = test()->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', [
        'device_id' => $identity['id'], 'display_name' => 'Test device', 'platform' => 'linux', 'public_key' => $identity['public'], 'key_version' => 1,
    ], ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data.challenge');
    assert(is_string($challenge));
    $response = test()->actingAs($user)->postJson("/api/native/v1/devices/enrollments/{$identity['id']}", enrollmentProof($identity, $challenge), ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data');

    return ['challenge' => $challenge, 'credential' => $response['credential']];
}

function devicePushPrivateKey(): string
{
    $key = openssl_pkey_new(['private_key_type' => OPENSSL_KEYTYPE_RSA, 'private_key_bits' => 2048]);
    assert($key !== false);
    openssl_pkey_export($key, $private);

    return $private;
}

test('first device enrollment requires an Ed25519 possession proof and issues one device credential', function (): void {
    $user = User::factory()->create();
    $identity = deviceIdentity();
    $challengeResponse = $this->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', [
        'device_id' => $identity['id'], 'display_name' => 'Test device', 'platform' => 'linux', 'public_key' => $identity['public'], 'key_version' => 1,
    ], ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->assertJsonPath('data.approval_context.version', 2)->assertJsonPath('data.approval_context.candidate_device_id', $identity['id']);
    $challenge = $challengeResponse->json('data.challenge');
    $this->actingAs($user)->postJson("/api/native/v1/devices/enrollments/{$identity['id']}", ['challenge' => $challenge, 'signature' => base64_encode(str_repeat('x', SODIUM_CRYPTO_SIGN_BYTES))], ['Sec-Fetch-Site' => 'same-origin'])->assertUnprocessable();
    $response = $this->actingAs($user)->postJson("/api/native/v1/devices/enrollments/{$identity['id']}", enrollmentProof($identity, $challenge), ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data');

    expect($response['credential'])->toHaveLength(64);
    $this->getJson('/api/native/v1/devices/sync', ['Authorization' => 'Bearer '.$response['credential']])->assertOk()->assertJsonPath('data.devices.0.status', DeviceStatus::Active->value);
});

test('a later device remains pending until an active device approves its exact key', function (): void {
    $user = User::factory()->create();
    $first = deviceIdentity();
    $credential = enrollDevice($user, $first)['credential'];
    $candidate = deviceIdentity();
    $challenge = $this->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', [
        'device_id' => $candidate['id'], 'display_name' => 'Second device', 'platform' => 'macos', 'public_key' => $candidate['public'], 'key_version' => 1,
    ], ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data.challenge');
    $this->actingAs($user)->postJson("/api/native/v1/devices/enrollments/{$candidate['id']}", enrollmentProof($candidate, $challenge), ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->assertJsonPath('data.status', 'pending');
    $approval = approvalProof($user, $candidate, $first);
    $this->postJson("/api/native/v1/devices/enrollments/{$candidate['id']}/approve", ['approval_signature' => $approval], ['Authorization' => 'Bearer '.$credential])->assertCreated()->assertJsonMissing(['credential']);
    $redemption = $this->postJson("/api/native/v1/devices/enrollments/{$candidate['id']}/credential", enrollmentProof($candidate, $challenge))->assertCreated()
        ->assertJsonPath('data.approval_evidence.approver_device_id', $first['id'])
        ->assertJsonPath('data.approval_evidence.approver_key_version', 1)
        ->json('data');
    $redeemed = $redemption['credential'];
    $this->getJson('/api/native/v1/devices/sync', ['Authorization' => 'Bearer '.$redeemed])->assertOk();
});

test('a candidate cannot take over another accounts device ID', function (): void {
    $owner = User::factory()->create();
    $attacker = User::factory()->create();
    $device = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $owner->id, 'display_name' => 'Owners device', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 4, 'status' => DeviceStatus::Active, 'generation' => 1]);
    $before = $device->only(['user_id', 'display_name', 'public_key', 'key_version', 'status', 'generation']);
    $identity = deviceIdentity();

    $this->actingAs($attacker)->postJson('/api/native/v1/devices/enrollment-challenge', [
        'device_id' => $device->id, 'display_name' => 'Taken over', 'platform' => 'macos', 'public_key' => $identity['public'], 'key_version' => 1,
    ], ['Sec-Fetch-Site' => 'same-origin'])->assertNotFound();

    expect($device->fresh()->only(array_keys($before)))->toEqual($before);
});

test('approval signatures cannot be replayed after a challenge is renewed', function (): void {
    $user = User::factory()->create();
    $approver = deviceIdentity();
    $credential = enrollDevice($user, $approver)['credential'];
    $candidate = deviceIdentity();
    $request = ['device_id' => $candidate['id'], 'display_name' => 'Candidate', 'platform' => 'linux', 'public_key' => $candidate['public'], 'key_version' => 1];
    $oldChallenge = $this->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', $request, ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data.challenge');
    $this->actingAs($user)->postJson("/api/native/v1/devices/enrollments/{$candidate['id']}", enrollmentProof($candidate, $oldChallenge), ['Sec-Fetch-Site' => 'same-origin'])->assertCreated();
    $oldApproval = approvalProof($user, $candidate, $approver);
    $newChallenge = $this->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', $request, ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data.challenge');
    expect($newChallenge)->not->toBe($oldChallenge);

    $this->postJson("/api/native/v1/devices/enrollments/{$candidate['id']}/approve", ['approval_signature' => $oldApproval], ['Authorization' => 'Bearer '.$credential])->assertUnprocessable();
    expect(Device::query()->findOrFail($candidate['id'])->status)->toBe(DeviceStatus::Pending);
});

test('challenge renewal invalidates the old possession proof and cannot demote an active device', function (): void {
    $user = User::factory()->create();
    $identity = deviceIdentity();
    $request = ['device_id' => $identity['id'], 'display_name' => 'Candidate', 'platform' => 'linux', 'public_key' => $identity['public'], 'key_version' => 1];
    $first = $this->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', $request, ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data.challenge');
    $renewed = $this->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', $request, ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data.challenge');

    $this->actingAs($user)->postJson("/api/native/v1/devices/enrollments/{$identity['id']}", enrollmentProof($identity, $first), ['Sec-Fetch-Site' => 'same-origin'])->assertUnprocessable();
    $this->actingAs($user)->postJson("/api/native/v1/devices/enrollments/{$identity['id']}", enrollmentProof($identity, $renewed), ['Sec-Fetch-Site' => 'same-origin'])->assertCreated();
    $this->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', $request, ['Sec-Fetch-Site' => 'same-origin'])->assertConflict();
    expect(Device::query()->findOrFail($identity['id'])->status)->toBe(DeviceStatus::Active);
});

test('expired or reset-generation challenges cannot activate a candidate', function (): void {
    $user = User::factory()->create();
    $expired = deviceIdentity();
    $request = ['device_id' => $expired['id'], 'display_name' => 'Expired', 'platform' => 'linux', 'public_key' => $expired['public'], 'key_version' => 1];
    $challenge = $this->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', $request, ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data.challenge');
    Device::query()->findOrFail($expired['id'])->update(['enrollment_expires_at' => now()->subSecond()]);
    $this->actingAs($user)->postJson("/api/native/v1/devices/enrollments/{$expired['id']}", enrollmentProof($expired, $challenge), ['Sec-Fetch-Site' => 'same-origin'])->assertNotFound();

    $candidate = deviceIdentity();
    $challenge = $this->actingAs($user)->postJson('/api/native/v1/devices/enrollment-challenge', ['device_id' => $candidate['id'], 'display_name' => 'Reset candidate', 'platform' => 'linux', 'public_key' => $candidate['public'], 'key_version' => 1], ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->json('data.challenge');
    $this->actingAs($user)->deleteJson('/api/native/v1/devices', ['current_password' => 'password', 'confirmation' => 'RESET'], ['Sec-Fetch-Site' => 'same-origin'])->assertOk();
    $this->actingAs($user)->postJson("/api/native/v1/devices/enrollments/{$candidate['id']}", enrollmentProof($candidate, $challenge), ['Sec-Fetch-Site' => 'same-origin'])->assertNotFound();
});

test('web revoke invalidates credentials and existing rendezvous authority without transfer metadata', function (): void {
    $user = User::factory()->create();
    $first = deviceIdentity();
    $firstCredential = enrollDevice($user, $first)['credential'];
    $second = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $user->id, 'display_name' => 'Other', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1]);
    $secondCredential = Str::random(64);
    $second->credentials()->create(['token_hash' => hash('sha256', $secondCredential), 'expires_at' => now()->addDay()]);
    $connection = $this->postJson('/api/native/v1/devices/connections', ['target_device_id' => $second->id], ['Authorization' => 'Bearer '.$firstCredential])->assertCreated()->json('data.connection.id');
    $this->actingAs($user)->deleteJson("/api/native/v1/devices/{$second->id}", ['current_password' => 'password'], ['Sec-Fetch-Site' => 'same-origin'])->assertOk();
    $this->getJson('/api/native/v1/devices/sync', ['Authorization' => 'Bearer '.$secondCredential])->assertUnauthorized();
    $this->getJson("/api/native/v1/devices/connections/{$connection}", ['Authorization' => 'Bearer '.$firstCredential])->assertNotFound();
    expect(Cache::get('filebeam:device-pair:'.$connection))->not->toHaveKey('manifest');
});

test('a current device alone can register a bounded push token and connection wakes are advisory', function (): void {
    config()->set('filebeam.push.enabled', true);
    app(PushSettings::class)->save(['enabled' => true, 'fcm' => ['project_id' => 'project-123', 'client_email' => 'service@example.test', 'private_key' => devicePushPrivateKey()]]);
    $user = User::factory()->create();
    $source = deviceIdentity();
    $sourceCredential = enrollDevice($user, $source)['credential'];
    $target = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $user->id, 'display_name' => 'Target', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1]);
    $targetCredential = Str::random(64);
    $target->credentials()->create(['token_hash' => hash('sha256', $targetCredential), 'expires_at' => now()->addDay()]);
    Http::preventStrayRequests();
    Http::fake([
        'https://oauth2.googleapis.com/token' => Http::response(['access_token' => 'token', 'expires_in' => 3600]),
        'https://fcm.googleapis.com/v1/projects/project-123/messages:send' => Http::response([]),
    ]);

    $this->putJson('/api/native/v1/devices/push-token', ['provider' => 'fcm', 'token' => 'target-token'], ['Authorization' => 'Bearer '.$targetCredential])->assertNoContent();
    $this->putJson('/api/native/v1/devices/push-token', ['provider' => 'fcm', 'token' => 'target-token', 'device_id' => $source['id']], ['Authorization' => 'Bearer '.$targetCredential])->assertUnprocessable();
    expect(DevicePushToken::query()->firstOrFail()->device_id)->toBe($target->id);
    $this->postJson('/api/native/v1/devices/connections', ['target_device_id' => $target->id], ['Authorization' => 'Bearer '.$sourceCredential])->assertCreated();
    Http::assertSentCount(2);

    $other = User::factory()->create();
    $otherDevice = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $other->id, 'display_name' => 'Other', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1]);
    $otherCredential = Str::random(64);
    $otherDevice->credentials()->create(['token_hash' => hash('sha256', $otherCredential), 'expires_at' => now()->addDay()]);
    $this->putJson('/api/native/v1/devices/push-token', ['provider' => 'fcm', 'token' => 'other-token'], ['Authorization' => 'Bearer '.$otherCredential])->assertNoContent();
    expect(DevicePushToken::query()->where('device_id', $target->id)->count())->toBe(1);

    $this->putJson('/api/native/v1/devices/push-token', ['provider' => 'fcm', 'token' => 'source-token'], ['Authorization' => 'Bearer '.$sourceCredential])->assertNoContent();
    $this->actingAs($user)->deleteJson("/api/native/v1/devices/{$target->id}", ['current_password' => 'password'], ['Sec-Fetch-Site' => 'same-origin'])->assertOk();
    Http::assertSent(fn ($request): bool => $request->url() === 'https://fcm.googleapis.com/v1/projects/project-123/messages:send'
        && array_keys($request->data()['message']['data']) === ['registry_revision']);
});

test('disabled wakes do not affect foreground pairing and web revoke deactivates push tokens', function (): void {
    $user = User::factory()->create();
    $source = deviceIdentity();
    $sourceCredential = enrollDevice($user, $source)['credential'];
    $target = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $user->id, 'display_name' => 'Target', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1]);
    $targetCredential = Str::random(64);
    $target->credentials()->create(['token_hash' => hash('sha256', $targetCredential), 'expires_at' => now()->addDay()]);
    Http::preventStrayRequests();
    $this->putJson('/api/native/v1/devices/push-token', ['provider' => 'fcm', 'token' => 'target-token'], ['Authorization' => 'Bearer '.$targetCredential])->assertNoContent();
    $this->postJson('/api/native/v1/devices/connections', ['target_device_id' => $target->id], ['Authorization' => 'Bearer '.$sourceCredential])->assertCreated();
    Http::assertNothingSent();
    $this->actingAs($user)->deleteJson("/api/native/v1/devices/{$target->id}", ['current_password' => 'password'], ['Sec-Fetch-Site' => 'same-origin'])->assertOk();
    expect(DevicePushToken::query()->firstOrFail()->revoked_at)->not->toBeNull();
});

test('inbound polling and ICE remain pair scoped and expose no file metadata', function (): void {
    config()->set('filebeam.instance_settings.environment.enabled_drivers', ['http', 'webrtc']);
    config()->set('filebeam.instance_settings.environment.default_driver', 'webrtc');
    config()->set('filebeam.webrtc.ice_servers', [['urls' => ['stun:relay.example.test']]]);
    config()->set('filebeam.webrtc.turn_urls', ['turn:relay.example.test']);
    config()->set('filebeam.webrtc.turn_secret', 'operator-secret');
    $user = User::factory()->create();
    $source = deviceIdentity();
    $sourceCredential = enrollDevice($user, $source)['credential'];
    $target = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $user->id, 'display_name' => 'Target', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1]);
    $targetCredential = Str::random(64);
    $target->credentials()->create(['token_hash' => hash('sha256', $targetCredential), 'expires_at' => now()->addDay()]);
    $connection = $this->postJson('/api/native/v1/devices/connections', ['target_device_id' => $target->id], ['Authorization' => 'Bearer '.$sourceCredential])->assertCreated()->json('data.connection.id');

    $this->getJson('/api/native/v1/devices/connections', ['Authorization' => 'Bearer '.$targetCredential])
        ->assertOk()->assertJsonPath('data.poll_after_seconds', 30)->assertJsonPath('data.connections.0.id', $connection)
        ->assertJsonMissing(['manifest', 'transfer_id', 'encrypted_manifest', 'receipt']);
    $this->getJson("/api/native/v1/devices/connections/{$connection}/ice", ['Authorization' => 'Bearer '.$targetCredential])
        ->assertOk()->assertJsonFragment(['urls' => ['stun:relay.example.test']])->assertJsonMissing(['operator-secret']);
    $other = User::factory()->create();
    $otherDevice = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $other->id, 'display_name' => 'Other', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1]);
    $otherCredential = Str::random(64);
    $otherDevice->credentials()->create(['token_hash' => hash('sha256', $otherCredential), 'expires_at' => now()->addDay()]);
    $this->getJson("/api/native/v1/devices/connections/{$connection}/ice", ['Authorization' => 'Bearer '.$otherCredential])->assertNotFound();

    $this->actingAs($user)->deleteJson("/api/native/v1/devices/{$target->id}", ['current_password' => 'password'], ['Sec-Fetch-Site' => 'same-origin'])->assertOk();
    $this->getJson("/api/native/v1/devices/connections/{$connection}/ice", ['Authorization' => 'Bearer '.$sourceCredential])->assertNotFound();
});

test('credential rotation is atomic and idempotent', function (): void {
    $user = User::factory()->create();
    $credential = enrollDevice($user, deviceIdentity())['credential'];
    $headers = ['Authorization' => 'Bearer '.$credential, 'Idempotency-Key' => str_repeat('r', 32)];
    $replacement = $this->postJson('/api/native/v1/devices/credentials/rotate', [], $headers)->assertCreated()->json('data.credential');
    $this->postJson('/api/native/v1/devices/credentials/rotate', [], $headers)->assertCreated()->assertJsonPath('data.credential', $replacement);
    $this->getJson('/api/native/v1/devices/sync', ['Authorization' => 'Bearer '.$credential])->assertUnauthorized();
    $this->getJson('/api/native/v1/devices/sync', ['Authorization' => 'Bearer '.$replacement])->assertOk();
});

test('expired credentials cannot be renewed', function (): void {
    $user = User::factory()->create();
    $credential = enrollDevice($user, deviceIdentity())['credential'];
    Device::query()->firstOrFail()->credentials()->update(['expires_at' => now()->subSecond()]);

    $this->postJson('/api/native/v1/devices/credentials/rotate', [], ['Authorization' => 'Bearer '.$credential, 'Idempotency-Key' => str_repeat('e', 32)])->assertUnauthorized();
});

test('device rendezvous accepts SDP syntax but rejects arbitrary metadata text', function (): void {
    $user = User::factory()->create();
    $source = deviceIdentity();
    $credential = enrollDevice($user, $source)['credential'];
    $target = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $user->id, 'display_name' => 'Target', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1]);
    $connection = $this->postJson('/api/native/v1/devices/connections', ['target_device_id' => $target->id], ['Authorization' => 'Bearer '.$credential])->assertCreated()->json('data.connection.id');

    $this->putJson("/api/native/v1/devices/connections/{$connection}/offer", ['description' => ['type' => 'offer', 'sdp' => "v=0\r\na=ice-ufrag:real-sdp\r\n"]], ['Authorization' => 'Bearer '.$credential])->assertNoContent();
    $this->putJson("/api/native/v1/devices/connections/{$connection}/offer", ['description' => ['type' => 'offer', 'sdp' => "v=0\r\na=private-manifest:canary\r\n"]], ['Authorization' => 'Bearer '.$credential])->assertUnprocessable();
});

test('device pruning removes expired candidate and credential state but retains revoked tombstones', function (): void {
    $user = User::factory()->create();
    $pending = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $user->id, 'display_name' => 'Pending', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Pending, 'generation' => 1, 'enrollment_expires_at' => now()->subSecond()]);
    $active = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $user->id, 'display_name' => 'Active', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Active, 'generation' => 1, 'presence_expires_at' => now()->subSecond()]);
    $revoked = Device::query()->create(['id' => (string) Str::ulid(), 'user_id' => $user->id, 'display_name' => 'Revoked', 'platform' => 'linux', 'public_key' => random_bytes(32), 'key_version' => 1, 'status' => DeviceStatus::Revoked, 'generation' => 1, 'revoked_at' => now()->subDay()]);
    $expired = $active->credentials()->create(['token_hash' => hash('sha256', 'expired'), 'expires_at' => now()->subDay()]);
    $push = DevicePushToken::query()->create(['device_id' => $revoked->id, 'provider' => 'fcm', 'token_hash' => hash('sha256', 'push'), 'token_ciphertext' => encrypt('push'), 'revoked_at' => now()->subDay()]);
    DeviceCredentialRotation::query()->create(['device_credential_id' => $expired->id, 'idempotency_key_hash' => hash('sha256', 'rotation'), 'replacement_token_hash' => hash('sha256', 'replacement'), 'replacement_token_ciphertext' => encrypt('replacement'), 'expires_at' => now()->subSecond()]);

    expect(Artisan::call('filebeam:prune-devices'))->toBe(0);

    $this->assertDatabaseMissing('devices', ['id' => $pending->id]);
    expect($active->fresh()->presence_expires_at)->toBeNull();
    $this->assertDatabaseHas('devices', ['id' => $revoked->id]);
    $this->assertDatabaseMissing('device_credentials', ['id' => $expired->id]);
    $this->assertDatabaseMissing('device_credential_rotations', ['device_credential_id' => $expired->id]);
    $this->assertDatabaseMissing('device_push_tokens', ['id' => $push->id]);
});
