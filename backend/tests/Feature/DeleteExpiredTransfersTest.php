<?php

declare(strict_types=1);

use App\Enums\TransferStatus;
use App\Jobs\DeleteTransfer;
use App\Models\Filestore;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferChunkLocation;
use App\Models\TransferChunkUpload;
use App\Models\TransferItem;
use App\Support\FilestoreRegistry;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Contracts\Filesystem\Filesystem;
use Illuminate\Database\Events\QueryExecuted;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Queue;
use Illuminate\Support\Facades\Storage;

beforeEach(function (): void {
    config()->set('filebeam.filesystems.environment', null);
    Storage::fake('transfers');
});

test('the pruning command queues expired transfers for idempotent deletion', function () {
    Queue::fake();
    $plan = Plan::factory()->create(['slug' => 'default']);
    $this->seed(FilestoreSeeder::class);
    $transfer = Transfer::factory()->for($plan)->create([
        'status' => TransferStatus::Available,
        'expires_at' => now()->subMinute(),
    ]);
    $item = TransferItem::factory()->for($transfer)->create();
    $chunk = TransferChunk::factory()->for($item, 'item')->create();
    $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create(['storage_path' => 'transfers/'.$transfer->id.'/chunk.bin']);
    Storage::disk('transfers')->put($location->storage_path, 'ciphertext');

    $this->artisan('filebeam:prune-transfers')->assertSuccessful();

    expect($transfer->refresh()->status)->toBe(TransferStatus::Deleting);
    Queue::assertPushed(DeleteTransfer::class);

    (new DeleteTransfer($transfer->id))->handle();

    expect(Transfer::query()->find($transfer->id))->toBeNull();
    Storage::disk('transfers')->assertMissing($location->storage_path);
});

test('a failed ciphertext deletion leaves the transfer deleting for queue retry', function () {
    Storage::fake('transfers');
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    $item = TransferItem::factory()->for($transfer)->create();
    $chunk = TransferChunk::factory()->for($item, 'item')->create();
    $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create();

    $disk = Mockery::mock(Filesystem::class);
    $disk->shouldReceive('delete')->with([$location->storage_path])->once()->andReturnFalse();
    $registry = Mockery::mock(FilestoreRegistry::class);
    $registry->shouldReceive('disk')->with(Mockery::type(Filestore::class))->once()->andReturn($disk);
    app()->instance(FilestoreRegistry::class, $registry);

    expect(fn () => (new DeleteTransfer($transfer->id))->handle())->toThrow(RuntimeException::class);

    expect($transfer->refresh()->status)->toBe(TransferStatus::Deleting);
});

test('a cleanup job never touches storage for a non-deleting transfer', function () {
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Available]);
    TransferItem::factory()->for($transfer)->create();

    Storage::shouldReceive('disk')->never();

    (new DeleteTransfer($transfer->id))->handle();

    expect($transfer->refresh()->status)->toBe(TransferStatus::Available);
});

test('duplicate cleanup jobs safely ignore an already deleted transfer', function () {
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    $item = TransferItem::factory()->for($transfer)->create();
    $chunk = TransferChunk::factory()->for($item, 'item')->create();
    $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create();
    $disk = Mockery::mock(Filesystem::class);

    $disk->shouldReceive('delete')->with([$location->storage_path])->once()->andReturnTrue();
    $registry = Mockery::mock(FilestoreRegistry::class);
    $registry->shouldReceive('disk')->with(Mockery::type(Filestore::class))->once()->andReturn($disk);
    app()->instance(FilestoreRegistry::class, $registry);

    (new DeleteTransfer($transfer->id))->handle();
    (new DeleteTransfer($transfer->id))->handle();

    expect(Transfer::query()->find($transfer->id))->toBeNull();
});

test('a partial ciphertext deletion leaves the transfer for retry and completes idempotently', function () {
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    $firstItem = TransferItem::factory()->for($transfer)->create(['position' => 0]);
    $secondItem = TransferItem::factory()->for($transfer)->create(['position' => 1]);
    $firstChunk = TransferChunk::factory()->for($firstItem, 'item')->create();
    $secondChunk = TransferChunk::factory()->for($secondItem, 'item')->create();
    $firstLocation = TransferChunkLocation::factory()->for($firstChunk, 'chunk')->create(['storage_path' => 'transfers/'.$transfer->id.'/first.bin']);
    $secondLocation = TransferChunkLocation::factory()->for($secondChunk, 'chunk')->create(['storage_path' => 'transfers/'.$transfer->id.'/second.bin', 'filestore_id' => $firstLocation->filestore_id]);
    Storage::disk('transfers')->put($firstLocation->storage_path, 'first ciphertext');
    Storage::disk('transfers')->put($secondLocation->storage_path, 'second ciphertext');
    $filesystem = app('filesystem');
    $realDisk = Storage::disk('transfers');
    $disk = Mockery::mock(Filesystem::class);

    $disk->shouldReceive('delete')->with([$firstLocation->storage_path, $secondLocation->storage_path])->once()->andReturnFalse();
    $registry = Mockery::mock(FilestoreRegistry::class);
    $registry->shouldReceive('disk')->with(Mockery::type(Filestore::class))->once()->andReturn($disk);
    app()->instance(FilestoreRegistry::class, $registry);

    expect(fn () => (new DeleteTransfer($transfer->id))->handle())->toThrow(RuntimeException::class);

    expect(Transfer::query()->find($transfer->id))->not->toBeNull();
    $realDisk->assertExists($firstLocation->storage_path);
    $realDisk->assertExists($secondLocation->storage_path);

    Storage::swap($filesystem);
    app()->forgetInstance(FilestoreRegistry::class);
    (new DeleteTransfer($transfer->id))->handle();

    expect(Transfer::query()->find($transfer->id))->toBeNull();
    $realDisk->assertMissing($secondLocation->storage_path);
});

test('ciphertext deletion runs outside the job database transaction', function () {
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    $item = TransferItem::factory()->for($transfer)->create();
    $chunk = TransferChunk::factory()->for($item, 'item')->create();
    $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create();
    $disk = Mockery::mock(Filesystem::class);
    $outerTransactionLevel = DB::transactionLevel();

    $disk->shouldReceive('delete')->with([$location->storage_path])->once()->andReturnUsing(function () use ($outerTransactionLevel): bool {
        expect(DB::transactionLevel())->toBe($outerTransactionLevel);

        return true;
    });
    $registry = Mockery::mock(FilestoreRegistry::class);
    $registry->shouldReceive('disk')->with(Mockery::type(Filestore::class))->once()->andReturn($disk);
    app()->instance(FilestoreRegistry::class, $registry);

    (new DeleteTransfer($transfer->id))->handle();
});

test('expired upload attempts are reaped after their lease even when the transfer is gone', function () {
    $attempt = TransferChunkUpload::factory()->create([
        'storage_path' => 'transfers/orphan/attempt.bin',
        'valid_until' => now()->subSecond(),
    ]);
    Transfer::query()->findOrFail($attempt->transfer_id)->delete();
    Storage::disk('transfers')->put($attempt->storage_path, 'late ciphertext');

    $this->artisan('filebeam:prune-transfers')->assertSuccessful();

    expect(TransferChunkUpload::query()->find($attempt->id))->toBeNull();
    Storage::disk('transfers')->assertMissing($attempt->storage_path);
});

test('the reaper skips an attempt recently claimed by another reaper', function () {
    $attempt = TransferChunkUpload::factory()->create([
        'valid_until' => now()->subSecond(),
        'is_reaping' => true,
        'cleanup_started_at' => now(),
    ]);

    Storage::shouldReceive('disk')->never();
    $this->artisan('filebeam:prune-transfers')->assertSuccessful();

    expect($attempt->refresh()->is_reaping)->toBeTrue();
});

test('the reaper recovers a claim left behind by a crashed cleanup process', function () {
    $attempt = TransferChunkUpload::factory()->create([
        'valid_until' => now()->subHour(),
        'is_reaping' => true,
        'cleanup_started_at' => now()->subMinutes(16),
    ]);
    Storage::disk('transfers')->put($attempt->storage_path, 'orphan ciphertext');

    $this->artisan('filebeam:prune-transfers')->assertSuccessful();

    expect(TransferChunkUpload::query()->find($attempt->id))->toBeNull();
    Storage::disk('transfers')->assertMissing($attempt->storage_path);
});

test('a failed attempt cleanup keeps its durable record for another pass', function () {
    $attempt = TransferChunkUpload::factory()->create(['valid_until' => now()->subSecond()]);
    $disk = Mockery::mock(Filesystem::class);
    $disk->shouldReceive('delete')->with($attempt->storage_path)->once()->andReturnFalse();
    $disk->shouldReceive('delete')->with($attempt->storage_path)->once()->andReturnTrue();
    $registry = Mockery::mock(FilestoreRegistry::class);
    $registry->shouldReceive('disk')->with(Mockery::type(Filestore::class))->twice()->andReturn($disk);
    app()->instance(FilestoreRegistry::class, $registry);

    $this->artisan('filebeam:prune-transfers')->assertSuccessful();
    expect($attempt->refresh()->is_reaping)->toBeFalse();
    $this->artisan('filebeam:prune-transfers')->assertSuccessful();
    expect(TransferChunkUpload::query()->find($attempt->id))->toBeNull();
});

test('the pruning command retries stale deleting transfers', function () {
    Queue::fake();
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    Transfer::query()->whereKey($transfer)->update(['updated_at' => now()->subMinutes(16)]);

    $this->artisan('filebeam:prune-transfers')->assertSuccessful();

    expect($transfer->refresh()->updated_at)->toBeGreaterThan(now()->subMinute());
    Queue::assertPushed(DeleteTransfer::class, fn (DeleteTransfer $job): bool => $job->transferId === $transfer->id);

    $this->artisan('filebeam:prune-transfers')->assertSuccessful();

    Queue::assertPushed(DeleteTransfer::class, 1);
});

test('the pruning command queues only transfers that remain eligible', function () {
    Queue::fake();
    config()->set('filebeam.transfers.incomplete_expiry_hours', 2);

    $expiredAvailable = Transfer::factory()->create([
        'status' => TransferStatus::Available,
        'expires_at' => now()->subMinute(),
    ]);
    $abandonedPending = Transfer::factory()->create([
        'status' => TransferStatus::Pending,
        'expires_at' => now()->subMinute(),
    ]);
    $freshAvailable = Transfer::factory()->create([
        'status' => TransferStatus::Available,
        'expires_at' => now()->addMinute(),
    ]);
    $freshPending = Transfer::factory()->create(['status' => TransferStatus::Pending]);
    $freshDeleting = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    $staleDeleting = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    Transfer::query()->whereKey($staleDeleting)->update(['updated_at' => now()->subMinutes(16)]);

    $this->artisan('filebeam:prune-transfers')->assertSuccessful();

    expect(Queue::pushed(DeleteTransfer::class)->map(fn (DeleteTransfer $job): string => $job->transferId)->all())
        ->toEqualCanonicalizing([$expiredAvailable->id, $abandonedPending->id, $staleDeleting->id]);
    expect($expiredAvailable->refresh()->status)->toBe(TransferStatus::Deleting);
    expect($abandonedPending->refresh()->status)->toBe(TransferStatus::Deleting);
    expect($freshAvailable->refresh()->status)->toBe(TransferStatus::Available);
    expect($freshPending->refresh()->status)->toBe(TransferStatus::Pending);
    expect($freshDeleting->refresh()->status)->toBe(TransferStatus::Deleting);
});

test('the pruning command does not delete a transfer completed after candidate selection', function () {
    Queue::fake();
    config()->set('filebeam.transfers.incomplete_expiry_hours', 2);

    $transfer = Transfer::factory()->create([
        'status' => TransferStatus::Pending,
        'expires_at' => now()->subMinute(),
    ]);

    $interleaved = false;
    $connection = DB::connection();
    $eventDispatcher = $connection->getEventDispatcher();
    $connection->setEventDispatcher(clone $eventDispatcher);

    DB::listen(function (QueryExecuted $query) use (&$interleaved, $transfer): void {
        $sql = str_replace(['"', '`'], '', strtolower($query->sql));

        if ($interleaved || ! str_starts_with($sql, 'select id from transfers')) {
            return;
        }

        $interleaved = true;
        Transfer::query()->whereKey($transfer)->update([
            'status' => TransferStatus::Available->value,
            'completed_at' => now(),
            'expires_at' => now()->addDay(),
        ]);
    });

    try {
        $this->artisan('filebeam:prune-transfers')->assertSuccessful();
    } finally {
        $connection->setEventDispatcher($eventDispatcher);
    }

    expect($interleaved)->toBeTrue();
    expect($transfer->refresh()->status)->toBe(TransferStatus::Available);
    Queue::assertNothingPushed();
});
