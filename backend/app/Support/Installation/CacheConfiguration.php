<?php

declare(strict_types=1);

namespace App\Support\Installation;

use Illuminate\Cache\FileStore;
use Illuminate\Cache\RedisStore;
use Illuminate\Filesystem\Filesystem;
use Illuminate\Redis\RedisManager;
use Illuminate\Support\Facades\Validator;
use Illuminate\Validation\ValidationException;
use Throwable;

class CacheConfiguration
{
    /** @return array{driver: 'file', transport: 'tcp', host: '127.0.0.1', port: 6379, username: '', password: '', database: 0, prefix: 'filebeam:'} */
    public function defaults(): array
    {
        return ['driver' => 'file', 'transport' => 'tcp', 'host' => '127.0.0.1', 'port' => 6379, 'username' => '', 'password' => '', 'database' => 0, 'prefix' => 'filebeam:'];
    }

    /**
     * @param  array<string, mixed>  $input
     * @return array{driver: 'file'}|array{driver: 'redis', transport: 'tcp'|'tls'|'unix', host: string, port: int, username: string, password: string, database: int, prefix: string}
     */
    public function validate(array $input): array
    {
        if (($input['driver'] ?? null) === 'file') {
            $input = ['driver' => 'file'];
        } elseif (($input['transport'] ?? null) === 'unix') {
            $input['port'] = 0;
        }
        $validator = Validator::make($input, [
            'driver' => ['required', 'in:file,redis'],
            'transport' => ['nullable', 'in:tcp,tls,unix'],
            'host' => ['nullable', 'string', 'max:4096'],
            'port' => ['nullable', 'integer', 'between:0,65535'],
            'username' => ['nullable', 'string', 'max:255'],
            'password' => ['nullable', 'string', 'max:4096'],
            'database' => ['nullable', 'integer', 'min:0'],
            'prefix' => ['nullable', 'string', 'max:128', 'regex:/\A[a-zA-Z0-9:_-]+\z/'],
        ]);
        if ($validator->fails()) {
            $errors = [];
            foreach ($validator->errors()->messages() as $field => $messages) {
                $errors['cache.'.$field] = $messages;
            }
            throw ValidationException::withMessages($errors);
        }
        $validated = $validator->validated();

        if ($validated['driver'] === 'file') {
            return ['driver' => 'file'];
        }

        $defaults = $this->defaults();
        $transport = (string) ($validated['transport'] ?? $defaults['transport']);
        if (! in_array($transport, ['tcp', 'tls', 'unix'], true)) {
            throw ValidationException::withMessages(['cache.transport' => 'Choose TCP, TLS, or Unix socket.']);
        }
        $cache = [
            'driver' => 'redis',
            'transport' => $transport,
            'host' => trim((string) (array_key_exists('host', $validated) ? $validated['host'] : $defaults['host'])),
            'port' => (int) ($validated['port'] ?? $defaults['port']),
            'username' => (string) ($validated['username'] ?? $defaults['username']),
            'password' => (string) ($validated['password'] ?? $defaults['password']),
            'database' => (int) ($validated['database'] ?? $defaults['database']),
            'prefix' => trim((string) (array_key_exists('prefix', $validated) ? $validated['prefix'] : $defaults['prefix'])),
        ];

        if ($cache['host'] === '') {
            throw ValidationException::withMessages(['cache.host' => 'A Redis host is required.']);
        }
        if ($cache['prefix'] === '' || ! preg_match('/\A[a-zA-Z0-9:_-]+\z/', $cache['prefix'])) {
            throw ValidationException::withMessages(['cache.prefix' => 'Use only letters, numbers, colons, underscores, and hyphens for the Redis prefix.']);
        }
        if ($cache['transport'] === 'unix') {
            if (! str_starts_with($cache['host'], '/') || preg_match('/[\x00-\x1f]/', $cache['host'])) {
                throw ValidationException::withMessages(['cache.host' => 'Enter an absolute Redis Unix socket path.']);
            }
            $cache['port'] = 0;
        } else {
            if ($cache['port'] < 1) {
                throw ValidationException::withMessages(['cache.port' => 'Enter a Redis TCP port between 1 and 65535.']);
            }
            if (filter_var($cache['host'], FILTER_VALIDATE_IP) === false && filter_var($cache['host'], FILTER_VALIDATE_DOMAIN, FILTER_FLAG_HOSTNAME) === false) {
                throw ValidationException::withMessages(['cache.host' => 'Enter a Redis hostname or IP address, not a URL.']);
            }
        }

        return $cache;
    }

    /** @param array<string, mixed> $input */
    public function test(array $input): void
    {
        $cache = $this->validate($input);
        if ($cache['driver'] === 'redis' && ! extension_loaded('redis')) {
            throw ValidationException::withMessages(['cache.driver' => 'The PHP Redis extension is required for Redis cache.']);
        }
        $key = 'installation-probe:'.bin2hex(random_bytes(16));
        $store = null;
        $peerStore = null;
        $firstLock = null;
        $secondLock = null;
        $cleanupFailed = false;
        $failed = false;

        try {
            $store = $this->store($cache);
            $peerStore = $this->store($cache);
            $value = bin2hex(random_bytes(16));

            if (! $store->put($key, $value, 60)) {
                throw new \RuntimeException('Cache write probe failed.');
            }
            if ($store->get($key) !== $value) {
                throw new \RuntimeException('Cache read probe failed.');
            }
            if (! $store->add($key.':counter', 1, 60)) {
                throw new \RuntimeException('Cache atomic add probe failed.');
            }
            if ($peerStore->add($key.':counter', 1, 60)) {
                throw new \RuntimeException('Cache atomic add probe failed.');
            }
            if ($store->increment($key.':counter') !== 2) {
                throw new \RuntimeException('Cache increment probe failed.');
            }
            if ((string) $store->get($key.':counter') !== '2') {
                throw new \RuntimeException('Cache read/write probe failed.');
            }

            $firstLock = $store->lock($key.':lock', 60);
            $secondLock = $peerStore->lock($key.':lock', 60);
            $firstAcquired = $firstLock->get();
            $secondAcquiredWhileHeld = $secondLock->get();
            $firstReleased = $firstLock->release();
            $secondAcquiredAfterRelease = $secondLock->get();
            $secondReleased = $secondLock->release();
            if (! $firstAcquired || $secondAcquiredWhileHeld || ! $firstReleased || ! $secondAcquiredAfterRelease || ! $secondReleased) {
                throw new \RuntimeException('Cache lock probe failed.');
            }
            $firstLock = null;
            $secondLock = null;
        } catch (Throwable) {
            $failed = true;
            throw ValidationException::withMessages(['cache' => 'The cache probe failed.']);
        } finally {
            try {
                if ($firstLock !== null) {
                    $firstLock->release();
                }
                if ($secondLock !== null) {
                    $secondLock->release();
                }
                if ($store !== null) {
                    foreach ([$key, $key.':counter'] as $probeKey) {
                        $store->forget($probeKey);
                        if ($store->get($probeKey) !== null) {
                            $cleanupFailed = true;
                        }
                    }
                }
            } catch (Throwable) {
                $cleanupFailed = true;
            }

            if ($cleanupFailed && ! $failed) {
                throw ValidationException::withMessages(['cache' => 'The cache probe could not be cleaned up.']);
            }
        }
    }

    /**
     * @param  array<string, mixed>  $input
     * @return array<string, string|null>
     */
    public function environmentValues(array $input): array
    {
        $cache = $this->validate($input);
        if ($cache['driver'] === 'file') {
            return ['CACHE_STORE' => 'file'];
        }

        return [
            'CACHE_STORE' => 'redis',
            'REDIS_CLIENT' => 'phpredis',
            'REDIS_HOST' => (string) $cache['host'],
            'REDIS_PORT' => (string) $cache['port'],
            'REDIS_USERNAME' => (string) $cache['username'],
            'REDIS_PASSWORD' => (string) $cache['password'],
            'REDIS_SCHEME' => $cache['transport'] === 'unix' ? null : (string) $cache['transport'],
            'REDIS_DB' => (string) $cache['database'],
            'REDIS_CACHE_DB' => (string) $cache['database'],
            'REDIS_PREFIX' => (string) $cache['prefix'],
            'CACHE_PREFIX' => 'cache:',
            'REDIS_CACHE_CONNECTION' => 'cache',
            'REDIS_CACHE_LOCK_CONNECTION' => 'cache',
            'REDIS_URL' => null,
        ];
    }

    /**
     * @param  array{driver: 'file'}|array{driver: 'redis', transport: 'tcp'|'tls'|'unix', host: string, port: int, username: string, password: string, database: int, prefix: string}  $cache
     */
    protected function store(array $cache): FileStore|RedisStore
    {
        if ($cache['driver'] === 'file') {
            $configuration = config('cache.stores.file');
            $store = new FileStore(app(Filesystem::class), $configuration['path'], null, false);
            $store->setLockDirectory($configuration['lock_path'] ?? null);

            return $store;
        }

        $connection = [
            'host' => $cache['host'],
            'port' => $cache['port'],
            'username' => $cache['username'],
            'password' => $cache['password'],
            'database' => $cache['database'],
            'timeout' => 3,
            'read_timeout' => 3,
            'max_retries' => 0,
        ];
        if ($cache['transport'] !== 'unix') {
            $connection['scheme'] = $cache['transport'];
        }

        $manager = new RedisManager(app(), 'phpredis', ['probe' => $connection]);
        $store = new RedisStore($manager, (string) $cache['prefix'].'cache:', 'probe', false);
        $store->setLockConnection('probe');

        return $store;
    }
}
