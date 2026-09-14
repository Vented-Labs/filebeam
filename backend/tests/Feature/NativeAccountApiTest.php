<?php

declare(strict_types=1);

use App\Models\AccountKeyBundle;
use App\Models\InstanceSetting;
use App\Models\User;

test('native session login uses the existing authenticated session boundary', function (): void {
    $user = User::factory()->create(['email' => 'native@example.test']);

    $this->postJson('/api/native/v1/session', ['email' => 'NATIVE@EXAMPLE.TEST', 'password' => 'password'])
        ->assertOk()->assertJsonPath('data.id', $user->id)->assertJsonPath('data.email', $user->email);
    $this->getJson('/api/native/v1/session')->assertOk()->assertJsonPath('data.id', $user->id);
    $this->deleteJson('/api/native/v1/session')->assertNoContent();
    $this->getJson('/api/native/v1/session')->assertUnauthorized();
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
