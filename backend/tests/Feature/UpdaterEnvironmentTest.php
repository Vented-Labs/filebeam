<?php

declare(strict_types=1);

use Filebeam\Updater\Env;
use Illuminate\Support\Facades\File;

beforeEach(function (): void {
    require_once base_path('../updater/Updater.php');
    $this->environmentRoot = storage_path('framework/testing/updater-env-'.bin2hex(random_bytes(4)));
    File::ensureDirectoryExists($this->environmentRoot.'/backend');
    $this->environmentFile = $this->environmentRoot.'/backend/.env';
    $this->environment = [];
    foreach (['APP_URL', 'APP_CONFIG_CACHE', 'DB_CONNECTION', 'DB_URL', 'DB_DATABASE', 'DB_HOST', 'DB_PORT', 'DB_USERNAME', 'DB_PASSWORD', 'DB_SOCKET', 'DB_SSLMODE', 'DB_SSLROOTCERT', 'DB_SSLCERT', 'DB_SSLKEY', 'PGHOSTADDR', 'PGSERVICE', 'PGSERVICEFILE', 'PGOPTIONS', 'PGTARGETSESSIONATTRS', 'PGLOADBALANCEHOSTS'] as $key) {
        $this->environment[$key] = getenv($key);
        putenv($key);
    }
});

afterEach(function (): void {
    foreach ($this->environment as $key => $value) {
        putenv($value === false ? $key : $key.'='.$value);
    }
    File::deleteDirectory($this->environmentRoot);
});

test('reads quoted dotenv values without trimming or mutating the process environment', function (): void {
    File::put($this->environmentFile, <<<'ENV'
APP_URL="https://filebeam.test/#ready"
DB_PASSWORD=' leading $password\with:colon '
ENV);

    $environment = Env::read($this->environmentFile);

    expect($environment['APP_URL'])->toBe('https://filebeam.test/#ready')
        ->and($environment['DB_PASSWORD'])->toBe(' leading $password\with:colon ')
        ->and(getenv('DB_PASSWORD'))->toBeFalse();
});

test('uses genuine process database overrides and normalizes PostgreSQL URLs', function (): void {
    File::put($this->environmentFile, 'DB_CONNECTION=sqlite'.PHP_EOL.'DB_DATABASE=ignored.sqlite'.PHP_EOL);
    putenv('DB_CONNECTION=postgres');
    putenv('DB_URL=postgresql://backup:pa%24%3Aword@db.example:5433/filebeam?sslmode=verify-full&sslrootcert=%2Fetc%2Fpostgres-ca.pem');
    putenv('DB_SOCKET=/var/run/postgresql');

    $database = Env::database($this->environmentFile);

    expect($database)->toMatchArray([
        'driver' => 'pgsql',
        'host' => 'db.example',
        'port' => '5433',
        'database' => 'filebeam',
        'username' => 'backup',
        'password' => 'pa$:word',
        'unix_socket' => null,
        'sslmode' => 'verify-full',
        'sslrootcert' => '/etc/postgres-ca.pem',
    ]);
});

test('parses encoded PostgreSQL URL credentials, socket hosts, and query overrides', function (): void {
    File::put($this->environmentFile, "DB_CONNECTION=pgsql\nDB_SSLMODE=prefer\nDB_APPLICATION_NAME=default\n");
    putenv('DB_URL=postgres://backup%40remote:pa%24%3Aword@%2Fvar%2Frun%2Fpostgresql/file%2Fbeam?port=5433&sslmode=require&sslrootcert=%2Fetc%2Fpostgres-ca.pem&application_name=backup');

    $database = Env::database($this->environmentFile);

    expect($database)->toMatchArray([
        'driver' => 'pgsql',
        'host' => '/var/run/postgresql',
        'port' => '5433',
        'database' => 'file/beam',
        'username' => 'backup@remote',
        'password' => 'pa$:word',
        'sslmode' => 'require',
        'sslrootcert' => '/etc/postgres-ca.pem',
        'application_name' => 'backup',
    ]);
});

test('normalizes database URL driver aliases and SQLite paths', function (): void {
    foreach ([
        'postgres://backup@host/filebeam' => 'pgsql',
        'postgresql://backup@host/filebeam' => 'pgsql',
        'mysql2://root@host/filebeam' => 'mysql',
        'sqlite3:///tmp/filebeam.sqlite' => 'sqlite',
    ] as $url => $driver) {
        File::put($this->environmentFile, 'DB_CONNECTION=sqlite'.PHP_EOL.'DB_URL='.$url.PHP_EOL);

        $database = Env::database($this->environmentFile);

        expect($database['driver'])->toBe($driver);
        if ($driver === 'sqlite') {
            expect($database['database'])->toBe('tmp/filebeam.sqlite');
        }
    }
});

test('rejects malformed database URLs without disclosing them', function (): void {
    File::put($this->environmentFile, "DB_CONNECTION=pgsql\nDB_URL=postgresql://backup:secret@host:99999/filebeam\n");

    expect(fn (): array => Env::database($this->environmentFile))
        ->toThrow(RuntimeException::class, 'Database configuration URL is invalid.');
});

test('preserves invalid percent sequences as Laravel URL parsing does', function (): void {
    File::put($this->environmentFile, "DB_CONNECTION=pgsql\nDB_URL=postgresql://backup@host/file%2Gbeam?application_name=backup%2Gjob\n");

    $database = Env::database($this->environmentFile);

    expect($database['database'])->toBe('file%2Gbeam')
        ->and($database['application_name'])->toBe('backup%2Gjob');
});

test('normalizes Laravel null passwords without changing quoted password content', function (): void {
    File::put($this->environmentFile, "DB_CONNECTION=pgsql\nDB_DATABASE=filebeam\nDB_USERNAME=backup\nDB_PASSWORD=null\n");

    $database = Env::database($this->environmentFile);

    expect($database['password'])->toBe('');
});

test('rejects inherited PostgreSQL target and session overrides', function (): void {
    File::put($this->environmentFile, "DB_CONNECTION=pgsql\nDB_DATABASE=filebeam\nDB_USERNAME=backup\n");

    foreach (['PGHOSTADDR', 'PGSERVICE', 'PGSERVICEFILE', 'PGOPTIONS', 'PGTARGETSESSIONATTRS', 'PGLOADBALANCEHOSTS'] as $key) {
        putenv($key.'=unexpected');
        expect(fn (): array => Env::database($this->environmentFile))
            ->toThrow(RuntimeException::class, 'Configure PostgreSQL connections with DB_* variables only.');
        putenv($key);
    }
});

test('refuses an environment that differs from cached database configuration', function (): void {
    File::put($this->environmentFile, "DB_CONNECTION=pgsql\nDB_HOST=primary.example\nDB_DATABASE=filebeam\nDB_USERNAME=backup\n");
    File::ensureDirectoryExists($this->environmentRoot.'/backend/bootstrap/cache');
    putenv('APP_CONFIG_CACHE=bootstrap/cache/live.php');
    File::put($this->environmentRoot.'/backend/bootstrap/cache/live.php', "<?php return ['database' => ['default' => 'pgsql', 'connections' => ['pgsql' => ['driver' => 'pgsql', 'host' => 'replica.example', 'port' => '5432', 'database' => 'filebeam', 'username' => 'backup', 'password' => '', 'sslmode' => 'prefer']]]];");

    expect(fn (): array => Env::database($this->environmentFile))
        ->toThrow(RuntimeException::class, 'Database configuration changed; run php artisan config:cache before updating.');
});

test('accepts cached PostgreSQL socket hosts as the same canonical endpoint', function (): void {
    File::put($this->environmentFile, "DB_CONNECTION=pgsql\nDB_SOCKET=/var/run/postgresql\nDB_DATABASE=filebeam\nDB_USERNAME=backup\n");
    File::ensureDirectoryExists($this->environmentRoot.'/backend/bootstrap/cache');
    File::put($this->environmentRoot.'/backend/bootstrap/cache/config.php', "<?php return ['database' => ['default' => 'pgsql', 'connections' => ['pgsql' => ['driver' => 'pgsql', 'host' => '/var/run/postgresql', 'port' => '5432', 'database' => 'filebeam', 'username' => 'backup', 'password' => null, 'sslmode' => 'prefer']]]];");

    $database = Env::database($this->environmentFile);

    expect($database['host'])->toBe('/var/run/postgresql')
        ->and($database['unix_socket'])->toBeNull()
        ->and($database['socket'])->toBeNull();
});
