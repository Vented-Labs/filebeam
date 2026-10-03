<?php

declare(strict_types=1);

use App\Enums\ReceivingPolicy;
use App\Enums\TransferDelivery;
use App\Enums\TransferStatus;
use App\Models\AccountKeyBundle;
use App\Models\ContactBlock;
use App\Models\ContactPreference;
use App\Models\Friendship;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferKeyEnvelope;
use App\Models\User;
use App\Notifications\FriendshipChanged;
use App\Support\ReceivingPermissions;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Support\Facades\Notification;

beforeEach(function (): void {
    Notification::fake();
    config()->set('filebeam.filesystems.environment', null);
    Plan::factory()->create(['slug' => 'default']);
    $this->seed(FilestoreSeeder::class);
});

test('friend requests need explicit acceptance and preferences are private and directional', function (): void {
    $alice = User::factory()->create(['username' => 'alice', 'normalized_username' => 'alice', 'inbox_enabled' => true]);
    $bob = User::factory()->create(['username' => 'bobby', 'normalized_username' => 'bobby', 'inbox_enabled' => true]);
    $this->actingAs($alice)->postJson('/api/native/v1/contacts/bobby', ['action' => 'request'])->assertOk()->assertJsonPath('data.contacts.0.status', 'outgoing');
    $this->postJson('/api/native/v1/contacts/bobby', ['action' => 'request'])->assertOk();
    Notification::assertSentToTimes($bob, FriendshipChanged::class, 1);
    $this->postJson('/api/native/v1/contacts/bobby', ['action' => 'accept'])->assertConflict();
    $this->actingAs($bob)->postJson('/api/native/v1/contacts/alice', ['action' => 'request'])->assertOk()->assertJsonPath('data.contacts.0.status', 'incoming');
    expect(Friendship::query()->sole()->accepted_at)->toBeNull();
    $this->postJson('/api/native/v1/contacts/alice', ['action' => 'accept'])->assertOk()->assertJsonPath('data.contacts.0.status', 'accepted');
    $this->postJson('/api/native/v1/contacts/alice', ['action' => 'accept'])->assertOk();
    Notification::assertSentToTimes($alice, FriendshipChanged::class, 1);
    $this->postJson('/api/native/v1/contacts/alice', ['action' => 'preferences', 'canSend' => false, 'autoDownload' => true])->assertOk()->assertJsonPath('data.contacts.0.effective.canSend', false);
    $this->actingAs($alice)->getJson('/api/native/v1/contacts')->assertOk()->assertJsonPath('data.contacts.0.canSend', null)->assertJsonPath('data.contacts.0.effective.canSend', true);
    $this->actingAs($bob)->postJson('/api/native/v1/contacts/alice', ['action' => 'preferences', 'canSend' => null, 'autoDownload' => null])->assertOk()->assertJsonPath('data.contacts.0.effective.canSend', true);
});

test('contact mutations reject self requests and unauthorized transitions', function (): void {
    $alice = User::factory()->create(['username' => 'alice', 'normalized_username' => 'alice']);
    $bob = User::factory()->create(['username' => 'bobby', 'normalized_username' => 'bobby']);
    $stranger = User::factory()->create();
    $this->getJson('/api/native/v1/contacts')->assertUnauthorized();
    $this->actingAs($alice)->postJson('/api/native/v1/contacts/alice', ['action' => 'request'])->assertUnprocessable();
    $this->getJson('/api/native/v1/contacts/bobby')->assertOk()->assertJsonMissingPath('data.email');
    $this->postJson('/api/native/v1/contacts/bobby', ['action' => 'request'])->assertOk();
    $this->postJson('/api/native/v1/contacts/bobby', ['action' => 'decline'])->assertConflict();
    $this->actingAs($stranger)->postJson('/api/native/v1/contacts/alice', ['action' => 'accept'])->assertConflict();
    $this->actingAs($bob)->postJson('/api/native/v1/contacts/alice', ['action' => 'cancel'])->assertConflict();
    $this->postJson('/api/native/v1/contacts/alice', ['action' => 'decline'])->assertOk();
    expect(Friendship::query()->count())->toBe(0);
});

test('suspended contacts remain removable and account deletion cascades relationship data', function (): void {
    $alice = User::factory()->create(['username' => 'alice', 'normalized_username' => 'alice']);
    $bob = User::factory()->create(['username' => 'bobby', 'normalized_username' => 'bobby']);
    $this->actingAs($alice)->postJson('/api/native/v1/contacts/bobby', ['action' => 'request'])->assertOk();
    $this->actingAs($bob)->postJson('/api/native/v1/contacts/alice', ['action' => 'accept'])->assertOk();
    $bob->forceFill(['suspended_at' => now()])->save();
    $this->actingAs($alice)->postJson('/api/native/v1/contacts/bobby', ['action' => 'remove'])->assertOk();
    $this->postJson('/api/native/v1/contacts/bobby', ['action' => 'request'])->assertNotFound();
    $this->postJson('/api/native/v1/contacts/bobby', ['action' => 'block'])->assertOk();
    $bob->delete();
    expect(ContactBlock::query()->count())->toBe(0)->and(Friendship::query()->count())->toBe(0);
});

test('blocking removes the friendship and overrides without deleting delivered files', function (): void {
    $alice = User::factory()->create(['username' => 'alice', 'normalized_username' => 'alice']);
    $bob = User::factory()->create(['username' => 'bobby', 'normalized_username' => 'bobby']);
    $friendship = Friendship::query()->create(['lower_user_id' => $alice->id, 'upper_user_id' => $bob->id, 'requester_id' => $alice->id, 'accepted_at' => now()]);
    ContactPreference::query()->create(['friendship_id' => $friendship->id, 'user_id' => $alice->id, 'can_send' => true]);
    $transfer = Transfer::factory()->create(['delivery' => TransferDelivery::Inbox, 'recipient_id' => $alice->id, 'owner_id' => $bob->id, 'status' => TransferStatus::Available]);
    $this->actingAs($alice)->postJson('/api/native/v1/contacts/bobby', ['action' => 'block'])->assertOk()->assertJsonPath('data.blocked.0.username', 'bobby');
    expect(Friendship::query()->count())->toBe(0)->and(ContactPreference::query()->count())->toBe(0)->and($transfer->fresh())->not->toBeNull();
    $this->getJson('/api/native/v1/contacts/bobby')->assertNotFound();
    $this->actingAs($bob)->postJson('/api/native/v1/contacts/alice', ['action' => 'request'])->assertNotFound();
    $this->actingAs($alice)->postJson('/api/native/v1/contacts/bobby', ['action' => 'unblock'])->assertOk();
    expect(ContactBlock::query()->count())->toBe(0)->and(Friendship::query()->count())->toBe(0);
});

test('receiving defaults distinguish anonymous non friends and accepted friends with overrides', function (): void {
    $recipient = User::factory()->create(['inbox_enabled' => true]);
    $sender = User::factory()->create();
    $policy = app(ReceivingPermissions::class);
    expect($policy->resolve($recipient, null)['canSend'])->toBeTrue()->and($policy->resolve($recipient, $sender)['autoDownload'])->toBeFalse();
    $recipient->update(['receiving_policy' => ReceivingPolicy::Authenticated]);
    expect($policy->resolve($recipient, null)['canSend'])->toBeFalse()->and($policy->resolve($recipient, $sender)['canSend'])->toBeTrue();
    $recipient->update(['receiving_policy' => ReceivingPolicy::Friends, 'auto_download_friends' => true]);
    $friendship = Friendship::query()->create(['lower_user_id' => $recipient->id, 'upper_user_id' => $sender->id, 'requester_id' => $sender->id]);
    expect($policy->resolve($recipient, $sender)['canSend'])->toBeFalse();
    $friendship->update(['accepted_at' => now()]);
    expect($policy->resolve($recipient, $sender))->toBe(['canSend' => true, 'autoDownload' => true]);
    $recipient->update(['receiving_policy' => ReceivingPolicy::Nobody]);
    $preference = ContactPreference::query()->create(['friendship_id' => $friendship->id, 'user_id' => $recipient->id, 'can_send' => true]);
    expect($policy->resolve($recipient, $sender)['canSend'])->toBeTrue();
    $preference->update(['can_send' => false]);
    expect($policy->resolve($recipient, $sender))->toBe(['canSend' => false, 'autoDownload' => false]);
    expect($policy->resolve($recipient, null, true)['canSend'])->toBeFalse();
});

test('raw transfer reservation enforces the receiving policy rather than trusting the client', function (): void {
    $recipient = User::factory()->create(['username' => 'alice', 'normalized_username' => 'alice', 'inbox_enabled' => true, 'receiving_policy' => ReceivingPolicy::Friends]);
    $sender = User::factory()->create(['plan_id' => Plan::query()->where('slug', 'default')->sole()->id]);
    $bundle = AccountKeyBundle::factory()->for($recipient)->create();
    $payload = ['kind' => 'files', 'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'), 'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]], 'recipient_username' => 'alice', 'account_key_bundle_id' => $bundle->id];
    $this->postJson('/api/v1/transfers', $payload)->assertUnprocessable();
    $this->actingAs($sender)->postJson('/api/v1/transfers', $payload)->assertUnprocessable();
    Friendship::query()->create(['lower_user_id' => $recipient->id, 'upper_user_id' => $sender->id, 'requester_id' => $recipient->id, 'accepted_at' => now()]);
    $response = $this->postJson('/api/v1/transfers', $payload)->assertCreated();
    $transfer = Transfer::query()->findOrFail($response->json('data.id'));
    expect($transfer->owner_id)->toBe($sender->id)->and((bool) $transfer->sender_authenticated)->toBeTrue();
    ContactBlock::query()->create(['user_id' => $recipient->id, 'blocked_user_id' => $sender->id]);
    $envelope = rtrim(strtr(base64_encode(random_bytes(80)), '+/', '-_'), '=');
    $this->postJson('/api/v1/transfers/'.$transfer->id.'/complete', ['encrypted_manifest' => 'manifest', 'encrypted_key' => $envelope], ['X-Filebeam-Upload-Token' => $response->json('data.upload_token')])->assertConflict();
    expect($transfer->refresh()->status)->toBe(TransferStatus::Pending)->and($transfer->keyEnvelopes()->sole()->encrypted_key)->toBe('');
});

test('inbox synchronization reflects current auto-download policy and staging exposes no private key', function (): void {
    $recipient = User::factory()->create(['inbox_enabled' => true, 'auto_download_friends' => true]);
    $sender = User::factory()->create();
    Friendship::query()->create(['lower_user_id' => $recipient->id, 'upper_user_id' => $sender->id, 'requester_id' => $sender->id, 'accepted_at' => now()]);
    $bundle = AccountKeyBundle::factory()->for($recipient)->create(['encrypted_private_key' => 'private-envelope']);
    $transfer = Transfer::factory()->create(['recipient_id' => $recipient->id, 'owner_id' => $sender->id, 'sender_authenticated' => true, 'delivery' => TransferDelivery::Inbox, 'status' => TransferStatus::Available, 'expires_at' => now()->addDay()]);
    TransferKeyEnvelope::factory()->for($transfer)->for($bundle, 'accountKeyBundle')->create(['role' => 'recipient']);
    $this->actingAs($recipient)->getJson('/api/native/v1/inbox/sync')->assertOk()->assertJsonPath('data.transfers.0.autoDownload', true);
    $this->getJson('/api/native/v1/inbox/'.$transfer->id.'/staging')->assertOk()->assertJsonMissingPath('data.recipient_key.bundle.encrypted_private_key');
    $this->patchJson('/api/native/v1/account/receiving', ['receivingPolicy' => 'anyone', 'autoDownloadFriends' => false])->assertOk();
    $this->actingAs($recipient->refresh());
    $this->getJson('/api/native/v1/inbox/'.$transfer->id.'/staging')->assertConflict();
    $this->getJson('/api/native/v1/inbox/sync')->assertOk()->assertJsonPath('data.transfers.0.autoDownload', false);
    $this->getJson('/api/native/v1/inbox/'.$transfer->id.'/metadata')->assertOk();
});
