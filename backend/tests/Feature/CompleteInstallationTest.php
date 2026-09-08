<?php

declare(strict_types=1);

use App\Models\Filestore;
use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\User;
use App\Support\Installation\CompleteInstallation;
use App\Support\Installation\EnvironmentWriter;
use App\Support\Installation\InstallationConfiguration;
use App\Support\Installation\InstallationState;
use App\Support\Installation\OptimizeInstallation;
use Dotenv\Dotenv;
use Illuminate\Database\Events\QueryExecuted;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Str;
use Illuminate\Validation\ValidationException;

use function Pest\Laravel\mock;

beforeEach(function (): void {
    $this->originalCompleteDatabase = config('database.default');
    mock(OptimizeInstallation::class)->shouldReceive('handle')->andReturnNull();
});

afterEach(function (): void {
    DB::setDefaultConnection($this->originalCompleteDatabase);
    DB::purge('installation');
    File::deleteDirectory($this->completeDirectory);
});

test('completes an isolated installation and persists a reloadable environment', function (bool $interruptClaim): void {
    $directory = storage_path('framework/testing/complete-installation-'.bin2hex(random_bytes(6)));
    $this->completeDirectory = $directory;
    File::ensureDirectoryExists($directory);
    File::ensureDirectoryExists($directory.'/database');
    config()->set('installation.state_directory', $directory.'/state');
    config()->set('installation.environment_path', $directory.'/.env');
    config()->set('installation.sqlite_directory', $directory.'/database');
    config()->set('filebeam.filesystems.local_root', $directory.'/files');
    config()->set('cache.stores.file.path', $directory.'/cache/data');
    config()->set('cache.stores.file.lock_path', $directory.'/cache/locks');
    config()->set('app.key', null);

    $state = app(InstallationState::class);
    $state->write(['id' => (string) Str::uuid(), 'status' => 'pending', 'token_hash' => hash('sha256', 'temporary-token')]);
    app(EnvironmentWriter::class)->write(['APP_ENV' => 'production', 'APP_DEBUG' => 'false', 'APP_KEY' => 'base64:'.base64_encode(random_bytes(32)), 'APP_NAME' => 'Filebeam', 'APP_URL' => 'http://localhost', 'SESSION_DRIVER' => 'cookie', 'CACHE_STORE' => 'file', 'QUEUE_CONNECTION' => 'database', 'FILEBEAM_INSTALL_TOKEN' => 'temporary-token'], true);
    $bootstrap = Dotenv::createArrayBacked($directory, '.env')->load();
    config()->set('app.key', $bootstrap['APP_KEY']);
    app()->instance('installation.external_environment', []);
    $input = completeInstallationInput($directory.'/database/filebeam.sqlite');
    $validated = app(InstallationConfiguration::class)->validate($input);

    if ($interruptClaim) {
        DB::listen(function (QueryExecuted $query) use (&$interruptClaim): void {
            if ($interruptClaim && str_contains($query->sql, 'create table "installation_records"')) {
                $interruptClaim = false;
                throw new RuntimeException('Simulated process interruption after claiming DDL.');
            }
        });
        expect(fn () => app(CompleteInstallation::class)->handle($validated))->toThrow(RuntimeException::class);
        expect($state->read()['checkpoint'])->toBe('claiming');
    }

    app(CompleteInstallation::class)->handle($validated);

    $admin = User::query()->where('email', 'admin@example.test')->sole();
    expect($admin->isAdmin())->toBeTrue()->and($admin->hasVerifiedEmail())->toBeTrue()->and(Hash::check('correct horse battery staple', $admin->password))->toBeTrue();
    expect(InstanceSetting::query()->pluck('value', 'key')->all())->toMatchArray(['registration' => false, 'anonymous_uploads' => false]);
    expect(Plan::query()->where('slug', 'default')->sole()->placement_mode)->toBe('replicate');
    expect(Filestore::query()->sole()->configuration)->toBe(['root' => 'primary', 'visibility' => 'private']);
    expect($state->isCompleted())->toBeTrue()->and($state->read()['token_hash'])->toBeNull();
    $environment = Dotenv::createArrayBacked($directory, '.env')->load();
    expect($environment)->toMatchArray(['APP_KEY' => $bootstrap['APP_KEY'], 'APP_NAME' => 'Private Filebeam', 'FILEBEAM_NAME' => 'Private Filebeam', 'APP_URL' => 'http://localhost', 'DB_CONNECTION' => 'sqlite', 'DB_DATABASE' => $directory.'/database/filebeam.sqlite', 'CHUNK_MAX_SIZE' => '17']);
    expect($environment)->not->toHaveKey('FILEBEAM_INSTALL_TOKEN');

    $state->write(['id' => $state->read()['id'], 'status' => 'installing', 'token_hash' => hash('sha256', 'stale-token')]);
    app(CompleteInstallation::class)->handle($validated);
    expect($state->isCompleted())->toBeTrue();

    $state->write(['id' => $state->read()['id'], 'status' => 'installing', 'token_hash' => hash('sha256', 'retry-token')]);
    app(EnvironmentWriter::class)->write(['FILEBEAM_INSTALL_TOKEN' => 'retry-token']);
    $completedAt = DB::connection('installation')->table('installation_records')->value('completed_at');
    mock(OptimizeInstallation::class)->shouldReceive('handle')->once()->andThrow(ValidationException::withMessages(['installation' => 'Application optimization could not be completed. Please retry.']));

    expect(fn (): mixed => app(CompleteInstallation::class)->handle($validated))->toThrow(ValidationException::class);

    expect($state->read()['status'])->toBe('installing');
    expect($state->read()['token_hash'])->toBe(hash('sha256', 'retry-token'));
    expect(Dotenv::createArrayBacked($directory, '.env')->load()['FILEBEAM_INSTALL_TOKEN'])->toBe('retry-token');
    expect(DB::connection('installation')->table('installation_records')->value('completed_at'))->toBe($completedAt);

    $state->write(['id' => (string) Str::uuid(), 'status' => 'pending', 'token_hash' => hash('sha256', 'other-token')]);
    expect(fn (): mixed => app(CompleteInstallation::class)->handle($validated))->toThrow(ValidationException::class);

})->with([false, true]);

/** @return array<string, mixed> */
function completeInstallationInput(string $database): array
{
    return ['database' => ['driver' => 'sqlite', 'database' => $database], 'instance' => ['name' => 'Private Filebeam', 'url' => 'http://localhost', 'username_domain' => '', 'visibility' => 'private'], 'storage' => [['name' => 'Primary', 'driver' => 'local', 'root' => 'primary', 'use_path_style_endpoint' => false]], 'placement_mode' => 'replicate', 'admin' => ['name' => 'Administrator', 'username' => 'admin_user', 'email' => 'admin@example.test', 'password' => 'correct horse battery staple', 'password_confirmation' => 'correct horse battery staple', 'email_ownership_confirmed' => true], 'chunk_max_size' => 17];
}
