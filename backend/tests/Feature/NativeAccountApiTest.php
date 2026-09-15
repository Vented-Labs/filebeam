<?php

declare(strict_types=1);

use App\Models\AccountKeyBundle;
use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\User;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Facades\Notification;
use Symfony\Component\HttpFoundation\Cookie;

beforeEach(function (): void {
    config()->set('app.key', 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=');
    config()->set('session.driver', 'cookie');
    config()->set('session.cookie', 'native-account-session');
    app('session')->forgetDrivers();
    app()->forgetInstance('session.store');
});

/** @return array<string, string> */
function nativeSessionCookies(string $email, string $password = 'password'): array
{
    $response = test()->postJson('/api/native/v1/session', ['email' => $email, 'password' => $password], ['Sec-Fetch-Site' => 'same-origin'])
        ->assertOk();
    $cookies = collect($response->headers->getCookies())
        ->mapWithKeys(fn (Cookie $cookie): array => [$cookie->getName() => $cookie->getValue()])
        ->all();

    app('session')->forgetDrivers();
    app()->forgetInstance('session.store');
    Auth::forgetGuards();
    Auth::clearResolvedInstance('auth');

    return $cookies;
}

test('native session login uses the existing authenticated session boundary', function (): void {
    $user = User::factory()->create(['email' => 'native@example.test']);

    $this->postJson('/api/native/v1/session', ['email' => 'NATIVE@EXAMPLE.TEST', 'password' => 'password'])
        ->assertOk()->assertJsonPath('data.id', $user->id)->assertJsonPath('data.email', $user->email);
    $this->getJson('/api/native/v1/session')->assertOk()->assertJsonPath('data.id', $user->id);
    $this->deleteJson('/api/native/v1/session')->assertNoContent();
    $this->getJson('/api/native/v1/session')->assertUnauthorized();
});

test('a native login cookie authorizes assigned-plan uploads, inbox reads, and recipient lookup', function (): void {
    InstanceSetting::query()->create(['key' => 'username_routing', 'value' => true]);
    Plan::factory()->create(['slug' => 'default']);
    $senderPlan = Plan::factory()->create(['maximum_transfer_bytes' => 16]);
    $sender = User::factory()->create(['email' => 'sender@example.test', 'plan_id' => $senderPlan->id]);
    $recipient = User::factory()->create([
        'username' => 'native_receiver',
        'normalized_username' => 'native_receiver',
        'inbox_enabled' => true,
    ]);
    $bundle = AccountKeyBundle::factory()->for($recipient)->create(['is_active' => true]);
    $this->seed(FilestoreSeeder::class);

    $cookies = nativeSessionCookies($sender->email);

    $this->withUnencryptedCookies($cookies)->withCredentials()->getJson('/api/native/v1/session')
        ->assertOk()->assertJsonPath('data.id', $sender->id);
    $this->getJson('/api/native/v1/recipients/native_receiver')
        ->assertOk()->assertJsonPath('data.account_key_bundle_id', $bundle->id);
    $this->withUnencryptedCookies($cookies)->withCredentials()->getJson('/api/native/v1/inbox')
        ->assertOk()->assertJsonPath('data', []);
    $this->withUnencryptedCookies($cookies)->withCredentials()->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'driver' => 'http',
        'protocol_version' => 1,
        'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
        'recipient_username' => $recipient->username,
        'account_key_bundle_id' => $bundle->id,
    ], ['Sec-Fetch-Site' => 'same-origin'])->assertCreated();

    $transfer = Transfer::query()->sole();
    expect($transfer->owner_id)->toBe($sender->id)
        ->and($transfer->plan_id)->toBe($senderPlan->id)
        ->and($transfer->recipient_id)->toBe($recipient->id);
});

test('native writes reject cross-site cookie replay and password changes revoke native sessions', function (): void {
    app()->detectEnvironment(static fn (): string => 'local');
    $user = User::factory()->create(['email' => 'replay@example.test']);

    $this->postJson('/api/native/v1/session', ['email' => $user->email, 'password' => 'password'])->assertStatus(419);
    $cookies = nativeSessionCookies($user->email);
    $this->withUnencryptedCookies($cookies)->withCredentials()->patchJson('/api/native/v1/inbox', ['enabled' => false])->assertStatus(419);
    $this->withUnencryptedCookies($cookies)->withCredentials()->patchJson('/api/native/v1/inbox', ['enabled' => false], ['Sec-Fetch-Site' => 'same-origin'])->assertOk();

    $user->forceFill(['password' => Hash::make('changed-password')])->save();
    Auth::forgetGuards();
    Auth::clearResolvedInstance('auth');
    $this->withUnencryptedCookies($cookies)->withCredentials()->getJson('/api/native/v1/session')->assertUnauthorized();
});

test('native login matches web verification policy and suspended sessions are blocked', function (): void {
    $unverified = User::factory()->unverified()->create(['email' => 'unverified@example.test']);
    $cookies = nativeSessionCookies($unverified->email);
    $this->withUnencryptedCookies($cookies)->withCredentials()->getJson('/api/native/v1/session')
        ->assertOk()->assertJsonPath('data.id', $unverified->id);

    $unverified->forceFill(['suspended_at' => now()])->save();
    Auth::forgetGuards();
    Auth::clearResolvedInstance('auth');
    $this->withUnencryptedCookies($cookies)->withCredentials()->getJson('/api/native/v1/session')->assertForbidden();
});

test('native registration follows the web registration gate and creates a session', function (): void {
    Notification::fake();
    Plan::factory()->create(['slug' => 'default']);

    $this->postJson('/api/native/v1/register', [
        'username' => 'native_user',
        'email' => 'native-register@example.test',
        'password' => 'a-long-secure-password',
        'password_confirmation' => 'a-long-secure-password',
    ], ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->assertJsonPath('data.username', 'native_user');

});

test('native account endpoints keep key envelopes and inbox data private', function (): void {
    $user = User::factory()->create();
    $bundle = AccountKeyBundle::factory()->for($user)->create();

    $this->actingAs($user)->getJson('/api/native/v1/account/keys')
        ->assertOk()->assertJsonPath('data.0.id', $bundle->id)->assertHeader('Cache-Control', 'no-store, private');
    auth()->logout();
    $this->getJson('/api/native/v1/account/keys')->assertUnauthorized();
    $this->actingAs($user)->getJson('/api/native/v1/inbox')->assertOk()->assertHeader('Cache-Control', 'no-store, private');
});

test('native recipient lookup exposes only an active username delivery key', function (): void {
    InstanceSetting::query()->create(['key' => 'username_routing', 'value' => true]);
    $recipient = User::factory()->create([
        'username' => 'native_receiver',
        'normalized_username' => 'native_receiver',
        'inbox_enabled' => true,
    ]);
    $bundle = AccountKeyBundle::factory()->for($recipient)->create(['is_active' => true]);

    $this->getJson('/api/native/v1/recipients/native_receiver')
        ->assertOk()->assertJsonPath('data.id', $recipient->id)
        ->assertJsonPath('data.account_key_bundle_id', $bundle->id)
        ->assertJsonMissing(['encrypted_private_key']);
    $recipient->forceFill(['suspended_at' => now()])->save();
    $this->getJson('/api/native/v1/recipients/native_receiver')->assertNotFound();
});

test('native login has the same suspended-account and rate-limit behavior as browser login', function (): void {
    User::factory()->create(['email' => 'native-suspended@example.test', 'suspended_at' => now()]);
    $this->postJson('/api/native/v1/session', ['email' => 'native-suspended@example.test', 'password' => 'password'])
        ->assertUnprocessable()->assertJsonValidationErrors('email');

    foreach (range(1, 5) as $_) {
        $this->postJson('/api/native/v1/session', ['email' => 'native-rate@example.test', 'password' => 'wrong'])
            ->assertUnprocessable()->assertJsonValidationErrors('email');
    }
    $this->postJson('/api/native/v1/session', ['email' => 'native-rate@example.test', 'password' => 'wrong'])
        ->assertUnprocessable()->assertJsonValidationErrors('email');
});
