<?php

declare(strict_types=1);

use App\Enums\TransferDelivery;
use App\Enums\TransferKind;
use App\Enums\TransferStatus;
use App\Http\Middleware\HandleInertiaRequests;
use App\Jobs\DeleteTransfer;
use App\Models\Filestore;
use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferChunkLocation;
use App\Models\TransferChunkUpload;
use App\Models\TransferItem;
use App\Models\User;
use App\Support\FilestoreRegistry;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Contracts\Filesystem\Filesystem;
use Illuminate\Database\Events\QueryExecuted;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Queue;
use Illuminate\Support\Facades\Storage;
use Illuminate\Support\Str;

beforeEach(function () {
    config()->set('filebeam.filesystems.environment', null);
    Plan::factory()->create(['slug' => 'default']);
    $this->seed(FilestoreSeeder::class);
    Storage::fake('transfers');
});

test('an anonymous encrypted transfer can be uploaded and completed', function () {
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [
            ['ciphertext_bytes' => 16, 'chunk_count' => 1],
        ],
    ])->assertCreated()->assertHeader('Cache-Control', 'no-store, private');

    $transferId = $creation->json('data.id');
    $itemId = $creation->json('data.items.0.id');
    $uploadToken = $creation->json('data.upload_token');
    $ciphertext = str_repeat('x', 16);

    $this->call(
        'PUT',
        "/api/v1/transfers/{$transferId}/items/{$itemId}/chunks/0",
        server: [
            'CONTENT_TYPE' => 'application/octet-stream',
            'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $uploadToken,
        ],
        content: $ciphertext,
    )->assertCreated();

    $this->postJson(
        "/api/v1/transfers/{$transferId}/complete",
        ['encrypted_manifest' => 'encrypted-manifest'],
        ['X-Filebeam-Upload-Token' => $uploadToken],
    )->assertOk()->assertJsonPath('data.id', $transferId);

    $this->getJson("/api/v1/transfers/{$transferId}")
        ->assertOk()
        ->assertJsonMissing(['upload_token' => $uploadToken]);

    $download = $this->get("/api/v1/transfers/{$transferId}/items/{$itemId}/chunks/0");
    $download->assertOk()->assertHeader('Content-Type', 'application/octet-stream');
    expect($download->streamedContent())->toBe($ciphertext);

    $transfer = Transfer::query()->findOrFail($transferId);
    expect($transfer->status)->toBe(TransferStatus::Available)
        ->and($transfer->ciphertext_bytes)->toBe(16)
        ->and($transfer->id)->toBe($transferId)
        ->and($transfer->getAttributes())->not->toHaveKey('filename');

    $item = TransferItem::query()->findOrFail($itemId);
    expect($item->id)->toBe($itemId);
    Storage::disk('transfers')->assertExists($item->chunks()->firstOrFail()->locations()->sole()->storage_path);
});

test('transfer creation rejects unsupported protocol versions', function (): void {
    $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 2,
        'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertUnprocessable()->assertJsonValidationErrors('protocol_version');
});

test('anonymous transfer creation follows the database instance setting', function (): void {
    InstanceSetting::query()->create(['key' => 'anonymous_uploads', 'value' => false]);

    $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1,
        'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertForbidden();

    expect(Transfer::query()->count())->toBe(0);
});

test('disabling anonymous uploads stops guest transfer continuation for signed-in visitors', function (): void {
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1,
        'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();
    InstanceSetting::query()->create(['key' => 'anonymous_uploads', 'value' => false]);
    $user = User::factory()->create();
    $transferId = $creation->json('data.id');
    $itemId = $creation->json('data.items.0.id');
    $headers = [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $creation->json('data.upload_token'),
    ];

    $this->actingAs($user)->call(
        'PUT',
        "/api/v1/transfers/{$transferId}/items/{$itemId}/chunks/0",
        server: $headers,
        content: str_repeat('x', 16),
    )->assertForbidden();
    $this->postJson("/api/v1/transfers/{$transferId}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $creation->json('data.upload_token'),
    ])->assertForbidden();
});

test('disabling anonymous uploads preserves public share reads', function (): void {
    $transfer = Transfer::factory()->create([
        'status' => TransferStatus::Available,
        'expires_at' => now()->addDay(),
    ]);
    $item = TransferItem::factory()->for($transfer, 'transfer')->create();
    $ciphertext = str_repeat('x', 16);
    $chunk = TransferChunk::factory()->for($item, 'item')->create([
        'ciphertext_bytes' => 16,
        'checksum' => hash('sha256', $ciphertext),
    ]);
    $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create([
        'storage_path' => 'public-share-read/chunk.bin',
        'ciphertext_bytes' => 16,
    ]);
    Storage::disk('transfers')->put($location->storage_path, $ciphertext);
    InstanceSetting::query()->create(['key' => 'anonymous_uploads', 'value' => false]);

    $this->getJson("/api/v1/transfers/{$transfer->id}")->assertOk();
    $response = $this->get("/api/v1/transfers/{$transfer->id}/items/{$item->id}/chunks/0")
        ->assertOk();

    expect($response->streamedContent())->toBe($ciphertext);
});

test('disabling anonymous uploads preserves account-owned transfer continuation', function (): void {
    $owner = User::factory()->create();
    $owner->plan->filestores()->sync([Filestore::query()->where('disk_name', 'transfers')->sole()->id => ['is_default' => true]]);
    $creation = $this->actingAs($owner)->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1,
        'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();
    InstanceSetting::query()->create(['key' => 'anonymous_uploads', 'value' => false]);
    auth()->logout();
    $transferId = $creation->json('data.id');
    $itemId = $creation->json('data.items.0.id');
    $uploadToken = $creation->json('data.upload_token');

    $this->call(
        'PUT',
        "/api/v1/transfers/{$transferId}/items/{$itemId}/chunks/0",
        server: [
            'CONTENT_TYPE' => 'application/octet-stream',
            'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $uploadToken,
        ],
        content: str_repeat('x', 16),
    )->assertCreated();
    $this->postJson("/api/v1/transfers/{$transferId}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $uploadToken,
    ])->assertOk();
});

test('upload capabilities are required and chunk retries are idempotent', function () {
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'note',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 17, 'chunk_count' => 1]],
    ])->assertCreated();

    $path = sprintf(
        '/api/v1/transfers/%s/items/%s/chunks/0',
        $creation->json('data.id'),
        $creation->json('data.items.0.id'),
    );
    $headers = [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $creation->json('data.upload_token'),
    ];

    $this->call('PUT', $path, server: ['CONTENT_TYPE' => 'application/octet-stream'], content: str_repeat('a', 17))
        ->assertForbidden();
    $this->call('PUT', $path, server: $headers, content: str_repeat('a', 17))->assertCreated();
    $this->call('PUT', $path, server: $headers, content: str_repeat('a', 17))->assertCreated();
    $this->call('PUT', $path, server: $headers, content: str_repeat('b', 17))->assertConflict();

    expect(Transfer::query()->findOrFail($creation->json('data.id'))->items()->firstOrFail()->chunks()->count())->toBe(1);
});

test('simultaneous identical uploads use separate attempts without deleting the accepted chunk', function () {
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1,
        'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();
    $path = sprintf('/api/v1/transfers/%s/items/%s/chunks/0', $creation->json('data.id'), $creation->json('data.items.0.id'));
    $headers = [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $creation->json('data.upload_token'),
    ];
    $disk = Mockery::mock(Filesystem::class);
    $writes = [];
    $deletes = [];
    $nested = false;

    $registry = Mockery::mock(FilestoreRegistry::class);
    $registry->shouldReceive('placementEnabled')->andReturnTrue();
    $registry->shouldReceive('disk')->with(Mockery::type(Filestore::class))->andReturn($disk);
    app()->instance(FilestoreRegistry::class, $registry);
    $disk->shouldReceive('put')->twice()->andReturnUsing(function (string $storagePath) use (&$nested, &$writes, $path, $headers): bool {
        $writes[] = $storagePath;
        if (! $nested) {
            $nested = true;
            $this->call('PUT', $path, server: $headers, content: str_repeat('x', 16))->assertCreated();
        }

        return true;
    });
    $disk->shouldReceive('delete')->once()->andReturnUsing(function (string $storagePath) use (&$deletes): bool {
        $deletes[] = $storagePath;

        return true;
    });

    $this->call('PUT', $path, server: $headers, content: str_repeat('x', 16))->assertCreated();

    $chunk = Transfer::query()->findOrFail($creation->json('data.id'))->chunks()->sole();
    expect($writes)->toHaveCount(2)
        ->and($writes[0])->not->toBe($writes[1])
        ->and($deletes)->toHaveCount(1)
        ->and($deletes[0])->not->toBe($chunk->locations()->sole()->storage_path);
});

test('a database failure rolls back an uploaded chunk and removes its ciphertext before retry', function () {
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();
    $item = TransferItem::query()->findOrFail($creation->json('data.items.0.id'));
    $path = "/api/v1/transfers/{$item->transfer_id}/items/{$item->id}/chunks/0";
    $headers = [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $creation->json('data.upload_token'),
    ];
    $connection = DB::connection();
    $dispatcher = $connection->getEventDispatcher();
    $connection->setEventDispatcher(clone $dispatcher);
    DB::listen(function (QueryExecuted $query): void {
        $sql = str_replace(['"', '`'], '', strtolower($query->sql));

        if (str_starts_with($sql, 'insert into transfer_chunks')) {
            throw new RuntimeException('Simulated chunk persistence failure.');
        }
    });

    $this->withoutExceptionHandling();

    try {
        expect(fn () => $this->call('PUT', $path, server: $headers, content: str_repeat('x', 16)))
            ->toThrow(RuntimeException::class, 'Simulated chunk persistence failure.');
    } finally {
        $connection->setEventDispatcher($dispatcher);
    }

    expect($item->chunks()->count())->toBe(0)
        ->and($item->refresh()->ciphertext_bytes)->toBe(0);
    expect(TransferChunkUpload::query()->where('transfer_item_id', $item->id)->count())->toBe(0);

    $this->call('PUT', $path, server: $headers, content: str_repeat('x', 16))->assertCreated();
    expect($item->chunks()->count())->toBe(1)
        ->and($item->refresh()->ciphertext_bytes)->toBe(16);
    Storage::disk('transfers')->assertExists($item->chunks()->firstOrFail()->locations()->sole()->storage_path);
});

test('free plan limits are enforced before storage is reserved', function () {
    $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => array_fill(0, 21, ['ciphertext_bytes' => 17, 'chunk_count' => 1]),
    ])->assertUnprocessable()->assertJsonValidationErrors('items');

    expect(Transfer::query()->count())->toBe(0);
});

test('chunk count boundaries accept feasible ciphertext sizes', function (int $chunkCount) {
    config()->set('filebeam.transfers.chunk_bytes', 1);
    $ciphertextBytes = (($chunkCount - 1) * 17) + 17;

    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => $ciphertextBytes, 'chunk_count' => $chunkCount]],
    ])->assertCreated();

    $item = TransferItem::query()->findOrFail($creation->json('data.items.0.id'));
    expect($item->chunk_count)->toBe($chunkCount)
        ->and($item->declared_ciphertext_bytes)->toBe($ciphertextBytes);
})->with([32_767, 32_768, 65_535]);

test('creating one hundred items uses two insert batches and preserves item ordering', function () {
    Plan::query()->where('slug', 'default')->firstOrFail()->update(['maximum_file_count' => 100]);
    $items = array_fill(0, 100, ['ciphertext_bytes' => 16, 'chunk_count' => 1]);

    DB::flushQueryLog();
    DB::enableQueryLog();
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => $items,
    ])->assertCreated();
    $queries = DB::getQueryLog();
    DB::disableQueryLog();

    $transferId = $creation->json('data.id');
    $responseItems = $creation->json('data.items');
    $storedItems = TransferItem::query()->where('transfer_id', $transferId)->orderBy('position')->get();
    $itemInsertQueries = collect($queries)
        ->filter(fn (array $query): bool => preg_match('/insert into ["`]?transfer_items["`]?/i', $query['query']) === 1);

    expect($itemInsertQueries->count())->toBeLessThanOrEqual(2)
        ->and($responseItems)->toHaveCount(100)
        ->and($storedItems)->toHaveCount(100)
        ->and(array_column($responseItems, 'position'))->toBe(range(0, 99))
        ->and(array_column($responseItems, 'id'))->toBe($storedItems->pluck('id')->all());

    foreach ($storedItems as $item) {
        expect(Str::isUlid($item->id))->toBeTrue()
            ->and($item->created_at->toIso8601String())->toBe($item->updated_at->toIso8601String());
    }
});

test('declared item sizes must be possible for their chunk count', function () {
    $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 2]],
    ])->assertUnprocessable()->assertJsonValidationErrors('items.0.ciphertext_bytes');

    expect(Transfer::query()->count())->toBe(0);
});

test('chunk bodies larger than the configured ciphertext limit are rejected', function () {
    config()->set('filebeam.transfers.chunk_bytes', 1);
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();

    $this->call(
        'PUT',
        sprintf('/api/v1/transfers/%s/items/%s/chunks/0', $creation->json('data.id'), $creation->json('data.items.0.id')),
        server: [
            'CONTENT_TYPE' => 'application/octet-stream',
            'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $creation->json('data.upload_token'),
        ],
        content: str_repeat('x', 18),
    )->assertUnprocessable();
});

test('chunk bodies must match their declared ciphertext position exactly', function () {
    config()->set('filebeam.transfers.chunk_bytes', 1);
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1,
        'chunk_bytes' => 1,
        'items' => [['ciphertext_bytes' => 17, 'chunk_count' => 1]],
    ])->assertCreated();

    $this->call(
        'PUT',
        sprintf('/api/v1/transfers/%s/items/%s/chunks/0', $creation->json('data.id'), $creation->json('data.items.0.id')),
        server: [
            'CONTENT_TYPE' => 'application/octet-stream',
            'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $creation->json('data.upload_token'),
        ],
        content: str_repeat('x', 16),
    )->assertUnprocessable();
});

test('transfer creation requires the current negotiated chunk size', function () {
    $payload = [
        'kind' => 'files',
        'protocol_version' => 1,
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ];

    $this->postJson('/api/v1/transfers', $payload)
        ->assertUnprocessable()
        ->assertJsonValidationErrors('chunk_bytes');
    $this->postJson('/api/v1/transfers', [...$payload, 'chunk_bytes' => 1])
        ->assertUnprocessable()
        ->assertJsonValidationErrors('chunk_bytes');
});

test('chunks use filestores selected for the plan', function () {
    config()->set('filebeam.transfers.chunk_bytes', 1);
    $plan = Plan::query()->where('slug', 'default')->sole();
    $stores = collect(['transfers', 'transfer-pool-b', 'transfer-pool-c', 'transfer-pool-d'])
        ->map(fn (string $disk): Filestore => Filestore::query()->firstOrCreate(
            ['disk_name' => $disk],
            ['name' => $disk, 'source' => 'laravel', 'placement_enabled' => true],
        ));
    $plan->filestores()->sync($stores->mapWithKeys(fn (Filestore $store): array => [$store->id => ['is_default' => $store->disk_name === 'transfers']])->all());
    Storage::fake('transfer-pool-b');
    Storage::fake('transfer-pool-c');
    Storage::fake('transfer-pool-d');
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1,
        'chunk_bytes' => 1,
        'items' => [['ciphertext_bytes' => 68, 'chunk_count' => 4]],
    ])->assertCreated();
    $transferId = $creation->json('data.id');
    $token = $creation->json('data.upload_token');

    foreach (range(0, 3) as $position) {
        $item = $creation->json('data.items.0');
        $this->call('PUT', "/api/v1/transfers/{$transferId}/items/{$item['id']}/chunks/{$position}", server: [
            'CONTENT_TYPE' => 'application/octet-stream',
            'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token,
        ], content: str_repeat('x', 17))->assertCreated();
    }

    $chunks = Transfer::query()->findOrFail($transferId)->chunks()->orderBy('transfer_chunks.id')->get();
    expect($chunks)->toHaveCount(4);
    foreach ($chunks as $chunk) {
        $location = $chunk->locations()->sole();
        expect($stores->pluck('id'))->toContain($location->filestore_id);
        Storage::disk($location->filestore->disk_name)->assertExists($location->storage_path);
    }
});

test('chunks retain their assigned filestore location', function () {
    $plan = Plan::query()->where('slug', 'default')->sole();
    $store = Filestore::factory()->create(['name' => 'Alternate transfers', 'disk_name' => 'alternate-transfers-test']);
    $plan->filestores()->sync([$store->id => ['is_default' => true]]);
    Storage::fake('alternate-transfers-test');
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();
    $transferId = $creation->json('data.id');
    $itemId = $creation->json('data.items.0.id');
    $this->call('PUT', "/api/v1/transfers/{$transferId}/items/{$itemId}/chunks/0", server: [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $creation->json('data.upload_token'),
    ], content: str_repeat('x', 16))->assertCreated();

    $chunk = TransferItem::query()->findOrFail($itemId)->chunks()->firstOrFail();
    $location = $chunk->locations()->sole();
    expect($location->filestore_id)->toBe($store->id);
    Storage::disk('alternate-transfers-test')->assertExists($location->storage_path);
});

test('expired incomplete transfers cannot accept chunks or complete', function () {
    config()->set('filebeam.transfers.incomplete_expiry_hours', 1);
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();
    $transferId = $creation->json('data.id');
    $itemId = $creation->json('data.items.0.id');
    $token = $creation->json('data.upload_token');

    $this->travel(2)->hours();

    $this->call('PUT', "/api/v1/transfers/{$transferId}/items/{$itemId}/chunks/0", server: [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token,
    ], content: str_repeat('x', 16))->assertNotFound();
    $this->postJson("/api/v1/transfers/{$transferId}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $token,
    ])->assertNotFound();
});

test('completion is manifest-idempotent and starts the plan retention window', function () {
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();
    $transferId = $creation->json('data.id');
    $itemId = $creation->json('data.items.0.id');
    $token = $creation->json('data.upload_token');
    $this->call('PUT', "/api/v1/transfers/{$transferId}/items/{$itemId}/chunks/0", server: [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $token,
    ], content: str_repeat('x', 16))->assertCreated();

    $this->travel(30)->minutes();
    $this->postJson("/api/v1/transfers/{$transferId}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $token,
    ])->assertOk();
    $completedTransfer = Transfer::query()->findOrFail($transferId);
    $expiresAt = $completedTransfer->expires_at;

    expect($expiresAt->toIso8601String())->toBe($completedTransfer->completed_at->addHours(24)->toIso8601String());

    $this->postJson("/api/v1/transfers/{$transferId}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $token,
    ])->assertOk();
    $this->postJson("/api/v1/transfers/{$transferId}/complete", ['encrypted_manifest' => 'other-manifest'], [
        'X-Filebeam-Upload-Token' => $token,
    ])->assertConflict();

    expect(Transfer::query()->findOrFail($transferId)->expires_at->toIso8601String())->toBe($expiresAt->toIso8601String());
});

test('inbox delivery ciphertext is not publicly readable before recipient authorization exists', function () {
    $transfer = Transfer::factory()->create([
        'delivery' => TransferDelivery::Inbox,
        'status' => TransferStatus::Available,
        'expires_at' => now()->addDay(),
    ]);

    $this->getJson("/api/v1/transfers/{$transfer->id}")->assertNotFound();
});

test('a deletion capability queues ciphertext removal', function () {
    Queue::fake();
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 17, 'chunk_count' => 1]],
    ])->assertCreated();

    $transferId = $creation->json('data.id');
    $this->deleteJson(
        "/api/v1/transfers/{$transferId}",
        headers: ['X-Filebeam-Delete-Token' => $creation->json('data.delete_token')],
    )->assertAccepted();

    expect(Transfer::query()->findOrFail($transferId)->status)->toBe(TransferStatus::Deleting);
    Queue::assertPushed(DeleteTransfer::class, fn (DeleteTransfer $job): bool => $job->transferId === $transferId);
});

test('retention choices are constrained by the seeded plan and snapshotted at reservation', function () {
    $plan = Plan::query()->where('slug', 'default')->firstOrFail();
    $plan->update([
        'default_file_retention_hours' => 24,
        'maximum_file_retention_hours' => 24,
        'default_note_retention_hours' => 720,
        'maximum_note_retention_hours' => 720,
    ]);

    $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'retention_hours' => 72,
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertUnprocessable()->assertJsonValidationErrors('retention_hours');
    $this->postJson('/api/v1/transfers', [
        'kind' => 'note',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'retention_hours' => 8760,
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertUnprocessable()->assertJsonValidationErrors('retention_hours');

    $file = $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();
    $note = $this->postJson('/api/v1/transfers', [
        'kind' => 'note',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'retention_hours' => 6,
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated();

    expect(Transfer::query()->findOrFail($file->json('data.id'))->retention_hours)->toBe(24)
        ->and(Transfer::query()->findOrFail($note->json('data.id'))->retention_hours)->toBe(6);

    $plan->update(['default_note_retention_hours' => 168, 'maximum_note_retention_hours' => 168]);
    $noteTransfer = Transfer::query()->findOrFail($note->json('data.id'));
    $noteItem = $noteTransfer->items()->firstOrFail();
    $this->call('PUT', "/api/v1/transfers/{$noteTransfer->id}/items/{$noteItem->id}/chunks/0", server: [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $note->json('data.upload_token'),
    ], content: str_repeat('x', 16))->assertCreated();
    $this->postJson("/api/v1/transfers/{$noteTransfer->id}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $note->json('data.upload_token'),
    ])->assertOk();

    $completedNote = $noteTransfer->refresh();
    expect($completedNote->expires_at->toIso8601String())
        ->toBe($completedNote->completed_at->addHours(6)->toIso8601String());
});

test('shared retention options include plan defaults and maximums within the plan boundary', function () {
    Plan::query()->where('slug', 'default')->firstOrFail()->update([
        'default_file_retention_hours' => 12,
        'maximum_file_retention_hours' => 168,
        'default_note_retention_hours' => 200,
        'maximum_note_retention_hours' => 200,
    ]);

    $shared = app(HandleInertiaRequests::class)->share(request());

    expect($shared['filebeam']['file_retention_options'])->toBe([1, 6, 12, 24, 72, 168])
        ->and($shared['filebeam']['note_retention_options'])->toBe([1, 6, 12, 24, 72, 168, 200]);
});

test('burn on read is rejected for file transfers', function () {
    $this->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'burn_on_read' => true,
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertUnprocessable()->assertJsonValidationErrors('burn_on_read');
});

test('a burn note consumes only with its read token and immediately blocks reads', function () {
    Queue::fake();
    $creation = $this->postJson('/api/v1/transfers', [
        'kind' => 'note',
        'protocol_version' => 1, 'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'burn_on_read' => true,
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ])->assertCreated()->assertJsonStructure(['data' => ['read_token']]);

    $transferId = $creation->json('data.id');
    $itemId = $creation->json('data.items.0.id');
    $uploadToken = $creation->json('data.upload_token');
    $readToken = $creation->json('data.read_token');
    $transfer = Transfer::query()->findOrFail($transferId);
    expect($readToken)->toBeString()->and(strlen($readToken))->toBeGreaterThanOrEqual(43)
        ->and($transfer->read_token_hash)->toBe(hash('sha256', $readToken));

    $this->call('PUT', "/api/v1/transfers/{$transferId}/items/{$itemId}/chunks/0", server: [
        'CONTENT_TYPE' => 'application/octet-stream',
        'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $uploadToken,
    ], content: str_repeat('x', 16))->assertCreated();
    $this->postJson("/api/v1/transfers/{$transferId}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $uploadToken,
    ])->assertOk();

    $this->getJson("/api/v1/transfers/{$transferId}")
        ->assertOk()
        ->assertJsonPath('data.burn_on_read', true)
        ->assertJsonPath('data.retention_hours', 720)
        ->assertJsonMissing(['read_token' => $readToken, 'read_token_hash' => hash('sha256', $readToken)]);
    $this->getJson("/api/v1/transfers/{$transferId}")->assertOk();
    expect($transfer->refresh()->status)->toBe(TransferStatus::Available);

    $this->postJson("/api/v1/transfers/{$transferId}/consume", [], ['X-Filebeam-Read-Token' => 'wrong'])
        ->assertForbidden();
    expect($transfer->refresh()->status)->toBe(TransferStatus::Available);

    $this->postJson("/api/v1/transfers/{$transferId}/consume", [], ['X-Filebeam-Read-Token' => $readToken])
        ->assertAccepted();
    expect($transfer->refresh()->status)->toBe(TransferStatus::Deleting);
    Queue::assertPushed(DeleteTransfer::class, fn (DeleteTransfer $job): bool => $job->transferId === $transferId);

    $this->getJson("/api/v1/transfers/{$transferId}")->assertNotFound();
    $this->get("/api/v1/transfers/{$transferId}/items/{$itemId}/chunks/0")->assertNotFound();
    $this->postJson("/api/v1/transfers/{$transferId}/consume", [], ['X-Filebeam-Read-Token' => $readToken])
        ->assertAccepted();
});

test('consumption requires an available unexpired burn note link', function () {
    $token = 'read-token';
    foreach ([
        ['status' => TransferStatus::Pending],
        ['status' => TransferStatus::Available, 'kind' => TransferKind::Files],
        ['status' => TransferStatus::Available, 'delivery' => TransferDelivery::Inbox],
        ['status' => TransferStatus::Available, 'burn_on_read' => false],
        ['status' => TransferStatus::Available, 'expires_at' => now()->subSecond()],
    ] as $attributes) {
        $transfer = Transfer::factory()->create([
            'kind' => TransferKind::Note,
            'delivery' => TransferDelivery::Link,
            'burn_on_read' => true,
            'read_token_hash' => hash('sha256', $token),
            'expires_at' => now()->addDay(),
            ...$attributes,
        ]);

        $this->postJson("/api/v1/transfers/{$transfer->id}/consume", [], ['X-Filebeam-Read-Token' => $token])
            ->assertNotFound();
    }
});

test('burn consumption is dispatched only after the surrounding transaction commits', function () {
    Queue::fake();
    $token = 'read-token';
    $transfer = Transfer::factory()->create([
        'kind' => TransferKind::Note,
        'status' => TransferStatus::Available,
        'burn_on_read' => true,
        'read_token_hash' => hash('sha256', $token),
        'expires_at' => now()->addDay(),
    ]);

    DB::transaction(function () use ($transfer, $token): void {
        $this->postJson("/api/v1/transfers/{$transfer->id}/consume", [], ['X-Filebeam-Read-Token' => $token])
            ->assertAccepted();
        Queue::assertNothingPushed();
    });

    Queue::assertPushed(DeleteTransfer::class, fn (DeleteTransfer $job): bool => $job->transferId === $transfer->id);
});
