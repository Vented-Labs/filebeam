<?php

declare(strict_types=1);

use App\Models\Filestore;
use App\Models\Plan;
use App\Support\FilestoreRegistry;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Storage;
use Illuminate\Validation\ValidationException;

test('resolves an assigned Laravel disk from the configured environment list', function (): void {
    Storage::fake('transfers');
    config()->set('filebeam.filesystems.environment', ['transfers']);
    $store = Filestore::factory()->create();

    app(FilestoreRegistry::class)->disk($store)->put('chunk.bin', 'ciphertext');

    Storage::disk('transfers')->assertExists('chunk.bin');
});

test('rejects an explicitly empty environment disk list', function (): void {
    config()->set('filebeam.filesystems.environment', []);

    app(FilestoreRegistry::class)->environmentDisks();
})->throws(InvalidArgumentException::class);

test('never treats a database filestore as environment-managed placement', function (): void {
    config()->set('filebeam.filesystems.environment', ['transfers']);
    $store = Filestore::factory()->create(['source' => 'database', 'disk_name' => 'transfers']);

    expect(app(FilestoreRegistry::class)->placementEnabled($store))->toBeFalse();
});

test('environment filestore sync preserves existing plan defaults', function (): void {
    config()->set('filebeam.filesystems.environment', ['transfers', 'secondary']);
    Storage::fake('secondary');
    $first = Filestore::factory()->create(['source' => 'environment', 'disk_name' => 'transfers']);
    $second = Filestore::factory()->create(['source' => 'environment', 'disk_name' => 'secondary']);
    $plan = Plan::factory()->create();
    DB::table('plan_filestore')->insert([
        ['plan_id' => $plan->id, 'filestore_id' => $first->id, 'is_default' => false],
        ['plan_id' => $plan->id, 'filestore_id' => $second->id, 'is_default' => true],
    ]);

    app(FilestoreSeeder::class)->run();

    expect((int) $plan->filestores()->wherePivot('is_default', true)->value('filestores.id'))->toBe($second->id);
});

test('environment sync promotes Laravel references and preserves removed disks for historical access', function (): void {
    config()->set('filebeam.filesystems.environment', null);
    $plan = Plan::factory()->create();
    app(FilestoreSeeder::class)->run();
    $transfers = Filestore::query()->where('disk_name', 'transfers')->firstOrFail();
    $id = $transfers->id;
    Storage::fake('secondary');

    config()->set('filebeam.filesystems.environment', ['transfers', 'secondary']);
    app(FilestoreSeeder::class)->run();

    expect($transfers->refresh()->id)->toBe($id)
        ->and($transfers->source)->toBe('environment')
        ->and($plan->filestores()->pluck('filestores.id')->all())->toBe([$id]);

    config()->set('filebeam.filesystems.environment', ['secondary']);
    app(FilestoreSeeder::class)->run();
    expect(app(FilestoreRegistry::class)->placementEnabled($transfers->refresh()))->toBeFalse();

    config()->set('filebeam.filesystems.environment', null);
    app(FilestoreRegistry::class)->disk($transfers->refresh())->put('historical.bin', 'ciphertext');
    app(FilestoreRegistry::class)->disk($transfers)->delete('historical.bin');

    expect($transfers->source)->toBe('environment')
        ->and(Filestore::query()->whereKey($id)->exists())->toBeTrue()
        ->and(app(FilestoreRegistry::class)->placementEnabled($transfers))->toBeTrue();
});

test('environment sync validates every disk before modifying filestores or plans', function (): void {
    config()->set('filebeam.filesystems.environment', null);
    $plan = Plan::factory()->create();
    $store = Filestore::factory()->create();
    DB::table('plan_filestore')->insert(['plan_id' => $plan->id, 'filestore_id' => $store->id, 'is_default' => true]);
    config()->set('filebeam.filesystems.environment', ['transfers', 'missing']);

    expect(fn (): mixed => app(FilestoreSeeder::class)->run())->toThrow(InvalidArgumentException::class);

    expect($store->refresh()->source)->toBe('laravel')
        ->and($plan->filestores()->pluck('filestores.id')->all())->toBe([$store->id])
        ->and(Filestore::query()->count())->toBe(1);
});

test('environment sync rejects database filestore name collisions without modifying credentials', function (): void {
    config()->set('filebeam.filesystems.environment', ['transfers']);
    $store = Filestore::factory()->create([
        'source' => 'database',
        'disk_name' => 'transfers',
        'driver' => 's3',
        'configuration' => ['key' => 'access-key', 'secret' => 'secret-value', 'region' => 'us-east-1', 'bucket' => 'private'],
    ]);

    expect(fn (): mixed => app(FilestoreSeeder::class)->run())->toThrow(InvalidArgumentException::class);

    expect($store->refresh()->source)->toBe('database')
        ->and($store->configuration['secret'])->toBe('secret-value');
});

test('environment sync assigns every configured disk as an initial default', function (): void {
    config()->set('filebeam.filesystems.environment', ['transfers', 'secondary']);
    Storage::fake('secondary');
    $plan = Plan::factory()->create();

    app(FilestoreSeeder::class)->run();

    expect($plan->filestores()->wherePivot('is_default', true)->pluck('filestores.disk_name')->all())
        ->toEqualCanonicalizing(['transfers', 'secondary']);
});

test('requires configured Laravel disks to be private', function (): void {
    config()->set('filesystems.disks.public-transfers', [
        'driver' => 'local',
        'root' => storage_path('app/public-transfers'),
        'visibility' => 'public',
    ]);
    $store = Filestore::factory()->create(['disk_name' => 'public-transfers']);

    app(FilestoreRegistry::class)->disk($store);
})->throws(InvalidArgumentException::class);

test('allows configured private Flysystem disks with installed adapters', function (): void {
    Storage::fake('custom-transfers');
    config()->set('filesystems.disks.custom-transfers', ['driver' => 'custom', 'visibility' => 'private']);
    $store = Filestore::factory()->create(['disk_name' => 'custom-transfers']);

    app(FilestoreRegistry::class)->disk($store)->put('chunk.bin', 'ciphertext');

    Storage::disk('custom-transfers')->assertExists('chunk.bin');
});

test('resolves a Laravel storage fake without a configured disk', function (): void {
    Storage::fake('ephemeral-transfers');
    $store = Filestore::factory()->create(['disk_name' => 'ephemeral-transfers']);

    app(FilestoreRegistry::class)->disk($store)->put('chunk.bin', 'ciphertext');

    Storage::disk('ephemeral-transfers')->assertExists('chunk.bin');
});

test('selects enabled default filestores assigned to a plan', function (): void {
    config()->set('filebeam.filesystems.environment', null);
    $plan = Plan::factory()->create();
    $defaultStore = Filestore::factory()->create();
    $secondaryStore = Filestore::factory()->create(['disk_name' => 'secondary', 'name' => 'Secondary']);
    DB::table('plan_filestore')->insert([
        ['plan_id' => $plan->id, 'filestore_id' => $defaultStore->id, 'is_default' => true],
        ['plan_id' => $plan->id, 'filestore_id' => $secondaryStore->id, 'is_default' => false],
    ]);

    $selected = app(FilestoreRegistry::class)->selectForPlan($plan, null);

    expect($selected)->toBe([$defaultStore->id]);
});

test('rejects disabled or unassigned requested filestores', function (): void {
    config()->set('filebeam.filesystems.environment', null);
    $plan = Plan::factory()->create();
    $disabledStore = Filestore::factory()->create(['placement_enabled' => false]);
    DB::table('plan_filestore')->insert([
        'plan_id' => $plan->id,
        'filestore_id' => $disabledStore->id,
        'is_default' => true,
    ]);

    app(FilestoreRegistry::class)->selectForPlan($plan, [$disabledStore->id]);
})->throws(ValidationException::class);

test('refuses public database-backed filestores', function (): void {
    $store = Filestore::factory()->create([
        'source' => 'database',
        'disk_name' => null,
        'driver' => 'local',
        'configuration' => ['root' => 'private', 'visibility' => 'public'],
    ]);

    app(FilestoreRegistry::class)->disk($store);
})->throws(InvalidArgumentException::class);

test('refuses local roots that traverse outside the configured root at resolution time', function (): void {
    config()->set('filebeam.filesystems.local_root', storage_path('app'));
    $store = Filestore::factory()->create([
        'source' => 'database',
        'disk_name' => null,
        'driver' => 'local',
        'configuration' => ['root' => '../outside'],
    ]);

    app(FilestoreRegistry::class)->disk($store);
})->throws(InvalidArgumentException::class);

test('refuses local roots that escape through a symlink at resolution time', function (): void {
    $base = storage_path('framework/testing/filestore-root');
    $outside = storage_path('framework/testing/filestore-outside');
    File::deleteDirectory($base);
    File::deleteDirectory($outside);
    File::ensureDirectoryExists($base);
    File::ensureDirectoryExists($outside);
    symlink($outside, $base.'/escape');
    config()->set('filebeam.filesystems.local_root', $base);
    $store = Filestore::factory()->create([
        'source' => 'database',
        'disk_name' => null,
        'driver' => 'local',
        'configuration' => ['root' => 'escape'],
    ]);

    try {
        app(FilestoreRegistry::class)->disk($store);
    } finally {
        File::deleteDirectory($base);
        File::deleteDirectory($outside);
    }
})->throws(InvalidArgumentException::class);

test('creates a configured database local root when it does not exist', function (): void {
    $base = storage_path('framework/testing/new-filestore-root');
    File::deleteDirectory($base);
    config()->set('filebeam.filesystems.local_root', $base);
    $store = Filestore::factory()->create([
        'source' => 'database',
        'disk_name' => null,
        'driver' => 'local',
        'configuration' => ['root' => 'private'],
    ]);

    try {
        app(FilestoreRegistry::class)->disk($store)->put('chunk.bin', 'ciphertext');

        expect(File::isDirectory($base.'/private'))->toBeTrue();
    } finally {
        File::deleteDirectory($base);
    }
});

test('normal-mode seeding initializes a missing Laravel transfers root', function (): void {
    $root = storage_path('framework/testing/seeded-transfers-root');
    File::deleteDirectory($root);
    config()->set('filebeam.filesystems.environment', null);
    config()->set('filesystems.disks.transfers.root', $root);

    try {
        app(FilestoreSeeder::class)->run();

        expect(File::isDirectory($root))->toBeTrue();
    } finally {
        File::deleteDirectory($root);
    }
});
