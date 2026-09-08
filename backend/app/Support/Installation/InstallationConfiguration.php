<?php

declare(strict_types=1);

namespace App\Support\Installation;

use App\Models\Filestore;
use App\Rules\ReservedUsername;
use App\Support\FilestoreRegistry;
use Illuminate\Database\Connection;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Validator;
use Illuminate\Validation\Rules\Password;
use Illuminate\Validation\ValidationException;
use InvalidArgumentException;
use Random\RandomException;
use Throwable;

class InstallationConfiguration
{
    private const MaxStores = 8;

    /**
     * @param  array<string, mixed>  $input
     * @return array<string, mixed>
     */
    public function validate(array $input): array
    {
        $validated = Validator::make($input, [
            'cache' => ['sometimes', 'array'],
            'database' => ['required', 'array:driver,transport,socket,host,port,database,username,password,sslmode'], 'database.driver' => ['required', 'in:sqlite,mysql,mariadb,pgsql'], 'database.transport' => ['sometimes', 'in:tcp,socket'], 'database.socket' => ['nullable', 'string', 'max:4096'], 'database.host' => ['nullable', 'string'], 'database.port' => ['nullable', 'integer', 'between:1,65535'], 'database.database' => ['nullable', 'string'], 'database.username' => ['nullable', 'string'], 'database.password' => ['nullable', 'string'], 'database.sslmode' => ['nullable', 'in:disable,allow,prefer,require,verify-ca,verify-full'],
            'instance' => ['required', 'array:name,url,username_domain,visibility,auto_updates_enabled'], 'instance.name' => ['required', 'string', 'max:255'], 'instance.url' => ['required', 'url'], 'instance.username_domain' => ['nullable', 'string', 'max:253'], 'instance.visibility' => ['required', 'in:public,private'], 'instance.auto_updates_enabled' => ['sometimes', 'boolean'],
            'storage' => ['required', 'array', 'min:1', 'max:'.self::MaxStores], 'storage.*' => ['array:name,driver,root,bucket,key,secret,region,endpoint,use_path_style_endpoint'], 'storage.*.name' => ['required', 'string', 'max:255', 'distinct'], 'storage.*.driver' => ['required', 'in:local,s3'], 'storage.*.root' => ['nullable', 'string'], 'storage.*.bucket' => ['nullable', 'string'], 'storage.*.key' => ['nullable', 'string'], 'storage.*.secret' => ['nullable', 'string'], 'storage.*.region' => ['nullable', 'string'], 'storage.*.endpoint' => ['nullable', 'url'], 'storage.*.use_path_style_endpoint' => ['required', 'boolean'],
            'placement_mode' => ['required', 'in:replicate,distribute'], 'admin' => ['required', 'array:name,username,email,password,password_confirmation,email_ownership_confirmed'], 'admin.name' => ['required', 'string', 'max:255'], 'admin.username' => ['required', 'string', 'regex:/\A[a-z0-9_]{3,24}\z/', new ReservedUsername], 'admin.email' => ['required', 'email:rfc'], 'admin.password' => ['required', 'confirmed', Password::defaults()], 'admin.email_ownership_confirmed' => ['accepted'], 'chunk_max_size' => ['required', 'integer', 'between:17,25000000'], 'chunk_warning_acknowledged' => ['sometimes', 'boolean'],
        ])->validate();
        $validated['admin']['username'] = mb_strtolower($validated['admin']['username']);
        $validated['cache'] = app(CacheConfiguration::class)->validate($validated['cache'] ?? ['driver' => 'file']);
        $validated['instance']['username_domain'] = ($validated['instance']['username_domain'] ?? '') === '' ? null : $validated['instance']['username_domain'];
        $this->validateDatabase($validated['database']);
        $this->validateInstance($validated['instance']);
        foreach ($validated['storage'] as $index => $storage) {
            $validated['storage'][$index] = $this->validateStorage($storage, "storage.{$index}");
        }
        $this->ensureNoProcessLocks();

        return $validated;
    }

    /** @param array<string, mixed> $database */
    private function validateDatabase(array &$database): void
    {
        if (! is_string($database['driver'] ?? null) || ! in_array($database['driver'], ['sqlite', 'mysql', 'mariadb', 'pgsql'], true)) {
            throw ValidationException::withMessages(['database.driver' => 'Unsupported database driver.']);
        }
        if ($database['driver'] === 'sqlite') {
            if (($database['transport'] ?? 'tcp') === 'socket') {
                throw ValidationException::withMessages(['database.transport' => 'SQLite uses a database file, not a Unix socket.']);
            }
            $database['database'] = $database['database'] ?? database_path('database.sqlite');
            if (! is_string($database['database']) || $database['database'] === '') {
                throw ValidationException::withMessages(['database.database' => 'A valid SQLite database path is required.']);
            }
            $this->prepareSqlitePath($database['database']);

            return;
        }
        $transport = $database['transport'] ?? 'tcp';
        if (! in_array($transport, ['tcp', 'socket'], true)) {
            throw ValidationException::withMessages(['database.transport' => 'Choose TCP or Unix socket.']);
        }
        if ($transport === 'socket') {
            $socket = $database['socket'] ?? null;
            if (! is_string($socket) || ! str_starts_with($socket, '/') || strlen($socket) > 4096 || preg_match('/[\x00-\x1f;]/', $socket)) {
                throw ValidationException::withMessages(['database.socket' => 'Enter an absolute Unix socket path (the socket directory for PostgreSQL).']);
            }
            $database['host'] = $database['driver'] === 'pgsql' ? $socket : 'localhost';
            $database['port'] = $database['driver'] === 'pgsql' ? ($database['port'] ?? 5432) : 3306;
        } else {
            $database['socket'] = '';
        }
        foreach (['host', 'database', 'username'] as $field) {
            if (! is_string($database[$field] ?? null) || $database[$field] === '') {
                throw ValidationException::withMessages(["database.{$field}" => 'This database setting is required.']);
            }
        }
        if ((! is_int($database['port'] ?? null) && ! ctype_digit((string) ($database['port'] ?? ''))) || (int) $database['port'] < 1 || (int) $database['port'] > 65535) {
            throw ValidationException::withMessages(['database.port' => 'A valid database port is required.']);
        }
        if (isset($database['password']) && ! is_string($database['password'])) {
            throw ValidationException::withMessages(['database.password' => 'The database password must be a string.']);
        }
        if (isset($database['sslmode']) && (! in_array($database['sslmode'], ['disable', 'allow', 'prefer', 'require', 'verify-ca', 'verify-full'], true))) {
            throw ValidationException::withMessages(['database.sslmode' => 'The SSL mode is invalid.']);
        }
    }

    private function prepareSqlitePath(string $path): void
    {
        $base = realpath((string) config('installation.sqlite_directory', database_path()));
        if ($base === false || ! str_starts_with($path, $base.DIRECTORY_SEPARATOR)) {
            throw ValidationException::withMessages(['database.database' => 'SQLite databases must remain in the approved non-public database directory.']);
        }
        $relative = substr($path, strlen($base) + 1);
        $segments = explode(DIRECTORY_SEPARATOR, $relative);
        if ($relative === '' || array_any($segments, fn (string $segment): bool => $segment === '' || $segment === '.' || $segment === '..')) {
            throw ValidationException::withMessages(['database.database' => 'SQLite database path is invalid.']);
        }
        $current = $base;
        foreach (array_slice($segments, 0, -1) as $segment) {
            $current .= DIRECTORY_SEPARATOR.$segment;
            if (is_link($current)) {
                throw ValidationException::withMessages(['database.database' => 'SQLite database path may not contain symlinks.']);
            }
        }
        if (is_link($path)) {
            throw ValidationException::withMessages(['database.database' => 'SQLite database may not be a symlink.']);
        }
        File::ensureDirectoryExists(dirname($path), 0700, true);
        if (! file_exists($path) && ! touch($path)) {
            throw ValidationException::withMessages(['database.database' => 'SQLite database could not be created.']);
        }
        chmod($path, 0600);
    }

    /** @param array<string, mixed> $instance */
    private function validateInstance(array $instance): void
    {
        $parts = parse_url($instance['url']);
        $host = $parts['host'] ?? null;
        $scheme = $parts['scheme'] ?? null;
        $host = is_string($host) ? trim($host, '[]') : null;
        $loopback = is_string($host) && ($host === 'localhost' || $host === '::1' || (filter_var($host, FILTER_VALIDATE_IP, FILTER_FLAG_IPV4) !== false && str_starts_with($host, '127.')));
        if (! is_string($host) || ! in_array($scheme, ['http', 'https'], true) || ($scheme === 'http' && ! $loopback) || isset($parts['user']) || isset($parts['pass']) || isset($parts['query']) || isset($parts['fragment']) || (($parts['path'] ?? '/') !== '/')) {
            throw ValidationException::withMessages(['instance.url' => 'Use a root HTTPS URL, except for localhost or loopback HTTP.']);
        }
        if (($instance['username_domain'] ?? null) !== null && filter_var($instance['username_domain'], FILTER_VALIDATE_DOMAIN, FILTER_FLAG_HOSTNAME) === false) {
            throw ValidationException::withMessages(['instance.username_domain' => 'Use an exact hostname.']);
        }
    }

    /**
     * @param  array<string, mixed>  $storage
     * @return array<string, mixed>
     */
    private function validateStorage(array $storage, string $attribute): array
    {
        if (! is_string($storage['driver'] ?? null) || ! in_array($storage['driver'], ['local', 's3'], true)) {
            throw ValidationException::withMessages(["{$attribute}.driver" => 'Unsupported storage driver.']);
        }
        if ($storage['driver'] === 'local') {
            if (! is_string($storage['root'] ?? null)) {
                throw ValidationException::withMessages(["{$attribute}.root" => 'A local storage root is required.']);
            }
            $this->ensureContainerFilestoreIsMounted($attribute);
            try {
                $storage['root'] = app(FilestoreRegistry::class)->normalizeLocalRoot($storage['root']);
                app(FilestoreRegistry::class)->localRoot($storage['root']);
            } catch (InvalidArgumentException) {
                throw ValidationException::withMessages(["{$attribute}.root" => 'The directory must remain inside the configured private storage root.']);
            }
        } else {
            foreach (['bucket', 'key', 'secret', 'region'] as $field) {
                if (! is_string($storage[$field] ?? null) || $storage[$field] === '') {
                    throw ValidationException::withMessages(["{$attribute}.{$field}" => 'This S3 setting is required.']);
                }
            }
            if (! is_bool($storage['use_path_style_endpoint'] ?? null)) {
                throw ValidationException::withMessages(["{$attribute}.use_path_style_endpoint" => 'This setting must be boolean.']);
            }
            $host = isset($storage['endpoint']) ? parse_url((string) $storage['endpoint'], PHP_URL_HOST) : null;
            if (isset($storage['endpoint']) && (! is_string($storage['endpoint']) || parse_url($storage['endpoint'], PHP_URL_SCHEME) !== 'https' || ! is_string($host) || (filter_var($host, FILTER_VALIDATE_IP) !== false && filter_var($host, FILTER_VALIDATE_IP, FILTER_FLAG_NO_PRIV_RANGE | FILTER_FLAG_NO_RES_RANGE) === false))) {
                throw ValidationException::withMessages(["{$attribute}.endpoint" => 'Use a public HTTPS endpoint.']);
            }
        }

        return $storage;
    }

    private function ensureContainerFilestoreIsMounted(string $attribute): void
    {
        if (! config('installation.container')) {
            return;
        }

        $root = (string) config('filebeam.filesystems.local_root');
        if (! is_dir($root) || ! is_writable($root)) {
            throw ValidationException::withMessages(["{$attribute}.root" => 'The container filestore mount is unavailable or not writable.']);
        }
    }

    private function ensureNoProcessLocks(): void
    {
        if (app(ContainerConfiguration::class)->enabled()) {
            return;
        }
        foreach (EnvironmentSettings::deploymentConflictKeys() as $key) {
            if (array_key_exists($key, app('installation.external_environment'))) {
                throw ValidationException::withMessages([$key => 'This setting is controlled by the process environment.']);
            }
        }
    }

    /** @param array<string, mixed> $database */
    public function testDatabase(array $database): void
    {
        $connection = 'installation_probe';
        config()->set("database.connections.{$connection}", $this->databaseConfiguration($database));
        DB::purge($connection);
        try {
            $database = DB::connection($connection);
            $database->getPdo();
            if ($database->getSchemaBuilder()->getTables() !== []) {
                throw ValidationException::withMessages(['database.database' => 'The selected database is not empty.']);
            }
            $this->probeSchemaPermissions($database, 'The database connection or schema permissions could not be verified.');
        } catch (ValidationException $exception) {
            throw $exception;
        } catch (Throwable) {
            throw ValidationException::withMessages(['database' => 'The database connection or schema permissions could not be verified.']);
        } finally {
            DB::disconnect($connection);
            config()->offsetUnset("database.connections.{$connection}");
        }
    }

    public function probeSchemaPermissions(Connection $connection, string $message): void
    {
        $table = 'filebeam_installation_probe_'.bin2hex(random_bytes(6));
        try {
            $connection->statement("create table {$table} (id integer)");
            $connection->statement("drop table {$table}");
        } catch (Throwable) {
            throw ValidationException::withMessages(['database' => $message]);
        }
    }

    /**
     * @param  array<string, mixed>  $database
     * @return array<string, mixed>
     */
    public function databaseConfiguration(array $database): array
    {
        $this->validateDatabase($database);
        if ($database['driver'] === 'sqlite') {
            return array_replace(config('database.connections.sqlite'), ['url' => null, 'database' => $database['database']]);
        }

        return array_replace(config("database.connections.{$database['driver']}"), [
            'url' => null,
            'driver' => $database['driver'],
            'host' => $database['host'],
            'port' => (int) $database['port'],
            'unix_socket' => in_array($database['driver'], ['mysql', 'mariadb'], true) ? $database['socket'] : '',
            'database' => $database['database'],
            'username' => $database['username'],
            'password' => $database['password'] ?? '',
            'sslmode' => $database['sslmode'] ?? 'prefer',
        ]);
    }

    /** @param array<string, mixed> $storage
     * @throws RandomException
     */
    public function testStorage(array $storage): void
    {
        $storage = $this->validateStorage($storage, 'storage');
        $store = new Filestore(['source' => 'database', 'driver' => $storage['driver'], 'configuration' => $this->storageConfiguration($storage)]);
        $path = 'installation-probes/'.bin2hex(random_bytes(16));
        $disk = null;
        try {
            $disk = app(FilestoreRegistry::class)->disk($store);
            $contents = bin2hex(random_bytes(16));
            $disk->put($path, $contents);
            if ($disk->get($path) !== $contents || ! $disk->delete($path) || $disk->exists($path)) {
                throw new InvalidArgumentException('Storage probe failed.');
            }
        } catch (Throwable) {
            throw ValidationException::withMessages(['storage' => 'The storage write/read/delete probe failed.']);
        } finally {
            try {
                $cleanupFailed = $disk !== null && (! $disk->delete($path) || $disk->exists($path));
            } catch (Throwable) {
                $cleanupFailed = true;
            }
            if ($cleanupFailed) {
                throw ValidationException::withMessages(['storage' => 'The storage probe could not be cleaned up.']);
            }
        }
    }

    /**
     * @param  array<string, mixed>  $storage
     * @return array<string, mixed>
     */
    public function storageConfiguration(array $storage): array
    {
        $storage = $this->validateStorage($storage, 'storage');

        return $storage['driver'] === 'local' ? ['root' => $storage['root'], 'visibility' => 'private'] : ['key' => $storage['key'], 'secret' => $storage['secret'], 'region' => $storage['region'], 'bucket' => $storage['bucket'], 'endpoint' => $storage['endpoint'] ?? null, 'use_path_style_endpoint' => $storage['use_path_style_endpoint'], 'visibility' => 'private'];
    }
}
