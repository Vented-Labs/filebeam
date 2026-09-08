<?php

declare(strict_types=1);

use App\Support\Installation\InstallationState;
use Filebeam\Updater\ActivityLock;
use Illuminate\Support\Facades\File;

beforeEach(function (): void {
    require_once base_path('../updater/ActivityLock.php');

    $this->updaterStatePath = storage_path('framework/testing/updater-probe-'.bin2hex(random_bytes(4)));
    $this->originalStoragePath = storage_path();
    $this->activityStoragePath = $this->updaterStatePath.'/storage';
    File::ensureDirectoryExists($this->updaterStatePath);
    File::ensureDirectoryExists($this->activityStoragePath);
    app()->useStoragePath($this->activityStoragePath);
    config()->set('filebeam.updates.state_path', $this->updaterStatePath);
    config()->set('version.distribution', 'package');
    $this->mock(InstallationState::class)->shouldReceive('requiresSetup')->andReturnFalse();
});

afterEach(function (): void {
    app()->useStoragePath($this->originalStoragePath);
    File::deleteDirectory($this->updaterStatePath);
    config()->set('version.distribution', 'source');
});

test('returns package readiness metadata for a valid updater probe token', function (): void {
    $token = str_repeat('a', 64);
    File::put($this->updaterStatePath.'/probe.json', json_encode(['token' => $token], JSON_THROW_ON_ERROR));

    $this->get('/updater/probe?token='.$token)
        ->assertOk()
        ->assertHeader('Cache-Control', 'no-store, private')
        ->assertExactJson([
            'token' => $token,
            'version' => config('version.version'),
            'built_at' => config('version.built_at'),
            'activity_protocol' => 1,
        ]);
});

test('returns 403 for an invalid updater probe token', function (): void {
    File::put($this->updaterStatePath.'/probe.json', json_encode(['token' => str_repeat('a', 64)], JSON_THROW_ON_ERROR));

    $this->get('/updater/probe?token='.str_repeat('b', 64))->assertForbidden();
});

test('excludes the updater probe but blocks ordinary requests while an update holds the activity lock', function (): void {
    $token = str_repeat('a', 64);
    File::put($this->updaterStatePath.'/probe.json', json_encode(['token' => $token], JSON_THROW_ON_ERROR));
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    $updater = $lock->acquireExclusive(0);

    try {
        $this->get('/updater/probe?token='.$token)
            ->assertOk()
            ->assertJsonPath('activity_protocol', 1);
        $this->get('/')->assertServiceUnavailable();
    } finally {
        $lock->release($updater);
    }
});

test('does not advertise updater activity support for source distributions', function (): void {
    $token = str_repeat('a', 64);
    File::put($this->updaterStatePath.'/probe.json', json_encode(['token' => $token], JSON_THROW_ON_ERROR));
    config()->set('version.distribution', 'source');

    $this->get('/updater/probe?token='.$token)
        ->assertOk()
        ->assertExactJson([
            'token' => $token,
            'version' => config('version.version'),
            'built_at' => config('version.built_at'),
        ]);
});
