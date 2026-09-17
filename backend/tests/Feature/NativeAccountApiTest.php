<?php

declare(strict_types=1);

use App\Enums\TransferDelivery;
use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Jobs\DeleteTransfer;
use App\Models\AccountKeyBundle;
use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferKeyEnvelope;
use App\Models\User;
use App\Models\UserInvitation;
use App\Notifications\InboxTransferCompleted;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Auth\Notifications\VerifyEmail;
use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Facades\Notification;
use Illuminate\Support\Facades\Password;
use Illuminate\Support\Facades\Queue;
use Illuminate\Support\Facades\URL;
use Illuminate\Support\Str;
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
    ], ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()
        ->assertJsonPath('data.username', 'native_user')
        ->assertJsonPath('data.notificationChannel', 'mail');

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

test('native inbox listing never acknowledges notifications without an explicit action', function (): void {
    $user = User::factory()->create();
    $notification = $user->notifications()->create(['id' => (string) Str::uuid(), 'type' => InboxTransferCompleted::class, 'data' => []]);

    $this->actingAs($user)->getJson('/api/native/v1/inbox')->assertOk();
    expect($notification->fresh()->read_at)->toBeNull();
    $this->actingAs($user)->postJson('/api/native/v1/inbox/notifications/read', [], ['Sec-Fetch-Site' => 'same-origin'])->assertNoContent();
    expect($notification->fresh()->read_at)->not->toBeNull();
});

test('native account preference and recovery APIs use persisted web behavior', function (): void {
    Notification::fake();
    $user = User::factory()->unverified()->create(['email' => 'native-actions@example.test']);

    $this->actingAs($user)->patchJson('/api/native/v1/account/notifications', ['channel' => 'database'], ['Sec-Fetch-Site' => 'same-origin'])
        ->assertOk()->assertJsonPath('data.channel', 'database');
    expect($user->fresh()->notification_channel)->toBe('database');
    $this->actingAs($user)->postJson('/api/native/v1/account/email/verification-notification', [], ['Sec-Fetch-Site' => 'same-origin'])->assertNoContent();
    auth()->logout();
    $this->postJson('/api/native/v1/password/recovery', ['email' => $user->email], ['Sec-Fetch-Site' => 'same-origin'])->assertNoContent();

    $token = Password::broker()->createToken($user);
    $this->postJson('/api/native/v1/password/reset', ['email' => $user->email, 'token' => $token, 'password' => 'another-long-secure-password', 'password_confirmation' => 'another-long-secure-password'], ['Sec-Fetch-Site' => 'same-origin'])
        ->assertNoContent();
    expect(Hash::check('another-long-secure-password', $user->fresh()->password))->toBeTrue();
});

test('native verification accepts only the current users real signed email capability', function (): void {
    $user = User::factory()->unverified()->create();
    $link = (new VerifyEmail)->toMail($user)->actionUrl;

    $this->actingAs($user)->postJson('/api/native/v1/account/email/verify', ['link' => $link], ['Sec-Fetch-Site' => 'same-origin'])
        ->assertNoContent();
    expect($user->fresh()->hasVerifiedEmail())->toBeTrue();
});

test('native verification rejects unsigned, forged, expired, and cross-account links', function (): void {
    $user = User::factory()->unverified()->create();
    $other = User::factory()->unverified()->create();
    $valid = URL::temporarySignedRoute('verification.verify', now()->addMinutes(10), ['id' => $user->id, 'hash' => sha1($user->email)]);
    $unsigned = rtrim((string) config('app.url'), '/')."/verify-email/{$user->id}/".sha1($user->email);
    $expired = URL::temporarySignedRoute('verification.verify', now()->subMinute(), ['id' => $user->id, 'hash' => sha1($user->email)]);
    $forgedHost = preg_replace('#^https?://[^/]+#', 'https://forged.example', $valid);

    foreach ([$unsigned, $expired, $forgedHost] as $link) {
        $this->actingAs($user)->postJson('/api/native/v1/account/email/verify', ['link' => $link], ['Sec-Fetch-Site' => 'same-origin'])
            ->assertUnprocessable();
    }
    $this->actingAs($other)->postJson('/api/native/v1/account/email/verify', ['link' => $valid], ['Sec-Fetch-Site' => 'same-origin'])
        ->assertUnprocessable();
    expect($user->fresh()->hasVerifiedEmail())->toBeFalse()
        ->and($other->fresh()->hasVerifiedEmail())->toBeFalse();
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

test('native policy is effective for guests and accounts without exposing private account material', function (): void {
    InstanceSetting::query()->create(['key' => 'registration', 'value' => false]);
    InstanceSetting::query()->create(['key' => 'username_routing', 'value' => true]);
    $guestPlan = Plan::factory()->create(['slug' => 'default', 'maximum_transfer_bytes' => 10]);
    $accountPlan = Plan::factory()->create(['maximum_transfer_bytes' => 20, 'default_note_retention_hours' => 72, 'maximum_note_retention_hours' => 72]);
    $user = User::factory()->create(['plan_id' => $accountPlan->id]);

    $this->getJson('/api/native/v1/policy')->assertOk()
        ->assertJsonPath('data.account.registrationEnabled', false)
        ->assertJsonPath('data.account.usernameRoutingEnabled', true)
        ->assertJsonPath('data.transfer.limits.http.maximum_transfer_bytes', $guestPlan->maximum_transfer_bytes)
        ->assertJsonMissing(['turn_secret', 'encrypted_private_key', 'encrypted_manifest']);
    $this->actingAs($user)->getJson('/api/native/v1/policy')->assertOk()
        ->assertJsonPath('data.transfer.limits.http.maximum_transfer_bytes', $accountPlan->maximum_transfer_bytes)
        ->assertJsonPath('data.transfer.retention.notes.defaultHours', 72);
});

test('native invitations can be inspected and accepted when public registration is disabled', function (): void {
    config()->set('filebeam.features.registration', false);
    Plan::factory()->create(['slug' => 'default', 'is_active' => true]);
    $token = 'native-invite-token';
    $invitation = UserInvitation::factory()->create(['email' => 'native-invite@example.test', 'token_hash' => hash('sha256', $token)]);

    $this->getJson("/api/native/v1/invitations/{$token}")->assertOk()
        ->assertJsonPath('data.email', $invitation->email)->assertJsonMissing(['token_hash']);
    expect($invitation->fresh()->accepted_at)->toBeNull();
    $this->postJson("/api/native/v1/invitations/{$token}", [
        'username' => 'native_invitee', 'email' => $invitation->email,
        'password' => 'a-long-secure-password', 'password_confirmation' => 'a-long-secure-password',
    ], ['Sec-Fetch-Site' => 'same-origin'])->assertCreated()->assertJsonPath('data.username', 'native_invitee');
    expect($invitation->fresh()->accepted_at)->not->toBeNull();

    auth()->logout();
    $this->getJson("/api/native/v1/invitations/{$token}")->assertNotFound();
    $expired = UserInvitation::factory()->create(['token_hash' => hash('sha256', 'expired-native-invite'), 'expires_at' => now()->subSecond()]);
    $this->getJson('/api/native/v1/invitations/expired-native-invite')->assertNotFound();
    expect($expired->fresh()->accepted_at)->toBeNull();
});

test('native inbox deletion uses recipient ownership and unread acknowledgement is explicit', function (): void {
    $recipient = User::factory()->create();
    $other = User::factory()->create();
    $transfer = Transfer::factory()->create(['recipient_id' => $recipient->id, 'delivery' => TransferDelivery::Inbox, 'status' => TransferStatus::Available, 'expires_at' => now()->addDay()]);
    $notification = $recipient->notifications()->create(['id' => (string) Str::uuid(), 'type' => InboxTransferCompleted::class, 'data' => []]);

    $this->actingAs($recipient)->getJson('/api/native/v1/inbox/unread-count')->assertOk()->assertJsonPath('data.unreadCount', 1);
    $this->actingAs($other)->deleteJson("/api/native/v1/inbox/{$transfer->id}", [], ['Sec-Fetch-Site' => 'same-origin'])->assertNotFound();
    $this->actingAs($recipient)->postJson('/api/native/v1/inbox/notifications/read', [], ['Sec-Fetch-Site' => 'same-origin'])->assertNoContent();
    expect($notification->fresh()->read_at)->not->toBeNull();
    $this->actingAs($recipient)->deleteJson("/api/native/v1/inbox/{$transfer->id}", [], ['Sec-Fetch-Site' => 'same-origin'])->assertAccepted();
});

test('native reports use the existing validation and confirmation contract', function (): void {
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Available, 'expires_at' => now()->addDay()]);

    $this->postJson('/api/native/v1/reports', ['transfer_id' => $transfer->id, 'category' => 'malware', 'description' => 'This transfer appears to contain malicious software.'], ['Sec-Fetch-Site' => 'same-origin'])
        ->assertAccepted()->assertExactJson(['status' => 'received']);
    $this->postJson('/api/native/v1/reports', ['transfer_id' => 'bad', 'category' => 'invalid', 'description' => 'short'], ['Sec-Fetch-Site' => 'same-origin'])
        ->assertUnprocessable()->assertJsonValidationErrors(['transfer_id', 'category', 'description']);
});

test('native account deletion requires confirmation and password, removes account secrets, and preserves other owners transfers', function (): void {
    Queue::fake();
    $user = User::factory()->create();
    $other = User::factory()->create();
    $bundle = AccountKeyBundle::factory()->for($user)->create();
    $owned = Transfer::factory()->create(['owner_id' => $user->id, 'status' => TransferStatus::Available]);
    $received = Transfer::factory()->create(['owner_id' => $other->id, 'recipient_id' => $user->id, 'delivery' => TransferDelivery::Inbox]);
    $invitation = UserInvitation::factory()->for($user, 'invitedBy')->create();
    TransferKeyEnvelope::factory()->for($owned)->for($bundle)->create();
    TransferKeyEnvelope::factory()->for($received)->for($bundle)->create();
    $user->notifications()->create(['id' => (string) Str::uuid(), 'type' => InboxTransferCompleted::class, 'data' => []]);
    DB::table('sessions')->insert([
        'id' => 'other-native-session', 'user_id' => $user->id, 'payload' => 'serialized-session', 'last_activity' => now()->getTimestamp(),
    ]);

    $this->actingAs($user)->deleteJson('/api/native/v1/account', ['current_password' => 'wrong', 'confirmation' => 'DELETE'], ['Sec-Fetch-Site' => 'same-origin'])->assertUnprocessable()->assertJsonValidationErrors('current_password');
    $this->actingAs($user)->deleteJson('/api/native/v1/account', ['current_password' => 'password', 'confirmation' => 'delete'], ['Sec-Fetch-Site' => 'same-origin'])->assertUnprocessable()->assertJsonValidationErrors('confirmation');
    $this->actingAs($user)->deleteJson('/api/native/v1/account', ['current_password' => 'password', 'confirmation' => 'DELETE'], ['Sec-Fetch-Site' => 'same-origin'])
        ->assertAccepted()->assertJsonPath('data.preserved.transfersOwnedByOthers', true);

    $this->assertDatabaseMissing('users', ['id' => $user->id]);
    $this->assertDatabaseMissing('account_key_bundles', ['id' => $bundle->id]);
    $this->assertDatabaseMissing('sessions', ['user_id' => $user->id]);
    expect($owned->fresh()->status)->toBe(TransferStatus::Deleting)
        ->and($received->fresh()->owner_id)->toBe($other->id)
        ->and($received->fresh()->recipient_id)->toBeNull()
        ->and($invitation->fresh()->invited_by)->toBeNull();
    Queue::assertPushed(DeleteTransfer::class, fn (DeleteTransfer $job): bool => $job->transferId === $owned->id);
    $this->getJson('/api/native/v1/session')->assertUnauthorized();
});

test('a sole active administrator cannot delete their native account', function (): void {
    $admin = User::factory()->create(['role' => UserRole::Admin, 'email_verified_at' => now()]);

    $this->actingAs($admin)->deleteJson('/api/native/v1/account', ['current_password' => 'password', 'confirmation' => 'DELETE'], ['Sec-Fetch-Site' => 'same-origin'])
        ->assertUnprocessable()->assertJsonValidationErrors('confirmation');
    $this->assertDatabaseHas('users', ['id' => $admin->id]);
});
