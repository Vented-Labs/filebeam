<?php

declare(strict_types=1);

use App\Enums\TransferDelivery;
use App\Enums\TransferStatus;
use App\Http\Middleware\HandleInertiaRequests;
use App\Models\AccountKeyBundle;
use App\Models\Filestore;
use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferItem;
use App\Models\TransferKeyEnvelope;
use App\Models\User;
use App\Notifications\InboxTransferCompleted;
use App\Services\FilebeamUrlGenerator;
use App\Support\InstanceSettings;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Http\Request;
use Illuminate\Notifications\ChannelManager;
use Illuminate\Support\Facades\Notification;
use Illuminate\Support\Facades\Password;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\Str;

beforeEach(function (): void {
    config()->set('filebeam.filesystems.environment', null);
    Plan::factory()->create(['slug' => 'default']);
    $this->seed(FilestoreSeeder::class);
});

function receivingKey(): array
{
    $key = random_bytes(32);

    return [rtrim(strtr(base64_encode($key), '+/', '-_'), '='), hash('sha256', $key)];
}

function recipientEnvelope(): string
{
    return rtrim(strtr(base64_encode(random_bytes(80)), '+/', '-_'), '=');
}

/** @return array{0: Transfer, 1: string} */
function pendingInboxTransfer(User $recipient, AccountKeyBundle $bundle, bool $complete = true): array
{
    $uploadToken = Str::random(64);
    $filestore = Filestore::query()->firstOrFail();
    $transfer = Transfer::factory()->create([
        'delivery' => TransferDelivery::Inbox,
        'recipient_id' => $recipient->id,
        'status' => TransferStatus::Pending,
        'protocol_version' => 1,
        'declared_ciphertext_bytes' => 16,
        'item_count' => 1,
        'upload_token_hash' => hash('sha256', $uploadToken),
        'expires_at' => now()->addDay(),
        'filestore_ids' => [$filestore->id],
    ]);
    $item = TransferItem::factory()->for($transfer, 'transfer')->create([
        'declared_ciphertext_bytes' => 16,
        'ciphertext_bytes' => $complete ? 16 : 0,
    ]);
    if ($complete) {
        $chunk = $item->chunks()->create([
            'position' => 0,
            'ciphertext_bytes' => 16,
            'checksum' => hash('sha256', Str::random()),
        ]);
        $chunk->locations()->create([
            'filestore_id' => $filestore->id,
            'storage_path' => 'transfers/'.$transfer->id.'/test.bin',
            'ciphertext_bytes' => 16,
        ]);
    }
    TransferKeyEnvelope::factory()->for($transfer)->for($bundle, 'accountKeyBundle')->create(['role' => 'recipient', 'encrypted_key' => '']);

    return [$transfer, $uploadToken];
}

test('username receiving pages respect the instance gate and expose no private key', function (): void {
    $this->withoutVite();
    config()->set('inertia.testing.ensure_pages_exist', false);
    [$publicKey, $fingerprint] = receivingKey();
    $user = User::factory()->create(['username' => 'receiver', 'normalized_username' => 'receiver', 'inbox_enabled' => true]);
    AccountKeyBundle::factory()->for($user)->create(['public_key' => $publicKey, 'fingerprint' => $fingerprint, 'encrypted_private_key' => 'private']);

    $this->get('/u/receiver')->assertOk()->assertInertia(fn ($page) => $page->component('Receive')->has('recipient')->missing('recipient.encrypted_private_key'));
    InstanceSetting::query()->create(['key' => 'username_routing', 'value' => false]);
    $this->get('/u/receiver')->assertNotFound();
});

test('username receive routes accept only registered usernames and cannot shadow transfer links', function (): void {
    $this->withoutVite();
    config()->set('inertia.testing.ensure_pages_exist', false);
    $transferId = (string) Str::ulid();

    $this->get("/{$transferId}")->assertOk()->assertInertia(fn ($page) => $page->component('Transfer'));
    $this->get("/u/{$transferId}")->assertNotFound();
    $this->post('/register', [
        'username' => 'u',
        'name' => 'Reserved',
        'email' => 'reserved@example.test',
        'password' => 'password',
        'password_confirmation' => 'password',
    ])->assertSessionHasErrors('username');
});

test('password custody requires the current password and validates its envelope', function (): void {
    [$publicKey, $fingerprint] = receivingKey();
    $user = User::factory()->create(['password' => 'secret-password']);
    $payload = ['public_key' => $publicKey, 'fingerprint' => $fingerprint, 'custody_mode' => 'password', 'encrypted_private_key' => json_encode([
        'v' => 1, 'kdf' => ['name' => 'argon2id', 'memory_kib' => 65536, 'iterations' => 3, 'parallelism' => 1],
        'salt' => rtrim(strtr(base64_encode(random_bytes(16)), '+/', '-_'), '='),
        'nonce_prefix' => rtrim(strtr(base64_encode(random_bytes(16)), '+/', '-_'), '='),
        'ciphertext' => rtrim(strtr(base64_encode(random_bytes(48)), '+/', '-_'), '='),
    ])];

    $this->actingAs($user)->postJson('/account/keys', $payload)->assertUnprocessable()->assertJsonValidationErrors('current_password');
    $this->actingAs($user)->postJson('/account/keys', [...$payload, 'current_password' => 'wrong-password'])->assertUnprocessable()->assertJsonValidationErrors('current_password');
    $this->actingAs($user)->postJson('/account/keys', [...$payload, 'current_password' => 'secret-password'])->assertCreated()->assertJsonPath('data.custody_mode', 'password');
    [$selfPublicKey, $selfFingerprint] = receivingKey();
    $this->actingAs($user)->postJson('/account/keys', [
        'public_key' => $selfPublicKey,
        'fingerprint' => $selfFingerprint,
        'custody_mode' => 'self',
        'encrypted_private_key' => 'plaintext',
        'replace' => true,
    ])->assertUnprocessable()->assertJsonValidationErrors('encrypted_private_key');
    $this->actingAs($user)->postJson('/account/keys', [
        'public_key' => $selfPublicKey,
        'fingerprint' => $selfFingerprint,
        'custody_mode' => 'self',
        'replace' => true,
    ])->assertCreated()->assertJsonPath('data.encrypted_private_key', null);
    $this->actingAs($user)->patchJson('/account/inbox', ['enabled' => true])->assertOk();
});

test('the first key atomically activates the inbox and remains readable while routing is disabled', function (): void {
    [$publicKey, $fingerprint] = receivingKey();
    $user = User::factory()->create();

    $this->actingAs($user)->postJson('/account/keys', [
        'public_key' => $publicKey,
        'fingerprint' => $fingerprint,
        'custody_mode' => 'self',
    ])->assertCreated()->assertJsonPath('data.encrypted_private_key', null);
    expect($user->refresh()->inbox_enabled)->toBeTrue()
        ->and($user->accountKeyBundles()->where('is_active', true)->count())->toBe(1);

    InstanceSetting::query()->create(['key' => 'username_routing', 'value' => false]);
    $this->actingAs($user)->getJson('/account/keys')->assertOk()->assertHeader('Cache-Control', 'no-store, private');
    $this->actingAs($user)->postJson('/account/keys', [
        'public_key' => $publicKey,
        'fingerprint' => $fingerprint,
        'custody_mode' => 'self',
        'replace' => true,
    ])->assertNotFound();
});

test('account public keys must be canonical non-low-order X25519 points', function (): void {
    $user = User::factory()->create();
    $publicKey = rtrim(strtr(base64_encode(str_repeat("\0", 32)), '+/', '-_'), '=');

    $this->actingAs($user)->postJson('/account/keys', [
        'public_key' => $publicKey,
        'fingerprint' => hash('sha256', str_repeat("\0", 32)),
        'custody_mode' => 'self',
    ])->assertUnprocessable()->assertJsonValidationErrors('public_key');
});

test('password key envelopes reject unrecognized fields', function (): void {
    [$publicKey, $fingerprint] = receivingKey();
    $user = User::factory()->create(['password' => 'secret-password']);
    $envelope = [
        'v' => 1, 'kdf' => ['name' => 'argon2id', 'memory_kib' => 65536, 'iterations' => 3, 'parallelism' => 1],
        'salt' => rtrim(strtr(base64_encode(random_bytes(16)), '+/', '-_'), '='),
        'nonce_prefix' => rtrim(strtr(base64_encode(random_bytes(16)), '+/', '-_'), '='),
        'ciphertext' => rtrim(strtr(base64_encode(random_bytes(48)), '+/', '-_'), '='),
        'unexpected' => true,
    ];

    $this->actingAs($user)->postJson('/account/keys', [
        'public_key' => $publicKey,
        'fingerprint' => $fingerprint,
        'custody_mode' => 'password',
        'encrypted_private_key' => json_encode($envelope),
        'current_password' => 'secret-password',
    ])->assertUnprocessable()->assertJsonValidationErrors('encrypted_private_key');
});

test('inbox metadata and public endpoints never disclose recipient ciphertext', function (): void {
    $recipient = User::factory()->create(['notification_channel' => 'database']);
    $other = User::factory()->create();
    $bundle = AccountKeyBundle::factory()->for($recipient)->create();
    $transfer = Transfer::factory()->create(['delivery' => TransferDelivery::Inbox, 'recipient_id' => $recipient->id, 'status' => TransferStatus::Available, 'expires_at' => now()->addDay()]);
    TransferKeyEnvelope::factory()->for($transfer)->for($bundle, 'accountKeyBundle')->create(['role' => 'recipient', 'encrypted_key' => str_repeat('A', 80)]);

    $this->actingAs($other)->getJson("/account/inbox/{$transfer->id}/metadata")->assertNotFound();
    $this->actingAs($recipient)->getJson("/account/inbox/{$transfer->id}/metadata")->assertOk()->assertJsonPath('data.recipient_key.encrypted_key', str_repeat('A', 80));
    $unrelatedTransfer = Transfer::factory()->create(['delivery' => TransferDelivery::Inbox, 'recipient_id' => $recipient->id, 'status' => TransferStatus::Available, 'expires_at' => now()->addDay()]);
    $unrelatedItem = TransferItem::factory()->for($unrelatedTransfer, 'transfer')->create();
    $this->actingAs($recipient)->get("/account/inbox/{$transfer->id}/items/{$unrelatedItem->id}/chunks/0")->assertNotFound();
    $this->getJson("/api/v1/transfers/{$transfer->id}")->assertNotFound();
});

test('inbox completion sends one notification and rejects missing or substituted envelopes', function (): void {
    Notification::fake();
    $recipient = User::factory()->create(['notification_channel' => 'mail', 'inbox_enabled' => true]);
    $bundle = AccountKeyBundle::factory()->for($recipient)->create();
    [$transfer, $uploadToken] = pendingInboxTransfer($recipient, $bundle);

    $this->postJson("/api/v1/transfers/{$transfer->id}/complete", ['encrypted_manifest' => 'manifest'], ['X-Filebeam-Upload-Token' => $uploadToken])->assertConflict();
    expect($transfer->keyEnvelopes()->sole()->encrypted_key)->toBe('');

    $encryptedKey = recipientEnvelope();
    $this->postJson("/api/v1/transfers/{$transfer->id}/complete", [
        'encrypted_manifest' => 'manifest',
        'encrypted_key' => $encryptedKey,
    ], ['X-Filebeam-Upload-Token' => $uploadToken])->assertOk();
    Notification::assertSentToTimes($recipient, InboxTransferCompleted::class, 1);

    $this->postJson("/api/v1/transfers/{$transfer->id}/complete", [
        'encrypted_manifest' => 'manifest',
        'encrypted_key' => $encryptedKey,
    ], ['X-Filebeam-Upload-Token' => $uploadToken])->assertOk();
    Notification::assertSentToTimes($recipient, InboxTransferCompleted::class, 1);

    $this->postJson("/api/v1/transfers/{$transfer->id}/complete", [
        'encrypted_manifest' => 'manifest',
        'encrypted_key' => recipientEnvelope(),
    ], ['X-Filebeam-Upload-Token' => $uploadToken])->assertConflict();
    Notification::assertSentToTimes($recipient, InboxTransferCompleted::class, 1);
});

test('incomplete inbox transfers roll back without notifying a recipient', function (): void {
    Notification::fake();
    $recipient = User::factory()->create(['inbox_enabled' => true]);
    $bundle = AccountKeyBundle::factory()->for($recipient)->create();
    [$transfer, $uploadToken] = pendingInboxTransfer($recipient, $bundle, complete: false);

    $this->postJson("/api/v1/transfers/{$transfer->id}/complete", [
        'encrypted_manifest' => 'manifest',
        'encrypted_key' => recipientEnvelope(),
    ], ['X-Filebeam-Upload-Token' => $uploadToken])->assertConflict();

    expect($transfer->refresh()->status)->toBe(TransferStatus::Pending)
        ->and($transfer->keyEnvelopes()->sole()->encrypted_key)->toBe('');
    Notification::assertNothingSent();
});

test('authenticated shares include only unread inbox notification counts', function (): void {
    $user = User::factory()->create(['notification_channel' => 'database']);
    $user->notifications()->create([
        'id' => (string) Str::uuid(),
        'type' => InboxTransferCompleted::class,
        'data' => ['message' => 'A new encrypted transfer is ready in your inbox.'],
    ]);

    $request = Request::create('/');
    $request->setUserResolver(fn (): User => $user);
    $shared = app(HandleInertiaRequests::class)->share($request);
    $authUser = $shared['auth']['user']();

    expect($authUser['unread_inbox_notifications'])->toBe(1);
});

test('inbox notifications honor mail and database preferences', function (): void {
    $databaseUser = User::factory()->create(['notification_channel' => 'database']);
    expect((new InboxTransferCompleted)->via($databaseUser))->toBe(['database']);
    (new ChannelManager(app()))->sendNow($databaseUser, new InboxTransferCompleted);
    expect($databaseUser->notifications()->where('type', InboxTransferCompleted::class)->count())->toBe(1);

    Notification::fake();
    $mailUser = User::factory()->create(['notification_channel' => 'mail']);
    expect((new InboxTransferCompleted)->via($mailUser))->toBe(['mail']);
    $mailUser->notify(new InboxTransferCompleted);
    Notification::assertSentTo($mailUser, InboxTransferCompleted::class, fn (InboxTransferCompleted $notification, array $channels): bool => $channels === ['mail']);
});

test('inbox completion requires a canonical 80-byte recipient envelope and an active recipient', function (): void {
    Notification::fake();
    $recipient = User::factory()->create(['username' => 'receiver', 'normalized_username' => 'receiver', 'inbox_enabled' => true]);
    $bundle = AccountKeyBundle::factory()->for($recipient)->create();
    [$transfer, $token] = pendingInboxTransfer($recipient, $bundle);
    $transferId = $transfer->id;

    $this->postJson("/api/v1/transfers/{$transferId}/complete", [
        'encrypted_manifest' => 'manifest',
        'encrypted_key' => str_repeat('A', 80),
    ], ['X-Filebeam-Upload-Token' => $token])->assertUnprocessable()->assertJsonValidationErrors('encrypted_key');
    $recipient->forceFill(['suspended_at' => now()])->save();
    $this->postJson("/api/v1/transfers/{$transferId}/complete", [
        'encrypted_manifest' => 'manifest',
        'encrypted_key' => recipientEnvelope(),
    ], ['X-Filebeam-Upload-Token' => $token])->assertConflict();
    expect(TransferKeyEnvelope::query()->where('transfer_id', $transferId)->sole()->encrypted_key)->toBe('');

    $recipient->forceFill(['suspended_at' => null])->save();
    $encryptedKey = recipientEnvelope();
    $this->postJson("/api/v1/transfers/{$transferId}/complete", [
        'encrypted_manifest' => 'manifest',
        'encrypted_key' => $encryptedKey,
    ], ['X-Filebeam-Upload-Token' => $token])->assertOk();
    expect(TransferKeyEnvelope::query()->where('transfer_id', $transferId)->sole()->encrypted_key)->toBe($encryptedKey);
});

test('optional username domains supplement the permanent path and obey the routing switch', function (): void {
    $this->withoutVite();
    config()->set('filebeam.username_domain', 'fbea.me');
    Route::middleware('web')->group(base_path('routes/web.php'));
    $recipient = User::factory()->create(['username' => 'receiver', 'normalized_username' => 'receiver', 'inbox_enabled' => true]);
    AccountKeyBundle::factory()->for($recipient)->create();

    expect(app(InstanceSettings::class)->boolean('username_routing'))->toBeTrue()
        ->and(app(FilebeamUrlGenerator::class)->profile('receiver'))->toBe('https://fbea.me/receiver');
    $this->get('https://fbea.me/receiver')->assertOk()->assertInertia(fn ($page) => $page->component('Receive')->where('recipient.username', 'receiver'));
    $this->get('http://localhost/u/receiver')->assertOk();
    $this->get('http://localhost/receiver')->assertNotFound();

    InstanceSetting::query()->create(['key' => 'username_routing', 'value' => false]);
    $this->get('https://fbea.me/receiver')->assertNotFound();
    $this->get('http://localhost/u/receiver')->assertNotFound();

    config()->set('filebeam.username_domain', null);
    config()->set('app.url', 'https://files.example.test');
    expect(app(FilebeamUrlGenerator::class)->profile('receiver'))->toBe('https://files.example.test/u/receiver');
});

test('password resets preserve encrypted inbox keys and warn about the original password', function (): void {
    Notification::fake();
    $recipient = User::factory()->create(['inbox_enabled' => true]);
    $bundle = AccountKeyBundle::factory()->for($recipient)->create(['custody_mode' => 'password', 'encrypted_private_key' => 'existing-encrypted-private-key']);
    $token = Password::createToken($recipient);

    $this->post('/reset-password', [
        'token' => $token,
        'email' => $recipient->email,
        'password' => 'NewAccount8!Password',
        'password_confirmation' => 'NewAccount8!Password',
    ])->assertRedirect('/login')->assertSessionHas('status', fn ($status): bool => str_contains($status, 'original password'));

    expect($bundle->refresh()->encrypted_private_key)->toBe('existing-encrypted-private-key')
        ->and($bundle->is_active)->toBeTrue();
});

test('viewing the inbox clears prior in-app notifications after changing preference to email', function (): void {
    $this->withoutVite();
    $user = User::factory()->create(['notification_channel' => 'database']);
    (new ChannelManager(app()))->sendNow($user, new InboxTransferCompleted);
    $user->update(['notification_channel' => 'mail']);
    expect($user->unreadNotifications()->count())->toBe(1);

    $this->actingAs($user)->get('/account/inbox')->assertOk();
    expect($user->unreadNotifications()->count())->toBe(0);
});

test('deactivating the only active key also pauses new deliveries', function (): void {
    $user = User::factory()->create(['inbox_enabled' => true]);
    $bundle = AccountKeyBundle::factory()->for($user)->create();

    $this->actingAs($user)->patchJson('/account/keys/'.$bundle->id, ['is_active' => false])->assertOk();
    expect($user->refresh()->inbox_enabled)->toBeFalse();
});

test('anonymous inbox reservations pin the recipient key and respect the global routing gate', function (): void {
    $user = User::factory()->create(['username' => 'receiver', 'normalized_username' => 'receiver', 'inbox_enabled' => true]);
    $bundle = AccountKeyBundle::factory()->for($user)->create();
    $payload = [
        'kind' => 'files', 'protocol_version' => 1,
        'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
        'recipient_username' => $user->username,
        'account_key_bundle_id' => $bundle->id,
    ];
    $response = $this->postJson('/api/v1/transfers', $payload)->assertCreated();
    $transfer = Transfer::query()->findOrFail($response->json('data.id'));
    expect($transfer->delivery)->toBe(TransferDelivery::Inbox)
        ->and($transfer->recipient_id)->toBe($user->id)
        ->and($transfer->keyEnvelopes()->sole()->account_key_bundle_id)->toBe($bundle->id);
    $this->getJson('/api/v1/transfers/'.$transfer->id)->assertNotFound();

    InstanceSetting::query()->create(['key' => 'username_routing', 'value' => false]);
    $this->postJson('/api/v1/transfers', $payload)->assertNotFound();
    expect(Transfer::query()->count())->toBe(1);
});
