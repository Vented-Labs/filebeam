<?php

declare(strict_types=1);

use App\Enums\TransferStatus;
use App\Jobs\DeleteTransfer;
use App\Models\Transfer;
use App\Models\TransferChunk;
use App\Models\TransferChunkLocation;
use App\Models\TransferItem;
use App\Support\FilestoreRegistry;
use App\Support\Installation\InstallationState;
use Illuminate\Console\Scheduling\Event;
use Illuminate\Console\Scheduling\Schedule;
use Illuminate\Contracts\Filesystem\Filesystem;
use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Exceptions;
use Illuminate\Support\Facades\Queue;
use Illuminate\Support\Facades\Storage;

test('cron registers short database batches with an overlap lock', function (): void {
    $event = collect(app(Schedule::class)->events())->first(fn (Event $event): bool => $event->description === 'filebeam:process-background-work');

    expect($event)->not->toBeNull();
    expect($event->expression)->toBe('* * * * *');
    expect($event->command)->toContain('queue:work', 'database', '--stop-when-empty', '--max-time=50', '--max-jobs=25', '--timeout=60');
    expect($event->withoutOverlapping)->toBeTrue();
    expect($event->expiresAt)->toBe(10);
    expect(config('queue.connections.database.retry_after'))->toBeGreaterThan(60);
});

test('cron maintains SQLite nightly without overlapping', function (): void {
    $default = DB::getDefaultConnection();
    $connection = 'sqlite_maintenance';
    // VACUUM must run outside RefreshDatabase's transaction, on an isolated SQLite connection.
    config()->set('database.connections.'.$connection, array_replace(config('database.connections.sqlite'), ['url' => null, 'database' => ':memory:']));
    DB::setDefaultConnection($connection);

    try {
        $database = DB::connection($connection);
        $database->enableQueryLog();
        $event = collect(app(Schedule::class)->events())->first(fn (Event $event): bool => $event->description === 'filebeam:maintain-sqlite');

        expect($event)->not->toBeNull();
        expect($event->expression)->toBe('0 0 * * *');
        expect($event->withoutOverlapping)->toBeTrue();
        expect($event->filtersPass(app()))->toBeTrue();

        $event->run(app());

        expect(array_column($database->getQueryLog(), 'query'))->toBe(['PRAGMA optimize', 'VACUUM']);
    } finally {
        DB::setDefaultConnection($default);
        DB::purge($connection);
        config()->offsetUnset('database.connections.'.$connection);
    }
});

test('SQLite maintenance cron skips non-SQLite connections', function (string $driver): void {
    $default = DB::getDefaultConnection();
    $connection = 'non_sqlite_maintenance';
    config()->set('database.connections.'.$connection, array_replace(config('database.connections.'.$driver), ['url' => null]));
    DB::setDefaultConnection($connection);

    try {
        $event = collect(app(Schedule::class)->events())->first(fn (Event $event): bool => $event->description === 'filebeam:maintain-sqlite');
        expect($event)->not->toBeNull();
        expect($event->filtersPass(app()))->toBeFalse();
    } finally {
        DB::setDefaultConnection($default);
        DB::purge($connection);
        config()->offsetUnset('database.connections.'.$connection);
    }
})->with(['mysql', 'pgsql']);

test('cron batches run only for an installed database-queue deployment that opts in', function (bool $enabled, string $connection, bool $pending, bool $runs): void {
    config()->set('queue.cron_enabled', $enabled);
    config()->set('queue.default', $connection);
    $this->mock(InstallationState::class)->shouldReceive('requiresSetup')->andReturn($pending);
    $event = collect(app(Schedule::class)->events())->first(fn (Event $event): bool => $event->description === 'filebeam:process-background-work');

    expect($event->filtersPass(app()))->toBe($runs);
    if ($runs) {
        expect($event->mutex->create($event))->toBeTrue();
        try {
            expect($event->filtersPass(app()))->toBeFalse();
        } finally {
            $event->mutex->forget($event);
        }
    }
})->with([
    [true, 'database', false, true],
    [false, 'database', false, false],
    [true, 'redis', false, false],
    [true, 'sync', false, false],
    [true, 'database', true, false],
]);

test('a short database batch performs queued ciphertext deletion and exits when empty', function (): void {
    Storage::fake('transfers');
    config()->set('queue.default', 'database');
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    $item = TransferItem::factory()->for($transfer)->create();
    $chunk = TransferChunk::factory()->for($item, 'item')->create();
    $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create();
    Storage::disk('transfers')->put($location->storage_path, 'ciphertext');
    Queue::connection('database')->push(new DeleteTransfer($transfer->id));
    expect(DB::table('jobs')->count())->toBe(1);

    expect(Artisan::call('queue:work', ['connection' => 'database', '--stop-when-empty' => true, '--max-time' => 50, '--max-jobs' => 25, '--sleep' => 0, '--timeout' => 60, '--tries' => 5]))->toBe(0);

    expect(DB::table('jobs')->count())->toBe(0);
    expect(Transfer::query()->find($transfer->id))->toBeNull();
    Storage::disk('transfers')->assertMissing($location->storage_path);
});

test('a database batch leaves excess work for the next cron invocation', function (): void {
    config()->set('queue.default', 'database');
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    for ($index = 0; $index < 26; $index++) {
        Queue::connection('database')->push(new DeleteTransfer($transfer->id));
    }

    expect(Artisan::call('queue:work', ['connection' => 'database', '--stop-when-empty' => true, '--max-time' => 50, '--max-jobs' => 25, '--sleep' => 0, '--timeout' => 60, '--tries' => 5]))->toBe(0);

    expect(DB::table('jobs')->count())->toBe(1);
    expect(DB::table('jobs')->value('reserved_at'))->toBeNull();
});

test('failed cleanup remains queued for a later cron batch and can recover', function (): void {
    $this->freezeTime();
    Exceptions::fake();
    config()->set('queue.default', 'database');
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    $item = TransferItem::factory()->for($transfer)->create();
    $chunk = TransferChunk::factory()->for($item, 'item')->create();
    $location = TransferChunkLocation::factory()->for($chunk, 'chunk')->create();
    $disk = Mockery::mock(Filesystem::class);
    $disk->shouldReceive('delete')->with([$location->storage_path])->twice()->andReturn(false, true);
    $this->mock(FilestoreRegistry::class)->shouldReceive('disk')->andReturn($disk);
    Queue::connection('database')->push(new DeleteTransfer($transfer->id));
    $options = ['connection' => 'database', '--stop-when-empty' => true, '--max-time' => 50, '--max-jobs' => 25, '--sleep' => 0, '--timeout' => 60, '--tries' => 5];

    expect(Artisan::call('queue:work', $options))->toBe(0);

    expect(DB::table('jobs')->count())->toBe(1);
    expect(DB::table('jobs')->value('attempts'))->toBe(1);
    expect(DB::table('jobs')->value('available_at'))->toBe(now()->addSeconds(30)->timestamp);
    expect(Transfer::query()->find($transfer->id))->not->toBeNull();
    Exceptions::assertReported(RuntimeException::class);

    $this->travel(31)->seconds();
    expect(Artisan::call('queue:work', $options))->toBe(0);

    expect(DB::table('jobs')->count())->toBe(0);
    expect(Transfer::query()->find($transfer->id))->toBeNull();
});
