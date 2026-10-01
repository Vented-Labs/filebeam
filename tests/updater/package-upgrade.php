<?php

declare(strict_types=1);

use Filebeam\Updater\Package;
use Symfony\Component\Process\Process;

/** @param array<string, mixed> $database */
function testPublishedPackageUpgrade(string $root, string $baseline, string $candidate, array $database, string $dump, string $restore): void
{
    require_once __DIR__.'/../../backend/vendor/autoload.php';
    require_once __DIR__.'/../../updater/Updater.php';
    $work = $root.'/upgrade';
    mkdir($work, 0700);
    $installation = $work."/installation's directory";
    $source = $work.'/candidate';
    $releases = $work.'/releases';
    foreach ([$installation, $source, $releases, $work.'/ini', $work.'/bin', $work.'/failing-bin'] as $directory) {
        mkdir($directory, 0700, true);
    }
    Package::extract($baseline, $installation, 4294967296);
    Package::extract($candidate, $source, 4294967296);
    $installation .= '/filebeam';
    $source .= '/filebeam';
    Package::verify($installation, Package::manifest($installation));
    Package::verify($source, Package::manifest($source));
    $originalUpdater = hash_file('sha256', $installation.'/updater/Updater.php');
    $pair = sodium_crypto_sign_keypair();
    $public = base64_encode(sodium_crypto_sign_publickey($pair));
    $secret = sodium_crypto_sign_secretkey($pair);

    $setVersion = static function (string $directory, ?string $number = null) use ($public): array {
        $file = $directory.'/backend/config/version.php';
        $version = require $file;
        $version['update_public_key'] = $public;
        if ($number !== null) {
            $version['version'] = $number;
            $version['tag'] = 'v'.$number;
        }
        file_put_contents($file, '<?php return '.var_export($version, true).';');
        $manifest = Package::manifest($directory);
        $manifest['files']['backend/config/version.php'] = hash_file('sha256', $file);
        file_put_contents($directory.'/package-files.json', json_encode($manifest, JSON_THROW_ON_ERROR));

        return $version;
    };
    $setVersion($installation);
    if (hash_file('sha256', $installation.'/updater/Updater.php') !== $originalUpdater) {
        throw new RuntimeException('Published updater code was modified by the fixture.');
    }

    $port = static function (): int {
        $socket = stream_socket_server('tcp://127.0.0.1:0', $error, $message);
        if ($socket === false) {
            throw new RuntimeException('Cannot reserve a fixture port.');
        }
        $address = stream_socket_get_name($socket, false);
        fclose($socket);

        return (int) substr($address, strrpos($address, ':') + 1);
    };
    $appUrl = 'http://127.0.0.1:'.$port();
    $releaseUrl = 'http://127.0.0.1:'.$port();
    file_put_contents($work.'/ini/99-updater-fixture.ini', 'auto_prepend_file="'.__DIR__.'/fixtures/release-transport.php"'."\n");
    symlink($dump, $work.'/bin/pg_dump');
    symlink($restore, $work.'/bin/pg_restore');
    file_put_contents($work.'/failing-bin/pg_dump', "#!/bin/sh\nfor arg in \"\$@\"; do\ncase \"\$arg\" in --version|--schema-only) exec ".escapeshellarg($dump)." \"\$@\";; esac\ndone\nprintf 'injected data backup failure\\n' >&2\nexit 9\n");
    chmod($work.'/failing-bin/pg_dump', 0700);
    $environment = [
        'APP_ENV' => 'production', 'APP_DEBUG' => 'false', 'APP_URL' => $appUrl,
        'APP_KEY' => 'base64:'.base64_encode(random_bytes(32)),
        'DB_CONNECTION' => 'pgsql', 'DB_URL' => '', 'DB_SOCKET' => $database['socket'],
        'DB_HOST' => $database['socket'], 'DB_PORT' => $database['port'],
        'DB_DATABASE' => $database['database'], 'DB_USERNAME' => $database['username'], 'DB_PASSWORD' => $database['password'],
        'DB_SSLMODE' => 'disable', 'CACHE_STORE' => 'file', 'SESSION_DRIVER' => 'file',
        'QUEUE_CONNECTION' => 'sync', 'FILEBEAM_CRON_QUEUE_ENABLED' => 'false',
        'FILEBEAM_AUTO_UPDATES_ENABLED' => 'false', 'INERTIA_SSR_ENABLED' => 'false',
        'APP_CONFIG_CACHE' => $installation.'/backend/bootstrap/cache/config.php',
        'PHP_INI_SCAN_DIR' => (getenv('PHP_INI_SCAN_DIR') ?: '').PATH_SEPARATOR.$work.'/ini',
        'FILEBEAM_UPDATER_FIXTURE_URL' => $releaseUrl.'/',
        'PATH' => $work.'/bin:'.getenv('PATH'),
    ];
    $lines = [];
    foreach ($environment as $key => $value) {
        if (! in_array($key, ['PATH', 'PHP_INI_SCAN_DIR', 'FILEBEAM_UPDATER_FIXTURE_URL', 'APP_CONFIG_CACHE'], true)) {
            $lines[] = $key.'='.dotenvValue($value);
        }
    }
    file_put_contents($installation.'/backend/.env', implode("\n", $lines)."\n");
    chmod($installation.'/backend/.env', 0600);
    $private = $installation.'/backend/storage/app/private/upgrade-fixture';
    if (! is_dir(dirname($private))) {
        mkdir(dirname($private), 0700, true);
    }
    if (file_put_contents($private, random_bytes(128)) !== 128) {
        throw new RuntimeException('Cannot prepare private upgrade fixture.');
    }
    $privateHash = hash_file('sha256', $private);
    $environmentHash = hash_file('sha256', $installation.'/backend/.env');
    if ($privateHash === false || $environmentHash === false) {
        throw new RuntimeException('Cannot fingerprint private upgrade fixture.');
    }

    $run = static function (array $arguments, array $overrides = [], bool $allowFailure = false) use ($installation, $environment): Process {
        $process = new Process([PHP_BINARY, ...$arguments], $installation.'/backend', array_replace($environment, $overrides), null, 180);
        $process->run();
        if (! $allowFailure && ! $process->isSuccessful()) {
            throw new RuntimeException('Package upgrade command failed: '.substr($process->getErrorOutput().$process->getOutput(), 0, 4096));
        }

        return $process;
    };
    $run(['artisan', 'migrate', '--force', '--no-interaction']);
    $pdo = new PDO('pgsql:host='.$database['socket'].';port='.$database['port'].';dbname='.$database['database'], $database['username'], $database['password'], [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
    $pdo->exec('CREATE TABLE updater_upgrade_sentinel (id integer PRIMARY KEY, value text NOT NULL)');
    $pdo->exec("INSERT INTO updater_upgrade_sentinel VALUES (1, 'preserved')");

    $publish = static function (string $number, bool $badMigration = false) use ($source, $releases, $setVersion, $secret): void {
        $version = $setVersion($source, $number);
        $manifest = Package::manifest($source);
        if ($badMigration) {
            $path = 'backend/database/migrations/2099_01_01_000000_updater_failure_fixture.php';
            file_put_contents($source.'/'.$path, '<?php return new class extends \\Illuminate\\Database\\Migrations\\Migration { public function up(): void { throw new \\RuntimeException("injected migration failure"); } public function down(): void {} };');
            $manifest['files'][$path] = hash_file('sha256', $source.'/'.$path);
            file_put_contents($source.'/package-files.json', json_encode($manifest, JSON_THROW_ON_ERROR));
        }
        $tag = 'v'.$number;
        $path = 'versions/'.$tag.'/filebeam-'.$tag.'.zip';
        mkdir(dirname($releases.'/'.$path), 0700, true);
        $zip = new ZipArchive;
        if ($zip->open($releases.'/'.$path, ZipArchive::CREATE) !== true) {
            throw new RuntimeException('Cannot create fixture release.');
        }
        foreach ([...array_keys($manifest['files']), 'package-files.json'] as $file) {
            $zip->addFile($source.'/'.$file, 'filebeam/'.$file);
        }
        $zip->close();
        $payload = json_encode(['schema' => 1, 'generation' => 1 + (int) explode('.', $number)[2],
            'published_at' => gmdate('c'), 'expires_at' => gmdate('c', time() + 3600),
            'releases' => [['tag' => $tag, 'version' => $number, 'built_at' => $version['built_at'],
                'package' => ['path' => $path, 'sha256' => hash_file('sha256', $releases.'/'.$path), 'size' => filesize($releases.'/'.$path)]]],
        ], JSON_THROW_ON_ERROR);
        file_put_contents($releases.'/index.json', json_encode(['signed' => base64_encode($payload), 'signature' => base64_encode(sodium_crypto_sign_detached($payload, $secret))], JSON_THROW_ON_ERROR));
    };
    $web = new Process([PHP_BINARY, '-S', substr($appUrl, 7), '-t', $installation.'/backend/public'], $installation.'/backend', $environment);
    $releaseServer = new Process([PHP_BINARY, '-S', substr($releaseUrl, 7), '-t', $releases], $work, $environment);
    $status = static function () use ($installation): array {
        return json_decode(file_get_contents($installation.'/.filebeam/status.json'), true, flags: JSON_THROW_ON_ERROR);
    };
    $assertPreserved = static function () use ($installation, $environmentHash, $private, $privateHash, $pdo): void {
        if (hash_file('sha256', $installation.'/backend/.env') !== $environmentHash || hash_file('sha256', $private) !== $privateHash
            || $pdo->query('SELECT value FROM updater_upgrade_sentinel WHERE id = 1')->fetchColumn() !== 'preserved') {
            throw new RuntimeException('An upgrade modified private configuration, storage, or database content.');
        }
    };
    try {
        $web->start();
        $releaseServer->start();
        $publish('99.0.0');
        $run([$installation.'/update.php', 'v99.0.0']);
        if (($status()['state'] ?? null) !== 'complete') {
            throw new RuntimeException('Published-package upgrade did not finish.');
        }
        $assertPreserved();
        Package::verify($installation, Package::manifest($installation));
        $publish('99.0.1');
        file_put_contents($installation.'/.filebeam/release-check.json', '{"state":"available","latest":{"tag":"v99.0.1","upgradeable":true}}');
        file_put_contents($installation.'/.filebeam/pending.json', '{"tag":"v99.0.1"}');
        $run(['artisan', 'schedule:run'], ['PATH' => $work.'/failing-bin:'.$environment['PATH']], true);
        $failed = $status();
        if (($failed['state'] ?? null) !== 'failed' || ($failed['recovered'] ?? false) !== true || ! str_contains($failed['error'], 'pg_dump backup')
            || is_file($installation.'/backend/storage/framework/down') || is_file($installation.'/.filebeam/journal.json') || is_file($installation.'/.filebeam/pending.json')) {
            throw new RuntimeException('Pre-replacement failure did not safely restore service and stop automatic retries.');
        }
        $assertPreserved();
        file_put_contents($installation.'/.filebeam/pending.json', '{"tag":"v99.0.1"}');
        $run(['artisan', 'schedule:run']);
        $heartbeat = json_decode(file_get_contents($installation.'/.filebeam/heartbeat.json'), true, flags: JSON_THROW_ON_ERROR);
        if (($status()['state'] ?? null) !== 'complete' || ($status()['tag'] ?? null) !== 'v99.0.1' || $heartbeat['state'] !== 'complete'
            || is_file($installation.'/.filebeam/release-check.json')) {
            throw new RuntimeException('Laravel cron did not complete the queued package upgrade.');
        }
        $assertPreserved();
        $publish('99.0.2', true);
        $run([$installation.'/update.php', 'v99.0.2'], [], true);
        $journal = json_decode(file_get_contents($installation.'/.filebeam/journal.json'), true, flags: JSON_THROW_ON_ERROR);
        if ($journal['phase'] !== 'migrating' || ! is_file($installation.'/backend/storage/framework/down') || ! is_file($journal['database_backup'])
            || $run([$installation.'/update.php', '--recover'], [], true)->isSuccessful()) {
            throw new RuntimeException('Migration failure did not retain maintenance mode and a database backup.');
        }
        $assertPreserved();
        echo "Published-package upgrade, real cron, safe backup-failure recovery, and migration fail-closed checks passed.\n";
    } finally {
        $web->stop(2);
        $releaseServer->stop(2);
    }
}
