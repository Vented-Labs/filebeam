<?php

declare(strict_types=1);

use App\Support\Installation\ContainerConfiguration;
use App\Support\Installation\EnvironmentSettings;
use App\Support\Installation\EnvironmentWriter;
use App\Support\Installation\InstallationState;
use Illuminate\Support\Facades\File;

beforeEach(function (): void {
    $this->containerInstallationDirectory = storage_path('framework/testing/container-installation-'.bin2hex(random_bytes(8)));
    File::ensureDirectoryExists($this->containerInstallationDirectory.'/database');
    config()->set([
        'installation.container' => true,
        'installation.container_variant' => 'light',
        'installation.state_directory' => $this->containerInstallationDirectory.'/app/installation',
        'installation.environment_path' => $this->containerInstallationDirectory.'/config/.env',
        'installation.sqlite_directory' => $this->containerInstallationDirectory.'/database',
    ]);
    app()->instance('installation.external_environment', ['FILEBEAM_CONTAINER' => 'true', 'FILEBEAM_VARIANT' => 'light']);
});

afterEach(function (): void {
    File::deleteDirectory($this->containerInstallationDirectory);
});

test('light containers use durable SQLite and file cache defaults', function (): void {
    $container = app(ContainerConfiguration::class);

    expect($container->managed())->toBe(['database' => false, 'cache' => false, 'instance' => false, 'auto_updates' => true]);
    expect($container->defaults()['database'])->toMatchArray(['driver' => 'sqlite', 'database' => $this->containerInstallationDirectory.'/database/database.sqlite']);
    expect($container->defaults()['cache']['driver'])->toBe('file');
});

test('omnibus connections are immutable and never return credentials to the browser', function (): void {
    config()->set('installation.container_variant', 'omnibus');
    app()->instance('installation.external_environment', ['FILEBEAM_CONTAINER' => 'true', 'FILEBEAM_VARIANT' => 'omnibus', 'DB_PASSWORD' => 'not-for-browser', 'REDIS_PASSWORD' => 'not-for-browser']);
    $container = app(ContainerConfiguration::class);

    expect($container->managed())->toBe(['database' => true, 'cache' => true, 'instance' => false, 'auto_updates' => true]);
    $defaults = $container->publicDefaults();
    expect($defaults['database']['driver'])->toBe('pgsql')->and($defaults['database']['host'])->toBe('/run/filebeam/postgresql')->and($defaults['database']['port'])->toBe(5432)->and($defaults['database']['database'])->toBe('filebeam')->and($defaults['database']['username'])->toBe('filebeam')->and($defaults['database']['password'])->toBe('');
    expect($defaults['cache']['driver'])->toBe('redis')->and($defaults['cache']['transport'])->toBe('unix')->and($defaults['cache']['host'])->toBe('/run/filebeam/valkey/valkey.sock')->and($defaults['cache']['port'])->toBe(0)->and($defaults['cache']['database'])->toBe(1)->and($defaults['cache']['password'])->toBe('');
    $applied = $container->apply(['database' => ['password' => 'tampered'], 'cache' => ['password' => 'tampered'], 'instance' => ['auto_updates_enabled' => true]]);
    expect($applied['database']['password'])->toBe('')->and($applied['cache']['password'])->toBe('not-for-browser')->and($applied['instance']['auto_updates_enabled'])->toBeFalse();
});

test('operator managed connections are applied server side without exposing passwords', function (): void {
    app()->instance('installation.external_environment', [
        'FILEBEAM_CONTAINER' => 'true', 'DB_CONNECTION' => 'pgsql', 'DB_HOST' => 'database.internal', 'DB_PORT' => '5433', 'DB_DATABASE' => 'filebeam', 'DB_USERNAME' => 'operator', 'DB_PASSWORD' => 'database-secret',
        'CACHE_STORE' => 'redis', 'REDIS_HOST' => 'cache.internal', 'REDIS_PORT' => '6380', 'REDIS_PASSWORD' => 'cache-secret',
    ]);
    $container = app(ContainerConfiguration::class);

    $defaults = $container->publicDefaults();
    expect($defaults['database']['host'])->toBe('database.internal')->and($defaults['database']['password'])->toBe('');
    expect($defaults['cache']['host'])->toBe('cache.internal')->and($defaults['cache']['password'])->toBe('');
    $applied = $container->apply(['database' => [], 'cache' => []]);
    expect($applied['database']['password'])->toBe('database-secret')->and($applied['cache']['password'])->toBe('cache-secret');
});

test('operator URL connections prefill managed settings without returning credentials', function (): void {
    app()->instance('installation.external_environment', [
        'FILEBEAM_CONTAINER' => 'true',
        'DB_URL' => 'postgresql://operator:database-secret@database.internal:5433/filebeam',
        'REDIS_URL' => 'rediss://cache-user:cache-secret@cache.internal:6380/2',
    ]);

    $container = app(ContainerConfiguration::class);
    $public = $container->publicDefaults();
    $applied = $container->apply(['database' => [], 'cache' => []]);

    expect($public['database'])->toMatchArray(['driver' => 'pgsql', 'host' => 'database.internal', 'port' => 5433, 'database' => 'filebeam', 'username' => 'operator', 'password' => '']);
    expect($public['cache'])->toMatchArray(['driver' => 'redis', 'transport' => 'tls', 'host' => 'cache.internal', 'port' => 6380, 'username' => 'cache-user', 'database' => 2, 'password' => '']);
    expect($applied['database']['password'])->toBe('database-secret')->and($applied['cache']['password'])->toBe('cache-secret');
});

test('operator instance identity is prefilled and cannot be replaced by installer input', function (): void {
    app()->instance('installation.external_environment', [
        'FILEBEAM_CONTAINER' => 'true',
        'APP_NAME' => 'Operator Filebeam',
        'APP_URL' => 'https://files.example.test',
        'FILEBEAM_USERNAME_DOMAIN' => 'example.test',
    ]);
    $container = app(ContainerConfiguration::class);

    $defaults = $container->publicDefaults();
    $applied = $container->apply(['instance' => ['name' => 'Tampered', 'url' => 'http://localhost', 'username_domain' => 'attacker.test']]);

    expect($container->managed()['instance'])->toBeTrue();
    expect($defaults['instance'])->toBe(['name' => 'Operator Filebeam', 'url' => 'https://files.example.test', 'username_domain' => 'example.test']);
    expect($applied['instance'])->toMatchArray($defaults['instance']);
});

test('operator remote database and cache transports are normalized without exposing credentials', function (string $databaseUrl, array $cacheEnvironment, string $driver, string $transport): void {
    app()->instance('installation.external_environment', [
        'FILEBEAM_CONTAINER' => 'true',
        'DB_URL' => $databaseUrl,
        ...$cacheEnvironment,
    ]);
    $container = app(ContainerConfiguration::class);

    $public = $container->publicDefaults();

    expect($public['database']['driver'])->toBe($driver)->and($public['database']['password'])->toBe('');
    expect($public['cache']['transport'])->toBe($transport)->and($public['cache']['password'])->toBe('');
})->with([
    ['postgresql://operator:secret@postgres.internal:5433/filebeam', ['REDIS_URL' => 'redis://cache:secret@redis.internal:6380/2'], 'pgsql', 'tcp'],
    ['mysql://operator:secret@mysql.internal:3307/filebeam', ['REDIS_URL' => 'rediss://cache:secret@redis.internal:6380/2'], 'mysql', 'tls'],
    ['mysql://operator:secret@mysql.internal/filebeam', ['CACHE_STORE' => 'redis', 'REDIS_HOST' => '/run/redis/redis.sock', 'REDIS_PASSWORD' => 'secret'], 'mysql', 'unix'],
]);

test('bootstrap writes an atomic runtime generation marker', function (): void {
    $state = app(InstallationState::class);
    $state->bootstrap(app(EnvironmentWriter::class));

    expect(File::get($state->directory().'/runtime.generation'))->toBe("1\n");
    $state->writeGeneration();
    expect(File::get($state->directory().'/runtime.generation'))->toBe("2\n");
});

test('container bootstrap persists a supplied application key and light runtime defaults', function (): void {
    app()->instance('installation.external_environment', ['FILEBEAM_CONTAINER' => 'true', 'FILEBEAM_VARIANT' => 'light', 'APP_KEY' => 'base64:container-key']);
    $state = app(InstallationState::class);

    $state->bootstrap(app(EnvironmentWriter::class));
    $environment = Dotenv\Dotenv::createArrayBacked($this->containerInstallationDirectory.'/config')->load();

    expect($environment)->toMatchArray(['APP_KEY' => 'base64:container-key', 'QUEUE_CONNECTION' => 'database', 'FILEBEAM_CRON_QUEUE_ENABLED' => 'false']);
});

test('container bootstrap persists an application key loaded from an allowlisted secret file', function (): void {
    $keyFile = $this->containerInstallationDirectory.'/app-key';
    File::put($keyFile, 'base64:file-secret-key');
    $previousKey = getenv('APP_KEY');
    $previousFile = getenv('APP_KEY_FILE');
    putenv('APP_KEY');
    putenv('APP_KEY_FILE='.$keyFile);

    try {
        EnvironmentSettings::resolveFileSecrets();
        app()->instance('installation.external_environment', ['FILEBEAM_CONTAINER' => 'true', 'FILEBEAM_VARIANT' => 'light', 'APP_KEY' => (string) getenv('APP_KEY')]);
        app(InstallationState::class)->bootstrap(app(EnvironmentWriter::class));

        expect(Dotenv\Dotenv::createArrayBacked($this->containerInstallationDirectory.'/config')->load()['APP_KEY'])->toBe('base64:file-secret-key');
    } finally {
        putenv('APP_KEY'.($previousKey === false ? '' : '='.$previousKey));
        putenv('APP_KEY_FILE'.($previousFile === false ? '' : '='.$previousFile));
        if ($previousKey === false) {
            unset($_ENV['APP_KEY'], $_SERVER['APP_KEY']);
        } else {
            $_ENV['APP_KEY'] = $previousKey;
            $_SERVER['APP_KEY'] = $previousKey;
        }
        if ($previousFile === false) {
            unset($_ENV['APP_KEY_FILE'], $_SERVER['APP_KEY_FILE']);
        } else {
            $_ENV['APP_KEY_FILE'] = $previousFile;
            $_SERVER['APP_KEY_FILE'] = $previousFile;
        }
    }
});

test('omnibus bootstrap keeps queues on Redis database zero and cache on database one', function (): void {
    config()->set('installation.container_variant', 'omnibus');
    app()->instance('installation.external_environment', ['FILEBEAM_CONTAINER' => 'true', 'FILEBEAM_VARIANT' => 'omnibus']);

    app(InstallationState::class)->bootstrap(app(EnvironmentWriter::class));
    $environment = Dotenv\Dotenv::createArrayBacked($this->containerInstallationDirectory.'/config')->load();

    expect($environment)->toMatchArray(['QUEUE_CONNECTION' => 'redis', 'REDIS_DB' => '0', 'REDIS_CACHE_DB' => '1']);
});
