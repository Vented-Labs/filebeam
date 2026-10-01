<?php

declare(strict_types=1);

use App\Support\Installation\InstallationState;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Str;

beforeEach(function (): void {
    $this->bootstrapDirectory = storage_path('framework/testing/bootstrap-'.Str::uuid());
    File::ensureDirectoryExists($this->bootstrapDirectory.'/database');
    config()->set([
        'installation.container' => true,
        'installation.container_variant' => 'light',
        'installation.state_directory' => $this->bootstrapDirectory.'/app/installation',
        'installation.environment_path' => $this->bootstrapDirectory.'/config/.env',
        'installation.sqlite_directory' => $this->bootstrapDirectory.'/database',
    ]);
    app()->instance('installation.external_environment', ['FILEBEAM_CONTAINER' => 'true', 'FILEBEAM_VARIANT' => 'light']);
});

afterEach(function (): void {
    File::deleteDirectory($this->bootstrapDirectory);
});

test('local bootstrap creates pending setup without printing its credentials', function (): void {
    $this->artisan('filebeam:installation:bootstrap')
        ->expectsOutput('Web setup initialized. The container supervisor logs the installation token while setup is pending.')
        ->assertSuccessful();

    $state = app(InstallationState::class);
    $environment = Dotenv\Dotenv::createArrayBacked($this->bootstrapDirectory.'/config')->load();
    expect($state->isPending())->toBeTrue();
    expect($environment['APP_KEY'])->toStartWith('base64:');
    expect($environment['FILEBEAM_INSTALL_TOKEN'])->toHaveLength(64);
    expect($state->read()['token_hash'])->toBe(hash('sha256', $environment['FILEBEAM_INSTALL_TOKEN']));
    expect(fileperms($state->environmentPath()) & 0777)->toBe(0600);
});

test('repeated local bootstrap preserves pending configuration and generation', function (): void {
    $this->artisan('filebeam:installation:bootstrap')->assertSuccessful();
    $state = app(InstallationState::class);
    $environment = File::get($state->environmentPath());
    $marker = File::get($state->directory().'/state.json');
    $generation = File::get($state->directory().'/runtime.generation');

    $this->artisan('filebeam:installation:bootstrap')->assertSuccessful();

    expect(File::get($state->environmentPath()))->toBe($environment);
    expect(File::get($state->directory().'/state.json'))->toBe($marker);
    expect(File::get($state->directory().'/runtime.generation'))->toBe($generation);
});

test('local bootstrap does not recreate configuration for a completed installation', function (): void {
    $state = app(InstallationState::class);
    $state->write(['id' => (string) Str::uuid(), 'status' => 'completed']);

    $this->artisan('filebeam:installation:bootstrap')->assertSuccessful();

    expect($state->isCompleted())->toBeTrue();
    expect(file_exists($state->environmentPath()))->toBeFalse();
});

test('local bootstrap leaves existing deployments without an installation marker intact', function (): void {
    $path = app(InstallationState::class)->environmentPath();
    File::ensureDirectoryExists(dirname($path));
    File::put($path, "APP_KEY=existing-key\n");

    $this->artisan('filebeam:installation:bootstrap')->assertSuccessful();

    expect(File::get($path))->toBe("APP_KEY=existing-key\n");
    expect(app(InstallationState::class)->read())->toBeNull();
});

test('local bootstrap refuses SQLite data without matching configuration', function (bool $pending): void {
    $state = app(InstallationState::class);
    if ($pending) {
        $state->write(['id' => (string) Str::uuid(), 'status' => 'pending']);
    }
    File::put($this->bootstrapDirectory.'/database/database.sqlite', 'existing database');

    $this->artisan('filebeam:installation:bootstrap')->assertFailed();

    expect(file_exists($state->environmentPath()))->toBeFalse();
    expect(File::get($this->bootstrapDirectory.'/database/database.sqlite'))->toBe('existing database');
})->with([false, true]);

test('local bootstrap refuses an unreadable installation marker', function (): void {
    $state = app(InstallationState::class);
    File::ensureDirectoryExists($state->directory());
    File::put($state->directory().'/state.json', '{broken');

    $this->artisan('filebeam:installation:bootstrap')->assertFailed();

    expect(File::get($state->directory().'/state.json'))->toBe('{broken');
    expect(file_exists($state->environmentPath()))->toBeFalse();
});
