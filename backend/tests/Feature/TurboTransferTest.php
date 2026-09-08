<?php

declare(strict_types=1);

use App\Enums\TransferDelivery;
use App\Enums\TransferKind;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferItem;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\Queue;
use Illuminate\Support\Facades\Storage;

beforeEach(function (): void {
    config()->set('filebeam.filesystems.environment', null);
    config()->set('filebeam.transfers.chunk_bytes', 1);
    config()->set('cache.default', 'array');
    config()->set('cache.stores.array.serialize', true);
    Cache::purge('array');
    Plan::factory()->create(['slug' => 'default']);
    $this->seed(FilestoreSeeder::class);
    Storage::fake('transfers');
});

function reserveTurbo(array $items = [['ciphertext_bytes' => 17, 'chunk_count' => 1]]): array
{
    return test()->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1,
        'chunk_bytes' => 1,
        'items' => $items,
    ])->assertCreated()->json('data');
}

function publishTurbo(array $reservation, string $descriptor = 'descriptor'): void
{
    test()->putJson("/api/v1/transfers/{$reservation['id']}/descriptor", ['encrypted_descriptor' => $descriptor], [
        'X-Filebeam-Upload-Token' => $reservation['upload_token'],
    ])->assertNoContent();
}

function putTurboChunk(array $reservation, int $position, string $ciphertext, int $item = 0): void
{
    test()->call('PUT', "/api/v1/transfers/{$reservation['id']}/items/{$reservation['items'][$item]['id']}/chunks/{$position}", server: [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $reservation['upload_token'],
    ], content: $ciphertext)->assertCreated();
}

test('pending turbo files become readable only after descriptor publication and descriptors are immutable', function (): void {
    $reservation = reserveTurbo();
    $itemId = $reservation['items'][0]['id'];
    expect($reservation['monitor_token'])->toBeString()->and(strlen($reservation['monitor_token']))->toBe(64);
    $this->getJson("/api/v1/transfers/{$reservation['id']}")->assertNotFound();
    $this->get("/api/v1/transfers/{$reservation['id']}/items/{$itemId}/chunks/0")->assertNotFound();
    publishTurbo($reservation);
    $this->getJson("/api/v1/transfers/{$reservation['id']}")
        ->assertOk()
        ->assertJsonPath('data.status', 'pending')
        ->assertJsonPath('data.encrypted_descriptor', 'descriptor')
        ->assertJsonPath('data.declared_ciphertext_bytes', 17)
        ->assertJsonPath('data.items.0.declared_ciphertext_bytes', 17);
    $pendingChunk = $this->get("/api/v1/transfers/{$reservation['id']}/items/{$itemId}/chunks/0")
        ->assertStatus(202)->assertHeader('Retry-After', '2');
    expect($pendingChunk->headers->get('Cache-Control'))->toContain('no-store');
    publishTurbo($reservation);
    $this->putJson("/api/v1/transfers/{$reservation['id']}/descriptor", ['encrypted_descriptor' => 'other'], [
        'X-Filebeam-Upload-Token' => $reservation['upload_token'],
    ])->assertConflict();
});

test('turbo pending access remains limited to files link transfers', function (): void {
    $transfer = Transfer::factory()->create([
        'kind' => TransferKind::Note,
        'delivery' => TransferDelivery::Link,
        'protocol_version' => 1,
        'encrypted_descriptor' => 'descriptor',
        'expires_at' => now()->addHour(),
    ]);
    $item = TransferItem::factory()->for($transfer)->create();

    $this->getJson("/api/v1/transfers/{$transfer->id}")->assertNotFound();
    $this->get("/api/v1/transfers/{$transfer->id}/items/{$item->id}/chunks/0")->assertNotFound();
});

test('public progress reports contiguous chunks and does not disclose sessions', function (): void {
    $reservation = reserveTurbo([['ciphertext_bytes' => 51, 'chunk_count' => 3]]);
    publishTurbo($reservation);
    putTurboChunk($reservation, 2, str_repeat('x', 17));
    $progress = $this->getJson("/api/v1/transfers/{$reservation['id']}/progress")
        ->assertOk()
        ->assertJsonPath('data.status', 'pending')
        ->assertJsonPath('data.progress', 33)
        ->assertJsonPath('data.items.0.ready_chunks', 0)
        ->assertJsonPath('data.items.0.uploaded_chunks', 1)
        ->assertJsonMissing(['sessions', 'token']);
    expect($progress->headers->get('Cache-Control'))->toContain('no-store');
    putTurboChunk($reservation, 0, str_repeat('x', 17));
    $this->getJson("/api/v1/transfers/{$reservation['id']}/progress")
        ->assertJsonPath('data.items.0.ready_chunks', 1);
});

test('progress remains available when uploader monitoring cache is unavailable', function (): void {
    $reservation = reserveTurbo();
    publishTurbo($reservation);
    putTurboChunk($reservation, 0, str_repeat('x', 17));
    Cache::shouldReceive('get')->once()->andThrow(new RuntimeException('cache unavailable'));

    $this->getJson("/api/v1/transfers/{$reservation['id']}/progress")
        ->assertOk()
        ->assertJsonPath('data.status', 'pending')
        ->assertJsonPath('data.progress', 99)
        ->assertJsonPath('data.uploader_status', 'unavailable');
    Transfer::query()->whereKey($reservation['id'])->update(['status' => 'available']);
    $this->getJson("/api/v1/transfers/{$reservation['id']}/progress")
        ->assertJsonPath('data.status', 'available')
        ->assertJsonPath('data.progress', 100);
});

test('download sessions require a valid selection and keep token reports private and monotonic', function (): void {
    $reservation = reserveTurbo();
    publishTurbo($reservation);
    $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", ['item_ids' => ['01JINVALID']])->assertUnprocessable();
    $session = $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", ['item_ids' => [$reservation['items'][0]['id']]])
        ->assertCreated()->assertJsonMissing(['token_hash'])->json('data');
    $this->patchJson("/api/v1/transfers/{$reservation['id']}/download-sessions/{$session['id']}", [
        'sequence' => 2, 'progress' => 80, 'status' => 'downloading',
    ], ['X-Filebeam-Session-Token' => $session['token']])->assertNoContent();
    $this->patchJson("/api/v1/transfers/{$reservation['id']}/download-sessions/{$session['id']}", [
        'sequence' => 1, 'progress' => 10, 'status' => 'waiting',
    ], ['X-Filebeam-Session-Token' => $session['token']])->assertNoContent();
    $this->getJson("/api/v1/transfers/{$reservation['id']}/monitor", ['X-Filebeam-Monitor-Token' => 'wrong'])->assertForbidden();
    $monitor = $this->getJson("/api/v1/transfers/{$reservation['id']}/monitor", ['X-Filebeam-Monitor-Token' => $reservation['monitor_token']])
        ->assertOk()->assertJsonPath('data.sessions.0.progress', 80)->assertJsonMissing(['token', 'token_hash', 'item_ids']);
    expect($monitor->headers->get('Cache-Control'))->toContain('no-store');
    $this->travel(31)->seconds();
    $this->getJson("/api/v1/transfers/{$reservation['id']}/monitor", ['X-Filebeam-Monitor-Token' => $reservation['monitor_token']])
        ->assertJsonPath('data.sessions.0.status', 'stale');
});

test('serialized session cache stores only scalar timestamps across requests', function (): void {
    $reservation = reserveTurbo();
    publishTurbo($reservation);
    $session = $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", ['item_ids' => [$reservation['items'][0]['id']]])
        ->assertCreated()->json('data');
    $this->patchJson("/api/v1/transfers/{$reservation['id']}/download-sessions/{$session['id']}", [
        'sequence' => 1, 'progress' => 10, 'status' => 'downloading',
    ], ['X-Filebeam-Session-Token' => $session['token']])->assertNoContent();
    $sessions = Cache::get("filebeam:turbo:{$reservation['id']}:sessions");

    expect($sessions[0]['heartbeat_at'])->toBeInt()
        ->and($sessions[0]['expires_at'])->toBeInt();
    $this->getJson("/api/v1/transfers/{$reservation['id']}/monitor", [
        'X-Filebeam-Monitor-Token' => $reservation['monitor_token'],
    ])->assertOk()->assertJsonPath('data.sessions.0.id', $session['id']);
});

test('download session selections are unique and scoped to their transfer', function (): void {
    $first = reserveTurbo();
    $second = reserveTurbo();
    publishTurbo($first);
    publishTurbo($second);
    $this->postJson("/api/v1/transfers/{$first['id']}/download-sessions", [
        'item_ids' => [$first['items'][0]['id'], $first['items'][0]['id']],
    ])->assertUnprocessable()->assertJsonValidationErrors('item_ids.0');
    $this->postJson("/api/v1/transfers/{$first['id']}/download-sessions", [
        'item_ids' => [$second['items'][0]['id']],
    ])->assertUnprocessable();
    $firstSession = $this->postJson("/api/v1/transfers/{$first['id']}/download-sessions", [
        'item_ids' => [$first['items'][0]['id']],
    ])->assertCreated()->json('data');
    $secondSession = $this->postJson("/api/v1/transfers/{$second['id']}/download-sessions", [
        'item_ids' => [$second['items'][0]['id']],
    ])->assertCreated()->json('data');

    $this->getJson("/api/v1/transfers/{$first['id']}/monitor", ['X-Filebeam-Monitor-Token' => $second['monitor_token']])->assertForbidden();
    $this->patchJson("/api/v1/transfers/{$first['id']}/download-sessions/{$firstSession['id']}", [
        'sequence' => 1, 'progress' => 10, 'status' => 'downloading',
    ], ['X-Filebeam-Session-Token' => $secondSession['token']])->assertForbidden();
    $this->patchJson("/api/v1/transfers/{$first['id']}/download-sessions/{$secondSession['id']}", [
        'sequence' => 1, 'progress' => 10, 'status' => 'downloading',
    ], ['X-Filebeam-Session-Token' => $secondSession['token']])->assertNotFound();
    $this->getJson("/api/v1/transfers/{$first['id']}/monitor", ['X-Filebeam-Monitor-Token' => $first['monitor_token']])
        ->assertJsonPath('data.sessions.0.progress', 0);
});

test('completed session reports require availability and terminal reports are sticky', function (): void {
    $reservation = reserveTurbo();
    publishTurbo($reservation);
    $session = $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", [
        'item_ids' => [$reservation['items'][0]['id']],
    ])->assertCreated()->json('data');
    $endpoint = "/api/v1/transfers/{$reservation['id']}/download-sessions/{$session['id']}";
    $headers = ['X-Filebeam-Session-Token' => $session['token']];
    $this->patchJson($endpoint, ['sequence' => 1, 'progress' => 100, 'status' => 'completed'], $headers)->assertConflict();
    Transfer::query()->whereKey($reservation['id'])->update(['status' => 'available']);
    $this->patchJson($endpoint, ['sequence' => 2, 'progress' => 100, 'status' => 'completed'], $headers)->assertNoContent();
    $this->patchJson($endpoint, ['sequence' => 3, 'progress' => 20, 'status' => 'downloading'], $headers)->assertNoContent();

    $this->getJson("/api/v1/transfers/{$reservation['id']}/monitor", ['X-Filebeam-Monitor-Token' => $reservation['monitor_token']])
        ->assertJsonPath('data.sessions.0.status', 'completed')
        ->assertJsonPath('data.sessions.0.progress', 100);
});

test('download sessions enforce the configured active session cap', function (): void {
    config()->set('filebeam.transfers.session_limit', 1);
    $reservation = reserveTurbo();
    publishTurbo($reservation);
    $payload = ['item_ids' => [$reservation['items'][0]['id']]];
    $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", $payload)->assertCreated();
    $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", $payload)->assertStatus(429);
});

test('uploader heartbeats do not extend a turbo pending expiry', function (): void {
    $reservation = reserveTurbo();
    publishTurbo($reservation);
    $transfer = Transfer::query()->findOrFail($reservation['id']);
    $expiresAt = $transfer->expires_at;
    $this->travel(30)->minutes();

    $this->patchJson("/api/v1/transfers/{$reservation['id']}/progress", [], [
        'X-Filebeam-Upload-Token' => $reservation['upload_token'],
    ])->assertNoContent();

    expect($transfer->refresh()->expires_at->toIso8601String())->toBe($expiresAt->toIso8601String());
});

test('expired sessions cannot be reported and replacement session numbers remain unique', function (): void {
    config()->set('filebeam.transfers.session_idle_minutes', 1);
    $reservation = reserveTurbo();
    publishTurbo($reservation);
    $first = $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", ['item_ids' => [$reservation['items'][0]['id']]])
        ->assertCreated()->json('data');
    $this->travel(2)->minutes();
    $this->patchJson("/api/v1/transfers/{$reservation['id']}/download-sessions/{$first['id']}", [
        'sequence' => 1, 'progress' => 50, 'status' => 'downloading',
    ], ['X-Filebeam-Session-Token' => $first['token']])->assertNotFound();
    $second = $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", ['item_ids' => [$reservation['items'][0]['id']]])
        ->assertCreated()->json('data');

    $this->getJson("/api/v1/transfers/{$reservation['id']}/monitor", ['X-Filebeam-Monitor-Token' => $reservation['monitor_token']])
        ->assertJsonPath('data.sessions.0.id', $second['id'])
        ->assertJsonPath('data.sessions.0.number', 2);
});

test('deleting turbo transfers reject public progress, monitoring, sessions, and chunks', function (): void {
    Queue::fake();
    $reservation = reserveTurbo();
    publishTurbo($reservation);
    putTurboChunk($reservation, 0, str_repeat('x', 17));
    $session = $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", ['item_ids' => [$reservation['items'][0]['id']]])
        ->assertCreated()->json('data');
    $this->deleteJson("/api/v1/transfers/{$reservation['id']}", [], [
        'X-Filebeam-Delete-Token' => $reservation['delete_token'],
    ])->assertAccepted();

    $this->getJson("/api/v1/transfers/{$reservation['id']}/progress")->assertNotFound();
    $this->getJson("/api/v1/transfers/{$reservation['id']}/monitor", [
        'X-Filebeam-Monitor-Token' => $reservation['monitor_token'],
    ])->assertNotFound();
    $this->postJson("/api/v1/transfers/{$reservation['id']}/download-sessions", [
        'item_ids' => [$reservation['items'][0]['id']],
    ])->assertNotFound();
    $this->patchJson("/api/v1/transfers/{$reservation['id']}/download-sessions/{$session['id']}", [
        'sequence' => 1, 'progress' => 1, 'status' => 'downloading',
    ], ['X-Filebeam-Session-Token' => $session['token']])->assertNotFound();
    $this->get("/api/v1/transfers/{$reservation['id']}/items/{$reservation['items'][0]['id']}/chunks/0")->assertNotFound();
});

test('published turbo expiry is extended only by newly committed chunks and capped', function (): void {
    config()->set('filebeam.transfers.incomplete_expiry_hours', 2);
    config()->set('filebeam.transfers.pending_max_lifetime_hours', 4);
    $reservation = reserveTurbo([['ciphertext_bytes' => 34, 'chunk_count' => 2]]);
    publishTurbo($reservation);
    $transfer = Transfer::query()->findOrFail($reservation['id']);
    $this->travel(90)->minutes();
    putTurboChunk($reservation, 0, str_repeat('x', 17));
    expect($transfer->refresh()->expires_at->toIso8601String())->toBe(now()->addHours(2)->toIso8601String());
    $this->travel(90)->minutes();
    putTurboChunk($reservation, 1, str_repeat('x', 17));
    expect($transfer->refresh()->expires_at->toIso8601String())->toBe($transfer->created_at->addHours(4)->toIso8601String());
});
