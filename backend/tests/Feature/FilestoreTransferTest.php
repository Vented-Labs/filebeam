<?php

declare(strict_types=1);

use App\Enums\TransferStatus;
use App\Http\Controllers\Api\V1\TransferChunkController;
use App\Jobs\DeleteTransfer;
use App\Models\Filestore;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferItem;
use App\Support\FilestoreRegistry;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Contracts\Filesystem\Filesystem;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Storage;

beforeEach(function (): void {
    config()->set('filebeam.filesystems.environment', null);
    config()->set('filebeam.transfers.chunk_bytes', 1);
    $this->plan = Plan::factory()->create(['slug' => 'default']);
});

function filestoreTransferStore(string $disk): Filestore
{
    config()->set("filesystems.disks.{$disk}", [
        'driver' => 'local',
        'root' => storage_path("framework/testing/{$disk}"),
        'visibility' => 'private',
    ]);
    Storage::fake($disk);

    return Filestore::factory()->create([
        'name' => $disk,
        'disk_name' => $disk,
        'source' => 'laravel',
    ]);
}

function filestoreTransferPlan(array $stores, string $mode = 'distribute', ?array $defaults = null): Plan
{
    /** @var Plan $plan */
    $plan = test()->plan;
    $plan->update(['placement_mode' => $mode]);
    $defaults ??= array_map(fn (Filestore $store): int => $store->id, $stores);
    $plan->filestores()->sync(collect($stores)->mapWithKeys(
        fn (Filestore $store): array => [$store->id => ['is_default' => in_array($store->id, $defaults, true)]],
    )->all());

    return $plan->refresh();
}

function reserveFilestoreTransfer(array $extra = []): array
{
    $response = test()->postJson('/api/v1/transfers', [
        'kind' => 'files',
        'protocol_version' => 1,
        'chunk_bytes' => 1,
        'items' => [['ciphertext_bytes' => 17, 'chunk_count' => 1]],
        ...$extra,
    ])->assertCreated();

    return $response->json('data');
}

function uploadFilestoreChunk(array $reservation, string $ciphertext = 'ciphertext-123456'): void
{
    test()->call(
        'PUT',
        "/api/v1/transfers/{$reservation['id']}/items/{$reservation['items'][0]['id']}/chunks/0",
        server: [
            'CONTENT_TYPE' => 'application/octet-stream',
            'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $reservation['upload_token'],
        ],
        content: $ciphertext,
    )->assertCreated();
}

function completeFilestoreTransfer(array $reservation): void
{
    test()->postJson("/api/v1/transfers/{$reservation['id']}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $reservation['upload_token'],
    ])->assertOk();
}

test('replication writes each physical copy once while accounting for one logical chunk', function (): void {
    $stores = [filestoreTransferStore('replica-a'), filestoreTransferStore('replica-b')];
    filestoreTransferPlan($stores, 'replicate');
    $reservation = reserveFilestoreTransfer();
    $ciphertext = 'ciphertext-123456';

    uploadFilestoreChunk($reservation, $ciphertext);
    uploadFilestoreChunk($reservation, $ciphertext);

    $item = TransferItem::query()->findOrFail($reservation['items'][0]['id']);
    $chunk = $item->chunks()->sole();
    $locations = $chunk->locations()->with('filestore')->get();
    expect($item->ciphertext_bytes)->toBe(17)
        ->and($chunk->checksum)->toBe(hash('sha256', $ciphertext))
        ->and($locations)->toHaveCount(2)
        ->and($locations->pluck('filestore_id')->sort()->values()->all())->toBe(collect($stores)->pluck('id')->sort()->values()->all());
    foreach ($locations as $location) {
        Storage::disk($location->filestore->disk_name)->assertExists($location->storage_path);
    }
});

test('a failed replica write preserves committed copies and retries only the missing copy', function (): void {
    $stores = [filestoreTransferStore('failure-a'), filestoreTransferStore('failure-b')];
    filestoreTransferPlan($stores, 'replicate');
    $reservation = reserveFilestoreTransfer();
    $failure = true;
    $registry = Mockery::mock(FilestoreRegistry::class)->makePartial();
    $registry->shouldReceive('disk')->andReturnUsing(function (Filestore $store) use ($stores, &$failure) {
        if ($store->id === $stores[1]->id && $failure) {
            $failure = false;

            $disk = Mockery::mock(Filesystem::class);
            $disk->shouldReceive('put')->once()->andReturnFalse();

            return $disk;
        }

        return Storage::disk($store->disk_name);
    });
    app()->instance(FilestoreRegistry::class, $registry);

    $this->call('PUT', "/api/v1/transfers/{$reservation['id']}/items/{$reservation['items'][0]['id']}/chunks/0", server: [
        'CONTENT_TYPE' => 'application/octet-stream', 'HTTP_X_FILEBEAM_UPLOAD_TOKEN' => $reservation['upload_token'],
    ], content: 'ciphertext-123456')->assertStatus(503);
    $chunk = Transfer::query()->findOrFail($reservation['id'])->chunks()->sole();
    expect($chunk->locations()->pluck('filestore_id')->all())->toBe([$stores[0]->id]);
    $this->postJson("/api/v1/transfers/{$reservation['id']}/complete", ['encrypted_manifest' => 'manifest'], [
        'X-Filebeam-Upload-Token' => $reservation['upload_token'],
    ])->assertConflict();

    uploadFilestoreChunk($reservation);
    expect($chunk->refresh()->locations)->toHaveCount(2);
    completeFilestoreTransfer($reservation);
});

test('reservation snapshots the plan pool and placement mode', function (): void {
    $first = filestoreTransferStore('snapshot-a');
    $second = filestoreTransferStore('snapshot-b');
    $plan = filestoreTransferPlan([$first, $second], 'replicate');
    $reservation = reserveFilestoreTransfer();
    $plan->update(['placement_mode' => 'distribute']);
    $plan->filestores()->sync([$second->id => ['is_default' => true]]);

    uploadFilestoreChunk($reservation);
    $transfer = Transfer::query()->findOrFail($reservation['id']);
    expect($transfer->filestore_ids)->toEqualCanonicalizing([$first->id, $second->id])
        ->and($transfer->placement_mode)->toBe('replicate')
        ->and($transfer->chunks()->sole()->locations)->toHaveCount(2);
});

test('disabled required stores block new reservations but not existing downloads', function (): void {
    $store = filestoreTransferStore('disabled-store');
    filestoreTransferPlan([$store]);
    $reservation = reserveFilestoreTransfer();
    uploadFilestoreChunk($reservation);
    completeFilestoreTransfer($reservation);
    $store->update(['placement_enabled' => false]);

    $this->postJson('/api/v1/transfers', [
        'kind' => 'files', 'protocol_version' => 1, 'chunk_bytes' => 1,
        'items' => [['ciphertext_bytes' => 17, 'chunk_count' => 1]],
    ])->assertUnprocessable();
    $response = $this->get("/api/v1/transfers/{$reservation['id']}/items/{$reservation['items'][0]['id']}/chunks/0")
        ->assertOk();
    expect($response->streamedContent())->toBe('ciphertext-123456');
});

test('distribution stores exactly one physical copy for each chunk', function (): void {
    $stores = [filestoreTransferStore('distribution-a'), filestoreTransferStore('distribution-b')];
    filestoreTransferPlan($stores, 'distribute');
    $reservation = reserveFilestoreTransfer();
    uploadFilestoreChunk($reservation);

    expect(Transfer::query()->findOrFail($reservation['id'])->chunks()->sole()->locations)->toHaveCount(1);
});

test('cleanup removes every replica', function (): void {
    $stores = [filestoreTransferStore('cleanup-a'), filestoreTransferStore('cleanup-b')];
    filestoreTransferPlan($stores, 'replicate');
    $reservation = reserveFilestoreTransfer();
    uploadFilestoreChunk($reservation);
    $locations = Transfer::query()->findOrFail($reservation['id'])->chunks()->sole()->locations()->with('filestore')->get();
    Transfer::query()->findOrFail($reservation['id'])->update(['status' => TransferStatus::Deleting]);

    (new DeleteTransfer($reservation['id']))->handle();
    expect(Transfer::query()->find($reservation['id']))->toBeNull();
    foreach ($locations as $location) {
        Storage::disk($location->filestore->disk_name)->assertMissing($location->storage_path);
    }
});

test('downloads fall back from missing or corrupt copies and fail when all copies are unavailable', function (): void {
    $stores = [filestoreTransferStore('fallback-a'), filestoreTransferStore('fallback-b')];
    filestoreTransferPlan($stores, 'replicate');
    $reservation = reserveFilestoreTransfer();
    uploadFilestoreChunk($reservation);
    completeFilestoreTransfer($reservation);
    $locations = Transfer::query()->findOrFail($reservation['id'])->chunks()->sole()->locations()->with('filestore')->get();
    Storage::disk($locations[0]->filestore->disk_name)->put($locations[0]->storage_path, 'corrupt');

    $response = $this->get("/api/v1/transfers/{$reservation['id']}/items/{$reservation['items'][0]['id']}/chunks/0")
        ->assertOk();
    expect($response->streamedContent())->toBe('ciphertext-123456');
    Storage::disk($locations[1]->filestore->disk_name)->delete($locations[1]->storage_path);
    $this->get("/api/v1/transfers/{$reservation['id']}/items/{$reservation['items'][0]['id']}/chunks/0")
        ->assertStatus(503);
});

test('chunk spooling closes the request input stream after copying it', function (): void {
    $input = tmpfile();
    expect(is_resource($input))->toBeTrue();
    fwrite($input, 'ciphertext-123456');
    rewind($input);
    $request = new Request;
    $request->initialize([], [], [], [], [], ['CONTENT_LENGTH' => '17'], $input);
    $spool = null;

    try {
        $method = new ReflectionMethod(TransferChunkController::class, 'spoolCiphertext');
        [$spool, $bytes, $checksum] = $method->invoke(null, $request, 17);

        expect($bytes)->toBe(17)
            ->and($checksum)->toBe(hash('sha256', 'ciphertext-123456'))
            ->and(is_resource($input))->toBeFalse();
    } finally {
        if (is_resource($spool)) {
            fclose($spool);
        }
        if (is_resource($input)) {
            fclose($input);
        }
    }
});

test('the API neither discloses filestores nor permits client placement overrides', function (): void {
    $store = filestoreTransferStore('private-pool');
    filestoreTransferPlan([$store]);
    $this->getJson('/api/v1/filestores')->assertNotFound();
    $this->postJson('/api/v1/transfers', [
        'kind' => 'files', 'protocol_version' => 1, 'chunk_bytes' => 1,
        'items' => [['ciphertext_bytes' => 17, 'chunk_count' => 1]],
        'filestore_ids' => [$store->id], 'placement_mode' => 'replicate',
    ])->assertUnprocessable()->assertJsonValidationErrors(['filestore_ids', 'placement_mode']);
});

test('no default pool prevents transfer creation while explicit plan defaults select only their subset', function (): void {
    $first = filestoreTransferStore('pool-a');
    $second = filestoreTransferStore('pool-b');
    filestoreTransferPlan([$first, $second], 'replicate', []);
    $this->postJson('/api/v1/transfers', [
        'kind' => 'files', 'protocol_version' => 1, 'chunk_bytes' => 1,
        'items' => [['ciphertext_bytes' => 17, 'chunk_count' => 1]],
    ])->assertUnprocessable();
    expect(Transfer::query()->count())->toBe(0);

    filestoreTransferPlan([$first, $second], 'replicate', [$second->id]);
    $reservation = reserveFilestoreTransfer();
    expect(Transfer::query()->findOrFail($reservation['id'])->filestore_ids)->toBe([$second->id]);
});

test('the Laravel filestore factory and seed use the transfers disk without duplicating it', function (): void {
    Storage::fake('transfers');
    $store = Filestore::factory()->create();
    $this->seed(FilestoreSeeder::class);

    expect($store->source)->toBe('laravel')
        ->and($store->disk_name)->toBe('transfers')
        ->and(Filestore::query()->where('disk_name', 'transfers')->count())->toBe(1);
});
