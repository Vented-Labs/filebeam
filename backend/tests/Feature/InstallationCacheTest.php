<?php

declare(strict_types=1);

use App\Support\Installation\CacheConfiguration;
use App\Support\Installation\EnvironmentWriter;
use Dotenv\Dotenv;
use Illuminate\Cache\FileStore;
use Illuminate\Cache\RedisStore;
use Illuminate\Filesystem\Filesystem;
use Illuminate\Support\Facades\File;
use Illuminate\Validation\ValidationException;

beforeEach(function (): void {
    $this->cacheProbeDirectory = storage_path('framework/testing/cache-configuration-'.bin2hex(random_bytes(8)));
    $this->previousFileStore = config('cache.stores.file');
    config()->set('cache.stores.file.path', $this->cacheProbeDirectory.'/data');
    config()->set('cache.stores.file.lock_path', $this->cacheProbeDirectory.'/locks');
});

afterEach(function (): void {
    config()->set('cache.stores.file', $this->previousFileStore);
    File::deleteDirectory($this->cacheProbeDirectory);
});

test('uses the configured file store and discards Redis draft settings', function (): void {
    $configuration = app(CacheConfiguration::class);

    $configuration->test(['driver' => 'file', 'host' => 'redis.example.test', 'password' => 'secret']);

    expect($configuration->defaults())->toBe(['driver' => 'file', 'transport' => 'tcp', 'host' => '127.0.0.1', 'port' => 6379, 'username' => '', 'password' => '', 'database' => 0, 'prefix' => 'filebeam:']);
    expect($configuration->validate(['driver' => 'file', 'host' => 'redis.example.test']))->toBe(['driver' => 'file']);
    expect($configuration->environmentValues(['driver' => 'file']))->toBe(['CACHE_STORE' => 'file']);
    expect(File::isDirectory($this->cacheProbeDirectory.'/data'))->toBeTrue();
    expect(File::isDirectory($this->cacheProbeDirectory.'/locks'))->toBeTrue();
    expect(File::allFiles($this->cacheProbeDirectory))->toBeEmpty();
    expect($configuration->validate(['driver' => 'file', 'port' => 'unused', 'prefix' => '/unused']))->toBe(['driver' => 'file']);
});

test('rejects invalid Redis settings without writing to the file cache', function (): void {
    $configuration = app(CacheConfiguration::class);

    expect(fn (): array => $configuration->validate(['driver' => 'redis', 'transport' => 'unix', 'host' => 'relative.sock']))->toThrow(ValidationException::class);

    expect(File::exists($this->cacheProbeDirectory))->toBeFalse();
});

test('normalizes TCP TLS and Unix Redis environment values', function (): void {
    $configuration = app(CacheConfiguration::class);

    $tls = $configuration->environmentValues(['driver' => 'redis', 'transport' => 'tls', 'host' => 'cache.example.test', 'port' => '6380', 'database' => '2', 'prefix' => 'tenant_1:']);
    $unix = $configuration->environmentValues(['driver' => 'redis', 'transport' => 'unix', 'host' => '/run/redis/redis.sock', 'port' => 6380]);

    expect($tls['REDIS_SCHEME'])->toBe('tls')->and($tls['REDIS_PORT'])->toBe('6380')->and($tls['REDIS_DB'])->toBe('2')->and($tls['REDIS_CACHE_DB'])->toBe('2');
    expect($unix['REDIS_HOST'])->toBe('/run/redis/redis.sock')->and($unix['REDIS_PORT'])->toBe('0')->and($unix['REDIS_SCHEME'])->toBeNull();
    $normalized = $configuration->validate(['driver' => 'redis', 'transport' => 'unix', 'host' => '/run/redis/redis.sock']);
    expect($configuration->validate($normalized))->toBe($normalized);
});

test('Redis configuration rejects invalid ports hosts and key prefixes', function (array $input, string $field): void {
    try {
        app(CacheConfiguration::class)->validate(['driver' => 'redis', ...$input]);
        test()->fail('Expected invalid cache configuration to be rejected.');
    } catch (ValidationException $exception) {
        expect($exception->errors())->toHaveKey('cache.'.$field);
    }
})->with([
    [['port' => 0], 'port'],
    [['host' => 'redis://user:secret@localhost'], 'host'],
    [['prefix' => '*'], 'prefix'],
    [['host' => null], 'host'],
    [['prefix' => null], 'prefix'],
    [['database' => -1], 'database'],
]);

test('Redis configuration persists credentials without changing cookie sessions or cron queues', function (): void {
    config()->set('installation.environment_path', $this->cacheProbeDirectory.'/.env');
    $writer = app(EnvironmentWriter::class);
    $writer->write(['SESSION_DRIVER' => 'cookie', 'QUEUE_CONNECTION' => 'database']);
    $values = app(CacheConfiguration::class)->environmentValues([
        'driver' => 'redis', 'transport' => 'tls', 'host' => 'redis.example.test', 'port' => 6380,
        'username' => 'tenant', 'password' => ' secret $ # " ', 'database' => 0, 'prefix' => 'filebeam_io:',
    ]);
    $writer->write($values);

    $environment = Dotenv::createArrayBacked($this->cacheProbeDirectory)->load();
    expect($environment)->toMatchArray([
        'CACHE_STORE' => 'redis', 'REDIS_CLIENT' => 'phpredis', 'REDIS_SCHEME' => 'tls',
        'REDIS_PASSWORD' => ' secret $ # " ', 'REDIS_CACHE_DB' => '0', 'REDIS_DB' => '0',
        'REDIS_CACHE_CONNECTION' => 'cache', 'REDIS_CACHE_LOCK_CONNECTION' => 'cache',
        'SESSION_DRIVER' => 'cookie', 'QUEUE_CONNECTION' => 'database',
    ]);
});

test('the Redis probe accepts numeric string counters and checks locks and cleanup', function (): void {
    if (! extension_loaded('redis')) {
        test()->markTestSkipped('The PHP Redis extension is unavailable.');
    }
    $values = [];
    $locks = new FileStore(new Filesystem, $this->cacheProbeDirectory.'/locks');
    $store = Mockery::mock(RedisStore::class);
    $store->shouldReceive('put')->once()->andReturnUsing(function (string $key, mixed $value) use (&$values): bool {
        $values[$key] = $value;

        return true;
    });
    $store->shouldReceive('get')->andReturnUsing(function (string $key) use (&$values): mixed {
        return $values[$key] ?? null;
    });
    $store->shouldReceive('add')->twice()->andReturnUsing(function (string $key, mixed $value) use (&$values): bool {
        if (isset($values[$key])) {
            return false;
        }
        $values[$key] = (string) $value;

        return true;
    });
    $store->shouldReceive('increment')->once()->andReturnUsing(function (string $key) use (&$values): int {
        $values[$key] = '2';

        return 2;
    });
    $store->shouldReceive('lock')->twice()->andReturnUsing(fn (string $key, int $seconds) => $locks->lock($key, $seconds));
    $store->shouldReceive('forget')->twice()->andReturnUsing(function (string $key) use (&$values): bool {
        unset($values[$key]);

        return true;
    });
    $configuration = Mockery::mock(CacheConfiguration::class)->makePartial()->shouldAllowMockingProtectedMethods();
    $configuration->shouldReceive('store')->twice()->andReturn($store);

    $configuration->test(['driver' => 'redis', 'host' => 'redis.example.test']);

    expect($values)->toBeEmpty();
    expect(File::allFiles($this->cacheProbeDirectory))->toBeEmpty();
});

test('returns a generic error for an unreachable Redis server', function (): void {
    if (! extension_loaded('redis')) {
        test()->markTestSkipped('The PHP Redis extension is unavailable.');
    }

    try {
        app(CacheConfiguration::class)->test(['driver' => 'redis', 'host' => '127.0.0.1', 'port' => 1]);
        test()->fail('Expected the Redis probe to fail.');
    } catch (ValidationException $exception) {
        expect($exception->errors())->toBe(['cache' => ['The cache probe failed.']]);
    }
});

test('probes an isolated password-protected Redis Unix socket and cleans up its keys', function (): void {
    if (! extension_loaded('redis') || shell_exec('command -v redis-server') === null) {
        test()->markTestSkipped('redis-server or the PHP Redis extension is unavailable.');
    }

    $directory = storage_path('framework/testing/cache-redis-'.bin2hex(random_bytes(8)));
    $socket = $directory.'/redis.sock';
    File::ensureDirectoryExists($directory);
    $process = proc_open(['redis-server', '--port', '0', '--unixsocket', $socket, '--save', '', '--appendonly', 'no', '--requirepass', 'probe-secret'], [STDIN, STDOUT, STDERR], $pipes);

    try {
        for ($attempt = 0; $attempt < 30 && ! file_exists($socket); $attempt++) {
            usleep(100_000);
        }
        if (! file_exists($socket)) {
            test()->fail('The isolated Redis server did not start.');
        }

        $configuration = app(CacheConfiguration::class);
        $input = ['driver' => 'redis', 'transport' => 'unix', 'host' => $socket, 'password' => 'probe-secret', 'prefix' => 'isolated:'];
        $configuration->test($input);

        expect(function () use ($configuration, $input): void {
            $configuration->test(array_replace($input, ['password' => 'wrong-secret']));
        })->toThrow(ValidationException::class);
        $redis = new Redis;
        $redis->connect($socket, 0, 3);
        $redis->auth('probe-secret');
        expect($redis->keys('isolated:cache:installation-probe:*'))->toBe([]);
        $redis->close();
    } finally {
        if (is_resource($process)) {
            proc_terminate($process);
            proc_close($process);
        }
        File::deleteDirectory($directory);
    }
});
