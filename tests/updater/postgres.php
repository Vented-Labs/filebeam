<?php

declare(strict_types=1);

use Filebeam\Updater\PostgresBackup;

if (getenv('FILEBEAM_POSTGRES_BACKUP_INTEGRATION') !== '1') {
    fwrite(STDERR, "Set FILEBEAM_POSTGRES_BACKUP_INTEGRATION=1 to run the isolated PostgreSQL backup integration.\n");

    exit(77);
}

require_once __DIR__.'/../../updater/PostgresBackup.php';

$root = sys_get_temp_dir().'/filebeam-postgres-backup-'.bin2hex(random_bytes(8));
$socket = $root.'/socket';
$backup = $root.'/backup';
$container = 'filebeam-postgres-backup-'.bin2hex(random_bytes(6));
$password = 'spaces : \\ $ quotes " single \' ';
$configuration = [
    'driver' => 'pgsql',
    'socket' => $socket,
    'port' => '5432',
    'database' => 'filebeam_updater_testing',
    'username' => 'filebeam_backup',
    'password' => $password,
    'sslmode' => 'disable',
    'application_name' => 'filebeam-postgres-backup-test',
];

/** @param list<string> $command @param array<string, string> $environment */
function postgresBackupCommand(array $command, array $environment = [], bool $allowFailure = false): string
{
    $process = proc_open($command, [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes, null, $environment + (getenv() ?: []), ['bypass_shell' => true]);
    if (! is_resource($process)) {
        throw new RuntimeException('Unable to execute isolated PostgreSQL integration command.');
    }
    $output = stream_get_contents($pipes[1]).stream_get_contents($pipes[2]);
    fclose($pipes[1]);
    fclose($pipes[2]);
    if (proc_close($process) !== 0 && ! $allowFailure) {
        throw new RuntimeException('Isolated PostgreSQL integration command failed.');
    }

    return $output;
}

function dotenvValue(string $value): string
{
    return '"'.str_replace(['\\', '"', '$'], ['\\\\', '\\"', '\\$'], $value).'"';
}

function removePostgresBackupDirectory(string $path): void
{
    if (! file_exists($path) && ! is_link($path)) {
        return;
    }
    if (! is_dir($path) || is_link($path)) {
        unlink($path);

        return;
    }
    foreach (new RecursiveIteratorIterator(new RecursiveDirectoryIterator($path, FilesystemIterator::SKIP_DOTS), RecursiveIteratorIterator::CHILD_FIRST) as $entry) {
        $entry->isDir() && ! $entry->isLink() ? rmdir($entry->getPathname()) : unlink($entry->getPathname());
    }
    rmdir($path);
}

try {
    mkdir($root, 0700, true);
    mkdir($socket, 0777, true);
    mkdir($backup, 0700, true);
    postgresBackupCommand([
        'docker', 'run', '--detach', '--rm', '--name', $container,
        '--env', 'POSTGRES_DB=filebeam_updater_testing',
        '--env', 'POSTGRES_USER=filebeam_admin',
        '--env', 'POSTGRES_PASSWORD='.$password,
        '--env', 'POSTGRES_INITDB_ARGS=--auth-local=scram-sha-256 --auth-host=scram-sha-256',
        '--volume', $socket.':/var/run/postgresql',
        '--volume', $root.':'.$root,
        'postgres:16-alpine',
    ]);

    $pdo = null;
    for ($attempt = 0; $attempt < 30; $attempt++) {
        try {
            $pdo = new PDO('pgsql:host='.$socket.';port=5432;dbname=filebeam_updater_testing;sslmode=disable', 'filebeam_admin', $password, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);

            break;
        } catch (PDOException) {
            usleep(200000);
        }
    }
    if (! $pdo instanceof PDO) {
        throw new RuntimeException('Isolated PostgreSQL did not become ready.');
    }
    $pdo->exec('CREATE ROLE filebeam_backup LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD '.$pdo->quote($password));
    $pdo->exec('ALTER DATABASE filebeam_updater_testing OWNER TO filebeam_backup');
    $pdo->exec('CREATE DATABASE filebeam_updater_restore_testing OWNER filebeam_backup');
    $pdo = new PDO('pgsql:host='.$socket.';port=5432;dbname=filebeam_updater_testing;sslmode=disable', 'filebeam_backup', $password, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
    $pdo->exec('CREATE TABLE restore_probe (id bigserial PRIMARY KEY, enabled boolean NOT NULL, payload jsonb NOT NULL, data bytea NOT NULL, code integer NOT NULL UNIQUE, CONSTRAINT restore_probe_enabled CHECK (enabled))');
    $pdo->exec('CREATE INDEX restore_probe_payload_index ON restore_probe USING gin (payload)');
    $statement = $pdo->prepare("INSERT INTO restore_probe (enabled, payload, data, code) VALUES (true, ?::jsonb, decode('0062696e617279ff', 'hex'), 7)");
    $statement->execute(['{"name":"filebeam","nested":{"answer":42}}']);

    $wrapper = $root.'/pg-tool';
    file_put_contents($wrapper, "#!/bin/sh\nexec docker exec --user 0 --env PGPASSFILE --env PGSSLMODE --env PGSSLROOTCERT --env PGSSLCERT --env PGSSLKEY --env PGAPPNAME ".escapeshellarg($container)." /usr/local/bin/\$(basename \"\$0\") \"\$@\"\n");
    chmod($wrapper, 0700);
    $dump = $root.'/pg_dump';
    $restore = $root.'/pg_restore';
    symlink($wrapper, $dump);
    symlink($wrapper, $restore);

    $cache = $root.'/cache';
    mkdir($cache, 0700, true);
    postgresBackupCommand([PHP_BINARY, __DIR__.'/../../backend/artisan', 'migrate', '--force', '--no-interaction'], [
        'APP_ENV' => 'testing',
        'APP_CONFIG_CACHE' => $cache.'/config.php',
        'APP_EVENTS_CACHE' => $cache.'/events.php',
        'APP_PACKAGES_CACHE' => $cache.'/packages.php',
        'APP_ROUTES_CACHE' => $cache.'/routes.php',
        'APP_SERVICES_CACHE' => $cache.'/services.php',
        'DB_CONNECTION' => 'pgsql',
        'DB_DATABASE' => 'filebeam_updater_testing',
        'DB_HOST' => $socket,
        'DB_SOCKET' => $socket,
        'DB_PORT' => '5432',
        'DB_USERNAME' => 'filebeam_backup',
        'DB_PASSWORD' => $password,
        'DB_SSLMODE' => 'disable',
        'DB_URL' => '',
        'CACHE_STORE' => 'array',
        'QUEUE_CONNECTION' => 'sync',
    ]);

    $helper = new PostgresBackup($configuration, $backup, $dump, $restore, 30);
    $helper->preflight();
    $archive = $helper->backup();
    if (file_get_contents($archive, false, null, 0, 5) !== 'PGDMP' || (fileperms($archive) & 0777) !== 0600) {
        throw new RuntimeException('Helper did not produce a private custom PostgreSQL archive.');
    }

    $passfile = $root.'/.pgpass';
    file_put_contents($passfile, str_replace(['\\', ':'], ['\\\\', '\\:'], $socket).':5432:filebeam_updater_restore_testing:filebeam_backup:'.str_replace(['\\', ':'], ['\\\\', '\\:'], $password)."\n");
    chmod($passfile, 0600);
    $pdo->exec('TRUNCATE restore_probe RESTART IDENTITY');
    postgresBackupCommand([$restore, '--no-password', '--no-owner', '--no-acl', '--exit-on-error', '--host='.$socket, '--port=5432', '--username=filebeam_backup', '--dbname=filebeam_updater_restore_testing', $archive], ['PGPASSFILE' => $passfile, 'PGSSLMODE' => 'disable']);
    $pdo = new PDO('pgsql:host='.$socket.';port=5432;dbname=filebeam_updater_restore_testing;sslmode=disable', 'filebeam_backup', $password, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
    $row = $pdo->query("SELECT id, enabled, payload->>'name' AS name, payload->'nested'->>'answer' AS answer, encode(data, 'hex') AS data, code FROM restore_probe")?->fetch(PDO::FETCH_ASSOC);
    $next = $pdo->query("SELECT nextval(pg_get_serial_sequence('restore_probe', 'id'))")?->fetchColumn();
    $index = $pdo->query("SELECT 1 FROM pg_indexes WHERE tablename = 'restore_probe' AND indexname = 'restore_probe_payload_index'")?->fetchColumn();
    $constraint = $pdo->query("SELECT 1 FROM pg_constraint WHERE conname = 'restore_probe_enabled'")?->fetchColumn();
    if (($row['id'] ?? null) !== 1 || ($row['enabled'] ?? null) !== true || ($row['name'] ?? null) !== 'filebeam' || ($row['answer'] ?? null) !== '42' || ($row['data'] ?? null) !== '0062696e617279ff' || ($row['code'] ?? null) !== 7 || (int) $next !== 2 || (int) $index !== 1 || (int) $constraint !== 1) {
        throw new RuntimeException('PostgreSQL restore did not preserve application data and schema objects.');
    }

    $mismatch = $root.'/pg_dump-17';
    file_put_contents($mismatch, "#!/bin/sh\nif [ \"\$1\" = '--version' ]; then echo 'pg_dump (PostgreSQL) 17.0'; exit 0; fi\nexec ".escapeshellarg($dump)." \"\$@\"\n");
    chmod($mismatch, 0700);
    try {
        (new PostgresBackup($configuration, $backup, $mismatch, $restore, 30))->preflight();
        throw new RuntimeException('Major-version mismatch was accepted.');
    } catch (RuntimeException $exception) {
        if ($exception->getMessage() !== 'PostgreSQL client tools must match the server major version.') {
            throw $exception;
        }
    }

    $package = $root.'/package';
    mkdir($package.'/backend', 0700, true);
    mkdir($package.'/bin', 0700, true);
    symlink(__DIR__.'/../../backend/vendor', $package.'/backend/vendor');
    symlink(__DIR__.'/../../updater', $package.'/updater');
    copy(__DIR__.'/../../update.php', $package.'/update.php');
    symlink($dump, $package.'/bin/pg_dump');
    symlink($restore, $package.'/bin/pg_restore');
    file_put_contents($package.'/backend/.env', implode("\n", [
        'DB_CONNECTION=pgsql',
        'DB_URL=""',
        'DB_DATABASE=filebeam_updater_testing',
        'DB_HOST=unused.invalid',
        'DB_SOCKET='.dotenvValue($socket),
        'DB_PORT=5432',
        'DB_USERNAME=filebeam_backup',
        'DB_PASSWORD='.dotenvValue($password),
        'DB_SSLMODE=disable',
        'DB_APPLICATION_NAME=filebeam-postgres-backup-test',
        '',
    ]));
    $nativeEnvironment = [
        'APP_CONFIG_CACHE' => $package.'/cache/config.php',
        'DB_CONNECTION' => 'pgsql',
        'DB_URL' => '',
        'DB_DATABASE' => 'filebeam_updater_testing',
        'DB_HOST' => 'unsafe-host.invalid',
        'DB_SOCKET' => $socket,
        'DB_PORT' => '5432',
        'DB_USERNAME' => 'filebeam_backup',
        'DB_PASSWORD' => $password,
        'DB_SSLMODE' => 'disable',
        'DB_APPLICATION_NAME' => 'filebeam-postgres-backup-test',
        'PATH' => $package.'/bin:'.getenv('PATH'),
    ];
    postgresBackupCommand([PHP_BINARY, $package.'/update.php', '--check-backup'], $nativeEnvironment);
    $nativeBackup = $package.'/.filebeam/database-backups';
    if (is_dir($nativeBackup) && glob($nativeBackup.'/*') !== []) {
        throw new RuntimeException('Native backup preflight created an archive.');
    }
    mkdir($package.'/bad-bin', 0700, true);
    file_put_contents($package.'/bad-bin/pg_dump', "#!/bin/sh\necho 'pg_dump (PostgreSQL) 17.0'\n");
    chmod($package.'/bad-bin/pg_dump', 0700);
    symlink($restore, $package.'/bad-bin/pg_restore');
    $failure = postgresBackupCommand([PHP_BINARY, $package.'/update.php', '--check-backup'], array_replace($nativeEnvironment, ['PATH' => $package.'/bad-bin:'.$package.'/bin:'.getenv('PATH')]), true);
    if (! str_contains($failure, 'PostgreSQL client tools must match the server major version.') || str_contains($failure, $password)) {
        throw new RuntimeException('Native backup preflight did not safely reject a mismatched client tool.');
    }
    foreach (['PGHOSTADDR' => '127.0.0.1', 'PGSERVICE' => 'unexpected-service'] as $key => $value) {
        $previous = getenv($key);
        putenv($key.'='.$value);
        try {
            (new PostgresBackup($configuration, $backup, $dump, $restore, 30))->preflight();
            throw new RuntimeException('Unsafe PostgreSQL client environment was accepted.');
        } catch (RuntimeException $exception) {
            if ($exception->getMessage() !== 'PostgreSQL client environment is unsafe for backup preflight.') {
                throw $exception;
            }
        } finally {
            putenv($previous === false ? $key : $key.'='.$previous);
        }
    }

    fwrite(STDOUT, "PostgreSQL 16 isolated backup, restore, and native preflight integration passed.\n");
} finally {
    try {
        postgresBackupCommand(['docker', 'rm', '--force', $container]);
    } catch (Throwable) {
    }
    try {
        postgresBackupCommand(['docker', 'run', '--rm', '--user', '0', '--volume', $socket.':/socket', 'alpine:3.22', 'sh', '-c', 'rm -rf /socket/* /socket/.[!.]* /socket/..?*']);
    } catch (Throwable) {
    }
    removePostgresBackupDirectory($root);
}
