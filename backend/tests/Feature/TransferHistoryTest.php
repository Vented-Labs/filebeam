<?php

declare(strict_types=1);

use App\Actions\Admin\RequestTransferTakedown;
use App\Actions\Transfers\RemoveTransfer;
use App\Enums\TransferRemovalReason;
use App\Enums\TransferStatus;
use App\Jobs\DeleteTransfer;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\User;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Queue;

beforeEach(function (): void {
    config()->set('app.key', 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=');
    $this->plan = Plan::factory()->create(['slug' => 'default', 'maximum_file_retention_hours' => 168, 'maximum_note_retention_hours' => 72]);
    $this->owner = User::factory()->create(['plan_id' => $this->plan->id]);
});

test('history lists only account-owned operational metadata across active and removed transfers', function (): void {
    Queue::fake();
    $active = Transfer::factory()->create(['owner_id' => $this->owner->id, 'encrypted_manifest' => 'private-ciphertext']);
    $deleted = Transfer::factory()->create(['owner_id' => $this->owner->id, 'driver' => 'webrtc']);
    Transfer::factory()->create();
    Transfer::factory()->create(['owner_id' => User::factory()->create()->id]);
    app(RemoveTransfer::class)->handle($deleted, TransferRemovalReason::Deleted);
    (new DeleteTransfer($deleted->id))->handle();
    (new DeleteTransfer($deleted->id))->handle();

    $response = $this->actingAs($this->owner)->getJson('/api/native/v1/history')->assertOk()->assertHeader('Cache-Control', 'no-store, private');
    expect(array_column($response->json('data'), 'id'))->toEqualCanonicalizing([$active->id, $deleted->id]);
    expect($response->getContent())->not->toContain('private-ciphertext', 'token', 'encrypted_manifest', 'owner_id');
    $this->getJson('/account/history?driver=webrtc&status=deleted')->assertOk()->assertJsonCount(1, 'data')->assertJsonPath('data.0.can_extend', false)->assertJsonPath('data.0.can_delete', false);
    $this->assertDatabaseMissing('transfers', ['id' => $deleted->id]);
    $this->assertDatabaseCount('transfer_history_entries', 1);
});

test('history cursor traverses cleanup without duplicating or losing an entry', function (): void {
    Queue::fake();
    $transfers = Transfer::factory()->count(4)->create(['owner_id' => $this->owner->id, 'driver' => 'webrtc', 'created_at' => now()]);
    $first = $this->actingAs($this->owner)->getJson('/api/native/v1/history?limit=2')->assertOk()->json();
    $remaining = $transfers->firstWhere('id', array_values(array_diff($transfers->pluck('id')->all(), array_column($first['data'], 'id')))[0]);
    app(RemoveTransfer::class)->handle($remaining, TransferRemovalReason::Deleted);
    (new DeleteTransfer($remaining->id))->handle();
    $second = $this->getJson('/api/native/v1/history?limit=2&cursor='.urlencode($first['next_cursor']))->assertOk()->json();
    expect(array_merge(array_column($first['data'], 'id'), array_column($second['data'], 'id')))->toEqualCanonicalizing($transfers->pluck('id')->all());
    expect($second['next_cursor'])->toBeNull();
    $this->getJson('/api/native/v1/history?limit=101')->assertUnprocessable();
    $this->getJson('/api/native/v1/history?cursor=invalid')->assertUnprocessable();
    $this->getJson('/api/native/v1/history?cursor='.urlencode(base64_encode('{"_pointsToNextItems":true}')))->assertUnprocessable();
});

test('history actions reject other owners and anonymous sessions', function (): void {
    $transfer = Transfer::factory()->create();
    $this->getJson('/api/native/v1/history')->assertUnauthorized();
    $this->postJson('/api/v1/transfers', [], ['X-Filebeam-Require-Account' => '1'])->assertUnauthorized();
    $this->actingAs($this->owner)->deleteJson('/api/native/v1/history/'.$transfer->id)->assertNotFound();
    $this->patchJson('/api/native/v1/history/'.$transfer->id.'/retention', ['retention_hours' => 48])->assertNotFound();
});

test('history mutations retain CSRF protection and suspended accounts cannot read history', function (): void {
    app()->detectEnvironment(static fn (): string => 'local');
    Queue::fake();
    $transfer = Transfer::factory()->create(['owner_id' => $this->owner->id, 'status' => 'available', 'completed_at' => now()]);
    $this->actingAs($this->owner)->patchJson('/api/native/v1/history/'.$transfer->id.'/retention', ['retention_hours' => 48])->assertStatus(419);
    $this->deleteJson('/account/history/'.$transfer->id)->assertStatus(419);
    $this->patchJson('/api/native/v1/history/'.$transfer->id.'/retention', ['retention_hours' => 48], ['Sec-Fetch-Site' => 'same-origin'])->assertOk();
    $this->owner->forceFill(['suspended_at' => now()])->save();
    $this->actingAs($this->owner->fresh())->getJson('/api/native/v1/history')->assertForbidden();
});

test('owner deletion is idempotent and preserves the original removal reason', function (): void {
    Queue::fake();
    $transfer = Transfer::factory()->create(['owner_id' => $this->owner->id, 'driver' => 'webrtc']);
    $this->actingAs($this->owner)->deleteJson('/api/native/v1/history/'.$transfer->id)->assertStatus(202);
    app(RemoveTransfer::class)->handle($transfer, TransferRemovalReason::Burned);
    $this->deleteJson('/account/history/'.$transfer->id)->assertStatus(202);
    Queue::assertPushed(DeleteTransfer::class, 1);
    (new DeleteTransfer($transfer->id))->handle();
    $this->deleteJson('/api/native/v1/history/'.$transfer->id)->assertStatus(202);
    $this->assertDatabaseHas('transfer_history_entries', ['id' => $transfer->id, 'status' => 'deleted']);
});

test('burn on read and staff takedown retain distinct owner-only outcomes', function (): void {
    Queue::fake();
    $note = Transfer::factory()->create(['owner_id' => $this->owner->id, 'kind' => 'note', 'status' => 'available', 'burn_on_read' => true, 'read_token_hash' => hash('sha256', 'reader')]);
    $this->postJson('/api/v1/transfers/'.$note->id.'/consume', [], ['X-Filebeam-Read-Token' => 'reader'])->assertStatus(202);
    (new DeleteTransfer($note->id))->handle();
    $removed = Transfer::factory()->create(['owner_id' => $this->owner->id, 'driver' => 'webrtc']);
    app(RequestTransferTakedown::class)->handle(User::factory()->create(['role' => 'admin', 'email_verified_at' => now()]), $removed, 'Remove reported upload');
    (new DeleteTransfer($removed->id))->handle();
    $this->actingAs($this->owner)->getJson('/api/native/v1/history?status=burned')->assertOk()->assertJsonPath('data.0.id', $note->id)->assertJsonPath('data.0.can_extend', false);
    $this->getJson('/api/native/v1/history?status=removed')->assertOk()->assertJsonPath('data.0.id', $removed->id);
});

test('HTTP extension uses completion time and current owner plan with repeat-safe requests', function (): void {
    $transfer = Transfer::factory()->create(['owner_id' => $this->owner->id, 'status' => 'available', 'completed_at' => now()->subHours(3), 'expires_at' => now()->addHours(21)]);
    $url = '/api/native/v1/history/'.$transfer->id.'/retention';
    $this->actingAs($this->owner)->patchJson($url, ['retention_hours' => 48])->assertOk()->assertJsonPath('data.expires_at', $transfer->completed_at->addHours(48)->toIso8601String());
    $this->patchJson($url, ['retention_hours' => 48])->assertOk();
    $this->patchJson($url, ['retention_hours' => 24])->assertConflict();
    $this->patchJson($url, ['retention_hours' => 169])->assertUnprocessable();
    $this->owner->update(['plan_id' => Plan::factory()->create(['maximum_file_retention_hours' => 60])->id]);
    $this->patchJson($url, ['retention_hours' => 72])->assertUnprocessable();
    $this->getJson('/api/native/v1/history')->assertJsonPath('data.0.maximum_retention_hours', 60);
});

test('live extension respects publication time and the instance cap', function (): void {
    config()->set('filebeam.webrtc.live_max_hours', 24);
    $transfer = Transfer::factory()->create(['owner_id' => $this->owner->id, 'driver' => 'webrtc', 'status' => 'live', 'created_at' => now()->subHours(2), 'published_at' => now()->subHour(), 'expires_at' => now()->addHours(3), 'retention_hours' => 4]);
    $this->actingAs($this->owner)->patchJson('/api/native/v1/history/'.$transfer->id.'/retention', ['retention_hours' => 48])->assertOk()->assertJsonPath('data.expires_at', $transfer->created_at->toImmutable()->addHours(24)->toIso8601String());
    $this->getJson('/api/native/v1/history')->assertJsonPath('data.0.can_extend', false);
    $transfer->update(['status' => TransferStatus::Ended]);
    $this->patchJson('/api/native/v1/history/'.$transfer->id.'/retention', ['retention_hours' => 72])->assertConflict();
});

test('expired and pending transfers cannot be revived and history reports expiry before pruning', function (): void {
    $expired = Transfer::factory()->create(['owner_id' => $this->owner->id, 'status' => 'available', 'completed_at' => now()->subDays(2), 'expires_at' => now()->subSecond()]);
    $pending = Transfer::factory()->create(['owner_id' => $this->owner->id]);
    $this->actingAs($this->owner)->getJson('/api/native/v1/history?status=expired')->assertJsonPath('data.0.id', $expired->id)->assertJsonPath('data.0.can_extend', false);
    foreach ([$expired, $pending] as $transfer) {
        $this->patchJson('/api/native/v1/history/'.$transfer->id.'/retention', ['retention_hours' => 168])->assertConflict();
    }
});

test('pruning archives expired and abandoned transfers and purges only overdue summaries', function (): void {
    Queue::fake();
    $expired = Transfer::factory()->create(['owner_id' => $this->owner->id, 'driver' => 'webrtc', 'status' => 'live', 'expires_at' => now()->subSecond()]);
    $abandoned = Transfer::factory()->create(['owner_id' => $this->owner->id, 'driver' => 'webrtc', 'expires_at' => now()->subSecond()]);
    $this->artisan('filebeam:prune-transfers')->assertSuccessful();
    foreach ([$expired, $abandoned] as $transfer) {
        (new DeleteTransfer($transfer->id))->handle();
    }
    $this->assertDatabaseHas('transfer_history_entries', ['id' => $expired->id, 'status' => 'expired']);
    $this->assertDatabaseHas('transfer_history_entries', ['id' => $abandoned->id, 'status' => 'abandoned']);
    DB::table('transfer_history_entries')->where('id', $expired->id)->update(['purge_at' => now()]);
    $this->actingAs($this->owner)->getJson('/api/native/v1/history')->assertJsonCount(1, 'data');
    $this->artisan('filebeam:prune-transfer-history')->assertSuccessful();
    $this->assertDatabaseCount('transfer_history_entries', 1);
    $this->owner->delete();
    $this->assertDatabaseCount('transfer_history_entries', 0);
});
