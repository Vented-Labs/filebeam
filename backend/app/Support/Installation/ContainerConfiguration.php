<?php

declare(strict_types=1);

namespace App\Support\Installation;

final class ContainerConfiguration
{
    public function enabled(): bool
    {
        return (bool) config('installation.container');
    }

    public function variant(): ?string
    {
        return $this->enabled() ? (string) config('installation.container_variant') : null;
    }

    /** @return array{database: bool, cache: bool, instance: bool, auto_updates: bool} */
    public function managed(): array
    {
        if (! $this->enabled()) {
            return ['database' => false, 'cache' => false, 'instance' => false, 'auto_updates' => false];
        }

        $environment = app('installation.external_environment');
        $omnibus = $this->variant() === 'omnibus';

        return [
            'database' => $omnibus || $this->has($environment, ['DB_CONNECTION', 'DB_URL', 'DB_HOST', 'DB_DATABASE', 'DB_USERNAME']),
            'cache' => $omnibus || $this->has($environment, ['CACHE_STORE', 'REDIS_URL', 'REDIS_HOST']),
            'instance' => $this->has($environment, ['APP_NAME', 'APP_URL', 'FILEBEAM_NAME', 'FILEBEAM_USERNAME_DOMAIN']),
            'auto_updates' => true,
        ];
    }

    /** @return array<string, mixed> */
    public function defaults(): array
    {
        $omnibus = $this->variant() === 'omnibus';
        $environment = app('installation.external_environment');
        $managed = $this->managed();

        $database = $omnibus
            ? ['driver' => 'pgsql', 'transport' => 'socket', 'socket' => '/run/filebeam/postgresql', 'host' => '/run/filebeam/postgresql', 'port' => 5432, 'database' => 'filebeam', 'username' => 'filebeam', 'password' => '', 'sslmode' => 'disable']
            : ['driver' => 'sqlite', 'transport' => 'tcp', 'socket' => '', 'host' => '127.0.0.1', 'port' => 3306, 'database' => (string) config('installation.sqlite_directory').'/database.sqlite', 'username' => '', 'password' => '', 'sslmode' => 'prefer'];
        $cache = $omnibus
            ? ['driver' => 'redis', 'transport' => 'unix', 'host' => '/run/filebeam/valkey/valkey.sock', 'port' => 0, 'username' => '', 'password' => '', 'database' => 1, 'prefix' => 'filebeam:']
            : app(CacheConfiguration::class)->defaults();

        if ($managed['database'] && ! $omnibus) {
            $database = $this->databaseFromEnvironment($environment, $database);
        }
        if ($managed['cache'] && ! $omnibus) {
            $cache = $this->cacheFromEnvironment($environment, $cache);
        }
        if ($omnibus) {
            $cache['password'] = $this->valkeyPassword($environment);
        }

        return ['database' => $database, 'cache' => $cache, 'instance' => $this->instanceFromEnvironment($environment)];
    }

    /** @return array<string, mixed> */
    public function publicDefaults(): array
    {
        $defaults = $this->defaults();
        $defaults['database']['password'] = '';
        $defaults['cache']['password'] = '';

        return $defaults;
    }

    /**
     * @param  array<string, mixed>  $input
     * @return array<string, mixed>
     */
    public function apply(array $input): array
    {
        if (! $this->enabled()) {
            return $input;
        }

        $defaults = $this->defaults();
        $managed = $this->managed();
        if ($managed['database']) {
            $input['database'] = $defaults['database'];
        }
        if ($managed['cache']) {
            $input['cache'] = $defaults['cache'];
        }
        if ($managed['instance'] && isset($input['instance']) && is_array($input['instance'])) {
            $input['instance'] = array_replace($input['instance'], $defaults['instance']);
        }
        if ($managed['auto_updates'] && isset($input['instance']) && is_array($input['instance'])) {
            $input['instance']['auto_updates_enabled'] = false;
        }

        return $input;
    }

    /**
     * @param  array<string, string>  $environment
     * @param  array<string, mixed>  $defaults
     * @return array<string, mixed>
     */
    private function databaseFromEnvironment(array $environment, array $defaults): array
    {
        $url = $this->databaseUrl($environment['DB_URL'] ?? null);
        $environment = [...$environment, ...$url];
        $driver = $environment['DB_CONNECTION'] ?? $defaults['driver'];
        $socket = $environment['DB_SOCKET'] ?? '';
        $transport = $socket !== '' ? 'socket' : 'tcp';

        return [
            'driver' => $driver,
            'transport' => $transport,
            'socket' => $socket,
            'host' => $transport === 'socket' && $driver === 'pgsql' ? $socket : ($environment['DB_HOST'] ?? $defaults['host']),
            'port' => (int) ($environment['DB_PORT'] ?? ($driver === 'pgsql' ? 5432 : 3306)),
            'database' => $environment['DB_DATABASE'] ?? $defaults['database'],
            'username' => $environment['DB_USERNAME'] ?? '',
            'password' => $environment['DB_PASSWORD'] ?? '',
            'sslmode' => $environment['DB_SSLMODE'] ?? 'prefer',
        ];
    }

    /**
     * @param  array<string, string>  $environment
     * @param  array<string, mixed>  $defaults
     * @return array<string, mixed>
     */
    private function cacheFromEnvironment(array $environment, array $defaults): array
    {
        $url = $this->redisUrl($environment['REDIS_URL'] ?? null);
        $environment = [...$environment, ...$url];
        $driver = $environment['CACHE_STORE'] ?? $defaults['driver'];
        $host = $environment['REDIS_HOST'] ?? $defaults['host'];
        $unix = $driver === 'redis' && str_starts_with($host, '/');

        return [
            'driver' => $driver,
            'transport' => $unix ? 'unix' : ($environment['REDIS_SCHEME'] ?? 'tcp'),
            'host' => $host,
            'port' => $unix ? 0 : (int) ($environment['REDIS_PORT'] ?? 6379),
            'username' => $environment['REDIS_USERNAME'] ?? '',
            'password' => $environment['REDIS_PASSWORD'] ?? '',
            'database' => (int) ($environment['REDIS_CACHE_DB'] ?? $environment['REDIS_DB'] ?? 0),
            'prefix' => $environment['REDIS_PREFIX'] ?? $defaults['prefix'],
        ];
    }

    /** @param array<string, string> $environment
     * @param  list<string>  $keys
     */
    private function has(array $environment, array $keys): bool
    {
        return array_any($keys, fn (string $key): bool => array_key_exists($key, $environment));
    }

    /** @return array<string, string> */
    private function databaseUrl(mixed $value): array
    {
        if (! is_string($value) || $value === '' || ($url = parse_url($value)) === false) {
            return [];
        }

        $scheme = match ($url['scheme'] ?? null) {
            'postgres', 'postgresql' => 'pgsql',
            'mysql', 'mariadb', 'pgsql' => $url['scheme'],
            default => null,
        };
        if ($scheme === null || ! isset($url['host'])) {
            return [];
        }

        return array_filter([
            'DB_CONNECTION' => $scheme,
            'DB_HOST' => (string) $url['host'],
            'DB_PORT' => isset($url['port']) ? (string) $url['port'] : null,
            'DB_DATABASE' => isset($url['path']) ? ltrim(rawurldecode((string) $url['path']), '/') : null,
            'DB_USERNAME' => isset($url['user']) ? rawurldecode((string) $url['user']) : null,
            'DB_PASSWORD' => isset($url['pass']) ? rawurldecode((string) $url['pass']) : null,
        ], fn (?string $setting): bool => $setting !== null);
    }

    /** @return array<string, string> */
    private function redisUrl(mixed $value): array
    {
        if (! is_string($value) || $value === '' || ($url = parse_url($value)) === false || ! in_array($url['scheme'] ?? null, ['redis', 'rediss'], true) || ! isset($url['host'])) {
            return [];
        }

        return array_filter([
            'CACHE_STORE' => 'redis',
            'REDIS_HOST' => (string) $url['host'],
            'REDIS_PORT' => isset($url['port']) ? (string) $url['port'] : null,
            'REDIS_SCHEME' => $url['scheme'] === 'rediss' ? 'tls' : 'tcp',
            'REDIS_DB' => isset($url['path']) && $url['path'] !== '' ? ltrim((string) $url['path'], '/') : null,
            'REDIS_USERNAME' => isset($url['user']) ? rawurldecode((string) $url['user']) : null,
            'REDIS_PASSWORD' => isset($url['pass']) ? rawurldecode((string) $url['pass']) : null,
        ], fn (?string $setting): bool => $setting !== null);
    }

    /** @param array<string, string> $environment */
    private function valkeyPassword(array $environment): string
    {
        if (array_key_exists('REDIS_PASSWORD', $environment)) {
            return $environment['REDIS_PASSWORD'];
        }

        $path = (string) config('installation.data_directory', '/data').'/config/valkey-password';

        return is_file($path) && ! is_link($path) && is_readable($path) ? rtrim((string) file_get_contents($path), "\r\n") : '';
    }

    /** @param array<string, string> $environment
     * @return array<string, string>
     */
    private function instanceFromEnvironment(array $environment): array
    {
        return array_filter([
            'name' => $environment['FILEBEAM_NAME'] ?? $environment['APP_NAME'] ?? null,
            'url' => $environment['APP_URL'] ?? null,
            'username_domain' => $environment['FILEBEAM_USERNAME_DOMAIN'] ?? null,
        ], fn (?string $setting): bool => $setting !== null);
    }
}
