<?php

declare(strict_types=1);

use App\Models\Filestore;
use App\Models\Plan;
use App\Models\User;
use Database\Seeders\DatabaseSeeder;
use Illuminate\Support\Facades\Hash;

test('local seeding creates a verified admin without resetting it on subsequent runs', function (): void {
    $this->app->detectEnvironment(fn (): string => 'local');
    $this->seed(DatabaseSeeder::class);

    $user = User::query()->where('email', 'admin@filebeam.test')->sole();
    expect($user->isAdmin())->toBeTrue()
        ->and($user->username)->toBe('local_admin')
        ->and($user->normalized_username)->toBe('local_admin')
        ->and(Hash::check('password', $user->password))->toBeTrue();

    $user->update(['password' => 'changed-password']);
    $user->saveAppAuthenticationSecret('JBSWY3DPEHPK3PXP');
    $this->seed(DatabaseSeeder::class);

    expect(User::query()->count())->toBe(1)
        ->and(Plan::query()->where('slug', 'default')->sole()->name)->toBe('Default')
        ->and(Hash::check('changed-password', $user->refresh()->password))->toBeTrue()
        ->and($user->getAppAuthenticationSecret())->toBe('JBSWY3DPEHPK3PXP');
});

test('non-local seeding does not create an admin', function (string $environment): void {
    $this->app->detectEnvironment(fn (): string => $environment);
    $this->artisan('db:seed', ['--class' => DatabaseSeeder::class, '--force' => true])->assertSuccessful();

    expect(User::query()->count())->toBe(0);
})->with(['testing', 'staging', 'production']);

test('seeding registers environment-managed filestores and assigns the default plan', function (): void {
    config()->set('filebeam.filesystems.environment', ['transfers']);

    $this->seed(DatabaseSeeder::class);

    $plan = Plan::query()->where('slug', 'default')->sole();
    $store = Filestore::query()->where('disk_name', 'transfers')->sole();

    expect($store->source)->toBe('environment');
    $this->assertDatabaseHas('plan_filestore', [
        'plan_id' => $plan->id,
        'filestore_id' => $store->id,
        'is_default' => true,
    ]);
});
