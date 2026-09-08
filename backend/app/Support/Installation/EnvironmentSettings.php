<?php

declare(strict_types=1);

namespace App\Support\Installation;

final class EnvironmentSettings
{
    /** @var list<string> */
    private const FileSecretKeys = [
        'APP_KEY', 'DB_PASSWORD', 'REDIS_PASSWORD', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY', 'MAIL_PASSWORD',
    ];

    /** @var list<string> */
    private const OutputKeys = [
        'APP_KEY', 'APP_ENV', 'APP_DEBUG', 'APP_NAME', 'APP_URL', 'SESSION_DRIVER', 'CACHE_STORE', 'QUEUE_CONNECTION', 'FILEBEAM_CRON_QUEUE_ENABLED',
        'DB_CONNECTION', 'DB_HOST', 'DB_PORT', 'DB_DATABASE', 'DB_USERNAME', 'DB_PASSWORD', 'DB_SSLMODE', 'DB_SOCKET', 'DB_URL',
        'CHUNK_MAX_SIZE', 'FILEBEAM_NAME', 'FILEBEAM_USERNAME_DOMAIN', 'FILEBEAM_INSTALL_TOKEN', 'FILEBEAM_FILESYSTEMS',
        'FILEBEAM_REGISTRATION_ENABLED', 'FILEBEAM_ANONYMOUS_UPLOADS_ENABLED', 'FILEBEAM_USERNAME_ROUTING_ENABLED',
        'FILEBEAM_AUTO_UPDATES_ENABLED',
        'CACHE_PREFIX', 'REDIS_CLIENT', 'REDIS_HOST', 'REDIS_PORT', 'REDIS_USERNAME', 'REDIS_PASSWORD', 'REDIS_SCHEME',
        'REDIS_DB', 'REDIS_CACHE_DB', 'REDIS_PREFIX', 'REDIS_CACHE_CONNECTION', 'REDIS_CACHE_LOCK_CONNECTION', 'REDIS_URL',
    ];

    /** @var list<string> */
    private const DeploymentConflictKeys = [
        'CACHE_STORE', 'CACHE_PREFIX', 'REDIS_CLIENT', 'REDIS_HOST', 'REDIS_PORT', 'REDIS_USERNAME', 'REDIS_PASSWORD', 'REDIS_SCHEME',
        'REDIS_DB', 'REDIS_CACHE_DB', 'REDIS_PREFIX', 'REDIS_CACHE_CONNECTION', 'REDIS_CACHE_LOCK_CONNECTION', 'REDIS_URL',
        'APP_NAME', 'APP_URL', 'DB_CONNECTION', 'DB_HOST', 'DB_PORT', 'DB_DATABASE', 'DB_USERNAME', 'DB_PASSWORD', 'DB_SSLMODE',
        'DB_SOCKET', 'DB_URL', 'CHUNK_MAX_SIZE', 'FILEBEAM_NAME', 'FILEBEAM_USERNAME_DOMAIN', 'FILEBEAM_FILESYSTEMS',
        'FILEBEAM_REGISTRATION_ENABLED', 'FILEBEAM_ANONYMOUS_UPLOADS_ENABLED', 'FILEBEAM_USERNAME_ROUTING_ENABLED',
        'FILEBEAM_AUTO_UPDATES_ENABLED',
    ];

    /** @var list<string> */
    private const CachePathKeys = [
        'APP_CONFIG_CACHE', 'APP_ROUTES_CACHE', 'APP_EVENTS_CACHE', 'APP_SERVICES_CACHE', 'APP_PACKAGES_CACHE',
    ];

    /** @return list<string> */
    public static function outputKeys(): array
    {
        return self::OutputKeys;
    }

    /** @return list<string> */
    public static function deploymentConflictKeys(): array
    {
        return self::DeploymentConflictKeys;
    }

    /** @return list<string> */
    public static function captureKeys(): array
    {
        // The setup token is generated server-side and must never be inherited by a child optimize process.
        return array_values(array_filter([...self::OutputKeys, ...self::CachePathKeys, 'FILEBEAM_CONTAINER', 'FILEBEAM_VARIANT', 'FILEBEAM_DATA_DIR'], fn (string $key): bool => $key !== 'FILEBEAM_INSTALL_TOKEN'));
    }

    /** @return list<string> */
    public static function cachePathKeys(): array
    {
        return self::CachePathKeys;
    }

    public static function resolveFileSecrets(): void
    {
        foreach (self::FileSecretKeys as $key) {
            $value = getenv($key);
            $file = getenv($key.'_FILE');
            if ($value !== false && $file !== false) {
                throw new \InvalidArgumentException("{$key} and {$key}_FILE cannot both be set.");
            }
            if ($file === false) {
                continue;
            }
            if ($file === '' || ! str_starts_with($file, '/') || ! is_file($file) || ! is_readable($file)) {
                throw new \InvalidArgumentException("{$key}_FILE must name a readable regular file.");
            }

            // Docker secrets may be symlinks; is_file verifies their resolved target is regular.
            $secret = file_get_contents($file);
            if ($secret === false || str_contains($secret, "\0") || str_contains($secret, "\r")) {
                throw new \InvalidArgumentException("{$key}_FILE contains an unsupported value.");
            }
            $secret = rtrim($secret, "\n");
            putenv($key.'='.$secret);
            $_ENV[$key] = $secret;
            $_SERVER[$key] = $secret;
            putenv($key.'_FILE');
            unset($_ENV[$key.'_FILE'], $_SERVER[$key.'_FILE']);
        }
    }

    public static function containerDataDirectory(): string
    {
        $directory = getenv('FILEBEAM_DATA_DIR');
        $directory = $directory === false ? '/data' : $directory;
        if ($directory === '' || ! str_starts_with($directory, '/') || ($directory !== '/' && str_ends_with($directory, '/'))) {
            throw new \InvalidArgumentException('FILEBEAM_DATA_DIR must be an absolute path without a trailing slash.');
        }

        $current = '';
        foreach (explode('/', ltrim($directory, '/')) as $segment) {
            if ($segment === '' || $segment === '.' || $segment === '..') {
                throw new \InvalidArgumentException('FILEBEAM_DATA_DIR must not contain empty or traversal segments.');
            }
            $current .= '/'.$segment;
            if (is_link($current)) {
                throw new \InvalidArgumentException('FILEBEAM_DATA_DIR must not traverse symbolic links.');
            }
        }

        return $directory;
    }
}
