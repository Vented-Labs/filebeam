<?php

declare(strict_types=1);

use App\Support\Installation\OptimizeInstallation;
use Illuminate\Process\Exceptions\ProcessTimedOutException as LaravelProcessTimedOutException;
use Illuminate\Process\PendingProcess;
use Illuminate\Process\ProcessResult;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Process;
use Illuminate\Validation\ValidationException;
use Symfony\Component\Process\Exception\ProcessTimedOutException as SymfonyProcessTimedOutException;
use Symfony\Component\Process\Process as SymfonyProcess;

beforeEach(function (): void {
    $keys = ['APP_NAME', 'APP_CONFIG_CACHE', 'APP_ROUTES_CACHE', 'APP_EVENTS_CACHE', 'FILEBEAM_INSTALL_TOKEN', 'CACHE_STORE', 'REDIS_HOST', 'REDIS_PASSWORD'];
    $this->optimizerEnvironment = array_combine($keys, array_map(getenv(...), $keys));
    $this->optimizerRealArtifacts = array_map(file_exists(...), [app()->getCachedConfigPath(), app()->getCachedRoutesPath(), app()->getCachedEventsPath(), app()->bootstrapPath('cache/blade-icons.php'), storage_path('framework/views')]);
    $this->optimizerDirectory = storage_path('framework/testing/optimize-installation-'.bin2hex(random_bytes(6)));
    optimizerFixture($this->optimizerDirectory);
    app()->instance('installation.external_environment', []);
    $this->optimizer = new OptimizeInstallation($this->optimizerDirectory);
});

afterEach(function (): void {
    foreach ($this->optimizerEnvironment as $key => $value) {
        putenv($key.($value === false ? '' : '='.$value));
    }
    File::deleteDirectory($this->optimizerDirectory);
});

test('optimizes a fresh application from its persisted environment', function (): void {
    putenv('APP_NAME=Stale Dotenv Name');
    putenv('APP_CONFIG_CACHE=/tmp/stale-config.php');
    putenv('FILEBEAM_INSTALL_TOKEN=stale-token');
    putenv('CACHE_STORE=file');
    putenv('REDIS_HOST=stale.example.test');
    putenv('REDIS_PASSWORD=stale-redis-secret');

    $this->optimizer->handle();

    $configPath = $this->optimizerDirectory.'/bootstrap/cache/config.php';
    $config = require $configPath;
    expect(File::exists($this->optimizerDirectory.'/bootstrap/cache/routes-v7.php'))->toBeTrue();
    expect(File::exists($this->optimizerDirectory.'/bootstrap/cache/events.php'))->toBeTrue();
    expect($config['app']['name'])->toBe('Fresh Filebeam');
    expect($config['database']['default'])->toBe('mysql');
    expect($config['database']['connections']['mysql']['unix_socket'])->toBe('/tmp/fresh.sock');
    expect($config['installation']['chunk_max_size'])->toBe('17');
    expect($config['session']['driver'])->toBe('cookie');
    expect($config['installation']['auto_updates_enabled'])->toBeTrue();
    expect($config['cache']['default'])->toBe('redis');
    expect($config['cache']['stores']['redis']['lock_connection'])->toBe('cache');
    expect($config['database']['redis']['cache'])->toMatchArray(['scheme' => 'tls', 'host' => 'redis.example.test', 'password' => 'fresh-redis-secret', 'database' => '0']);
    expect(File::get($configPath))->not->toContain('fixture-token', 'stale-token');
});

test('preserves genuine cache path overrides and scrubs inherited Dotenv values', function (): void {
    $cacheDirectory = $this->optimizerDirectory.'/operator-cache';
    File::ensureDirectoryExists($cacheDirectory);
    putenv('APP_CONFIG_CACHE='.$cacheDirectory.'/config.php');
    putenv('APP_ROUTES_CACHE='.$cacheDirectory.'/routes.php');
    putenv('APP_EVENTS_CACHE='.$cacheDirectory.'/events.php');
    app()->instance('installation.external_environment', [
        'APP_NAME' => 'Deployment Name',
        'DB_CONNECTION' => 'pgsql',
        'APP_CONFIG_CACHE' => $cacheDirectory.'/config.php',
        'APP_ROUTES_CACHE' => $cacheDirectory.'/routes.php',
        'APP_EVENTS_CACHE' => $cacheDirectory.'/events.php',
    ]);
    putenv('APP_NAME=Stale Dotenv Name');
    putenv('FILEBEAM_INSTALL_TOKEN=stale-token');
    Process::fake(function () use ($cacheDirectory): mixed {
        File::put($cacheDirectory.'/config.php', '<?php return [];');
        File::put($cacheDirectory.'/routes.php', '<?php return [];');
        File::put($cacheDirectory.'/events.php', '<?php return [];');

        return Process::result();
    });

    $this->optimizer->handle();

    Process::assertRan(function (PendingProcess $process) use ($cacheDirectory): bool {
        return array_slice($process->command, -3) === ['artisan', 'optimize', '--no-interaction']
            && $process->path === $this->optimizerDirectory
            && $process->environment['APP_NAME'] === 'Deployment Name'
            && $process->environment['DB_CONNECTION'] === 'pgsql'
            && $process->environment['DB_HOST'] === false
            && $process->environment['DB_SOCKET'] === false
            && $process->environment['FILEBEAM_INSTALL_TOKEN'] === false
            && $process->environment['APP_CONFIG_CACHE'] === $cacheDirectory.'/config.php'
            && $process->environment['APP_ROUTES_CACHE'] === $cacheDirectory.'/routes.php'
            && $process->environment['APP_EVENTS_CACHE'] === $cacheDirectory.'/events.php';
    });
});

test('removes only optimization artifacts when a subtask fails', function (): void {
    optimizerPartialArtifacts($this->optimizerDirectory);
    Process::fake(['*' => Process::result('  FAIL  ')]);

    expect(fn (): mixed => $this->optimizer->handle())->toThrow(ValidationException::class);

    expect(File::exists($this->optimizerDirectory.'/bootstrap/cache/config.php'))->toBeFalse();
    expect(File::exists($this->optimizerDirectory.'/bootstrap/cache/routes-v7.php'))->toBeFalse();
    expect(File::exists($this->optimizerDirectory.'/bootstrap/cache/events.php'))->toBeFalse();
    expect(File::exists($this->optimizerDirectory.'/bootstrap/cache/services.php'))->toBeTrue();
    expect(File::exists($this->optimizerDirectory.'/storage/framework/views/compiled.php'))->toBeFalse();
    expect(array_map(file_exists(...), [app()->getCachedConfigPath(), app()->getCachedRoutesPath(), app()->getCachedEventsPath(), app()->bootstrapPath('cache/blade-icons.php'), storage_path('framework/views')]))->toBe($this->optimizerRealArtifacts);
});

test('removes partial artifacts when optimization times out', function (): void {
    optimizerPartialArtifacts($this->optimizerDirectory);
    Process::fake(function (): never {
        $process = new SymfonyProcess([PHP_BINARY, '-r', '']);
        $process->setTimeout(1);

        throw new LaravelProcessTimedOutException(
            new SymfonyProcessTimedOutException($process, SymfonyProcessTimedOutException::TYPE_GENERAL),
            new ProcessResult($process),
        );
    });

    expect(fn (): mixed => $this->optimizer->handle())->toThrow(ValidationException::class);

    expect(File::exists($this->optimizerDirectory.'/bootstrap/cache/config.php'))->toBeFalse();
    expect(File::exists($this->optimizerDirectory.'/bootstrap/cache/routes-v7.php'))->toBeFalse();
    expect(File::exists($this->optimizerDirectory.'/bootstrap/cache/events.php'))->toBeFalse();
    expect(array_map(file_exists(...), [app()->getCachedConfigPath(), app()->getCachedRoutesPath(), app()->getCachedEventsPath(), app()->bootstrapPath('cache/blade-icons.php'), storage_path('framework/views')]))->toBe($this->optimizerRealArtifacts);
});

function optimizerFixture(string $directory): void
{
    foreach (['app/Http/Controllers', 'bootstrap/cache', 'config', 'resources/views', 'routes', 'storage/framework/views'] as $path) {
        File::ensureDirectoryExists($directory.'/'.$path);
    }

    File::put($directory.'/artisan', <<<'PHP'
#!/usr/bin/env php
<?php

require __DIR__.'/vendor/autoload.php';

$app = require __DIR__.'/bootstrap/app.php';
$status = $app->handleCommand(new Symfony\Component\Console\Input\ArgvInput);

exit($status);
PHP);
    File::put($directory.'/bootstrap/app.php', <<<'PHP'
<?php

use Illuminate\Foundation\Application;

return Application::configure(basePath: dirname(__DIR__))
    ->withProviders([Illuminate\Foundation\Providers\ConsoleSupportServiceProvider::class])
    ->withRouting(web: __DIR__.'/../routes/web.php')
    ->withExceptions(fn () => null)
    ->create();
PHP);
    File::put($directory.'/config/app.php', <<<'PHP'
<?php

return ['name' => env('APP_NAME'), 'env' => env('APP_ENV', 'production'), 'debug' => env('APP_DEBUG', false), 'url' => env('APP_URL'), 'key' => env('APP_KEY')];
PHP);
    File::put($directory.'/config/database.php', File::get(config_path('database.php')));
    File::put($directory.'/config/cache.php', File::get(config_path('cache.php')));
    File::put($directory.'/config/installation.php', <<<'PHP'
<?php

return ['chunk_max_size' => env('CHUNK_MAX_SIZE'), 'auto_updates_enabled' => env('FILEBEAM_AUTO_UPDATES_ENABLED', false)];
PHP);
    File::put($directory.'/config/session.php', <<<'PHP'
<?php

return ['driver' => env('SESSION_DRIVER')];
PHP);
    File::put($directory.'/config/view.php', <<<'PHP'
<?php

return ['paths' => [resource_path('views')], 'compiled' => storage_path('framework/views')];
PHP);
    File::put($directory.'/routes/web.php', <<<'PHP'
<?php

use Illuminate\Support\Facades\Route;

Route::get('/fixture', [Illuminate\Foundation\Inspiring::class, 'quote']);
PHP);
    File::put($directory.'/resources/views/welcome.blade.php', 'fixture');
    File::put($directory.'/composer.json', '{"autoload":{"psr-4":{"App\\\\":"app/"}}}');
    File::put($directory.'/.env', "APP_ENV=production\nAPP_KEY=base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\nAPP_NAME=\"Fresh Filebeam\"\nAPP_URL=http://fixture.test\nSESSION_DRIVER=cookie\nDB_CONNECTION=mysql\nDB_SOCKET=/tmp/fresh.sock\nCHUNK_MAX_SIZE=17\nFILEBEAM_AUTO_UPDATES_ENABLED=true\nFILEBEAM_INSTALL_TOKEN=fixture-token\n");
    File::append($directory.'/.env', "CACHE_STORE=redis\nREDIS_HOST=redis.example.test\nREDIS_SCHEME=tls\nREDIS_PASSWORD=fresh-redis-secret\nREDIS_CACHE_DB=0\nREDIS_CACHE_LOCK_CONNECTION=cache\n");
    symlink(base_path('vendor'), $directory.'/vendor');
}

function optimizerPartialArtifacts(string $directory): void
{
    File::put($directory.'/bootstrap/cache/config.php', '<?php return [];');
    File::put($directory.'/bootstrap/cache/routes-v7.php', '<?php return [];');
    File::put($directory.'/bootstrap/cache/events.php', '<?php return [];');
    File::put($directory.'/bootstrap/cache/services.php', 'preserve');
    File::put($directory.'/storage/framework/views/compiled.php', 'partial');
}
