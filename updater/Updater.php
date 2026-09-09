<?php

declare(strict_types=1);

namespace Filebeam\Updater;

use Dotenv\Dotenv;
use PDO;
use RuntimeException;
use ZipArchive;

final class Command
{
    private const INDEX_URL = 'https://releases.filebeam.io/index.json';

    private const MAX_ARCHIVE_BYTES = 1073741824;

    private const MAX_UNPACKED_BYTES = 4294967296;

    /** @param list<string> $argv */
    public static function run(array $argv, string $root): int
    {
        if (PHP_SAPI !== 'cli') {
            fwrite(STDERR, "This updater must be run from the CLI.\n");

            return 64;
        }

        try {
            $state = new State($root);
            $argument = $argv[1] ?? null;
            if (in_array($argument, ['--help', '-h'], true)) {
                self::help();

                return 0;
            }
            if ($argument === '--check-backup') {
                (new self($root, $state))->databasePreflight();
                self::write('Database backup prerequisites verified.');

                return 0;
            }
            if ($argument === '--status') {
                $status = json_encode($state->read('status.json') ?? ['state' => 'idle'], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);
                self::write($status === false ? throw new RuntimeException('Cannot encode updater status.') : $status);

                return 0;
            }
            if ($argument === '--recover') {
                $lock = $state->lock();
                try {
                    (new Recovery($root, $state))->run();
                    self::write('Recovery complete.');
                } finally {
                    flock($lock, LOCK_UN);
                    fclose($lock);
                }

                return 0;
            }
            $fromCron = $argument === '--cron';
            if ($fromCron) {
                $argument = null;
            }
            if ($argument !== null && str_starts_with($argument, '--')) {
                throw new RuntimeException('Unknown option. Use --help.');
            }

            $updater = new self($root, $state);
            $updater->update($argument, $fromCron);

            return 0;
        } catch (\Throwable $e) {
            fwrite(STDERR, 'Update failed: '.$e->getMessage()."\n");

            return 1;
        }
    }

    private function __construct(private readonly string $root, private readonly State $state) {}

    private function update(?string $requestedTag, bool $fromCron): void
    {
        $version = $this->version();
        if (($version['distribution'] ?? null) !== 'package' || is_file('/.dockerenv')) {
            throw new RuntimeException('Self-updates are supported only by package installations outside containers.');
        }
        if (! extension_loaded('sodium') || ! extension_loaded('zip') || ! function_exists('curl_init')) {
            throw new RuntimeException('The sodium, zip, and curl PHP extensions are required.');
        }
        $key = base64_decode((string) ($version['update_public_key'] ?? ''), true);
        if ($key === false || strlen($key) !== SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES) {
            throw new RuntimeException('This installation has no valid update public key.');
        }
        $lock = $this->state->lock();
        require_once __DIR__.'/ActivityLock.php';
        $activity = new ActivityLock($this->root.'/backend/storage/app/update-activity.lock');
        $exclusive = null;
        try {
            if ($this->state->read('journal.json') !== null) {
                throw new RuntimeException('An interrupted update journal exists. Run php update.php --recover or complete the documented manual recovery before another update.');
            }
            if ($fromCron) {
                $this->state->write('heartbeat.json', ['at' => gmdate('c'), 'pid' => getmypid()]);
                $pending = $this->state->read('pending.json');
                if ($pending === null) {
                    return;
                }
                $requestedTag = $pending['tag'] ?? throw new RuntimeException('Pending update has no tag.');
            }
            $this->state->write('status.json', ['state' => 'checking', 'at' => gmdate('c')]);
            $catalog = $this->catalog($key);
            $release = $this->release($catalog, $requestedTag, (string) $version['version']);
            $this->requirements($release);
            $database = $this->databasePreflight();
            $probe = $this->prepareWebProbe();
            $this->verifyWeb($version, $probe);
            $this->state->write('status.json', ['state' => 'downloading', 'tag' => $release['tag'], 'at' => gmdate('c')]);
            $archive = $this->download($release);
            $stage = $this->extract($archive, (string) $release['tag']);
            $manifest = Package::manifest($stage);
            Package::verify($stage, $manifest);

            $old = is_file($this->root.'/package-files.json') ? Package::manifest($this->root) : ['files' => []];
            $backup = $this->state->path('backups/'.gmdate('YmdHis').'-'.bin2hex(random_bytes(3)));
            if (! is_dir($backup) && ! mkdir($backup, 0700, true) && ! is_dir($backup)) {
                throw new RuntimeException('Unable to create code backup directory.');
            }
            $journal = [
                'schema' => 1,
                'tag' => $release['tag'],
                'phase' => 'prepared',
                'started_at' => gmdate('c'),
                'backup' => $backup,
                'database_backup' => null,
                'old_files' => array_merge(array_keys($old['files']), is_file($this->root.'/package-files.json') ? ['package-files.json'] : []),
                'new_files' => array_merge(array_keys($manifest['files']), ['package-files.json']),
            ];
            $this->state->write('journal.json', $journal);
            $this->artisan(['down', '--retry=60', '--secret='.$probe['token']]);
            $journal['phase'] = 'maintenance';
            $this->state->write('journal.json', $journal);
            $this->artisan(['queue:restart']);
            $journal['phase'] = 'draining';
            $this->state->write('journal.json', $journal);
            $exclusive = $activity->acquireExclusive(120);
            $this->clearBootstrapCaches();
            $journal['database_backup'] = $this->backupDatabase($database);
            $this->state->write('journal.json', $journal);
            $this->replace($stage, $manifest, $old, $backup);
            if (Env::database($this->root.'/backend/.env') !== $database) {
                throw new RuntimeException('Database configuration changed during the update. Recover before retrying.');
            }
            $journal['phase'] = 'migrating';
            $this->state->write('journal.json', $journal);
            $this->artisan(['migrate', '--force', '--no-interaction']);
            $this->artisan(['queue:restart']);
            $this->verifyWeb($release, $probe, true);
            $this->artisan(['up']);
            $this->verifyWeb($release, $probe);
            if ($fromCron) {
                $this->state->remove('pending.json');
            }
            $this->state->remove('journal.json');
            $this->state->remove('probe.json');
            $this->state->write('status.json', ['state' => 'complete', 'tag' => $release['tag'], 'at' => gmdate('c')]);
            self::write('Updated to '.$release['tag'].'.');
        } catch (\Throwable $e) {
            if (isset($journal) && in_array($journal['phase'], ['migrating', 'maintenance', 'draining'], true)) {
                try {
                    $this->artisan(['down', '--retry=60', '--secret='.($probe['token'] ?? bin2hex(random_bytes(32)))]);
                } catch (\Throwable) {
                }
            }
            $this->state->write('status.json', ['state' => 'failed', 'error' => $e->getMessage(), 'at' => gmdate('c')]);
            throw $e;
        } finally {
            if (is_resource($exclusive)) {
                $activity->release($exclusive);
            }
            flock($lock, LOCK_UN);
            fclose($lock);
        }
    }

    /** @return array<string, mixed> */
    private function version(): array
    {
        $file = $this->root.'/backend/config/version.php';
        if (! is_file($file)) {
            throw new RuntimeException('Missing backend/config/version.php. This is not a packaged installation.');
        }
        $value = require $file;
        if (! is_array($value) || ! isset($value['version'], $value['distribution'])) {
            throw new RuntimeException('Invalid version metadata.');
        }

        return $value;
    }

    /** @return array{schema: int, generation: int, published_at?: string, expires_at: string, releases: array<int, mixed>} */
    private function catalog(string $key): array
    {
        $raw = Http::get(self::INDEX_URL, 1048576);
        $catalog = Catalog::verify($raw, $key);
        $previous = $this->state->read('catalog.json');
        if (is_array($previous) && (! is_int($previous['generation'] ?? null) || $catalog['generation'] < $previous['generation'])) {
            throw new RuntimeException('Release catalog rollback detected.');
        }
        $this->state->write('catalog.json', ['generation' => $catalog['generation'], 'published_at' => $catalog['published_at'] ?? null]);

        return $catalog;
    }

    /**
     * @param  array{schema: int, generation: int, published_at?: string, expires_at: string, releases: array<int, mixed>}  $catalog
     * @return array<string, mixed>
     */
    private function release(array $catalog, ?string $tag, string $current): array
    {
        $wanted = $tag === null ? null : Semver::parseTag($tag, true);
        $candidates = [];
        foreach ($catalog['releases'] as $release) {
            if (! is_array($release) || ! isset($release['tag'], $release['version'], $release['package'])) {
                continue;
            }
            $parsed = Semver::parseTag((string) $release['tag'], true);
            if ($parsed === null || Semver::parseTag((string) $release['version'], true) !== $parsed || ($release['withdrawn'] ?? false)) {
                continue;
            }
            if ($wanted !== null ? $parsed !== $wanted : str_contains($parsed, '-')) {
                continue;
            }
            $candidates[] = $release;
        }
        usort($candidates, fn ($a, $b) => Semver::compare((string) $b['version'], (string) $a['version']));
        $release = $candidates[0] ?? throw new RuntimeException($tag ? 'Requested release is unavailable.' : 'No stable release is available.');
        if (Semver::compare((string) $release['version'], $current) <= 0) {
            throw new RuntimeException('Refusing to install the current version or downgrade.');
        }
        if (($release['minimum_updater'] ?? 1) > 1 || Semver::compare($current, (string) ($release['minimum_version'] ?? '0.0.0')) < 0) {
            throw new RuntimeException('This release requires a newer updater or a newer intermediate Filebeam version.');
        }

        return $release;
    }

    /** @param array<string, mixed> $release */
    private function requirements(array $release): void
    {
        $package = $release['package'];
        $expectedPath = 'versions/'.$release['tag'].'/filebeam-'.$release['tag'].'.zip';
        if (! is_string($release['built_at'] ?? null) || strtotime($release['built_at']) === false || ! is_array($package) || ($package['path'] ?? null) !== $expectedPath || ! preg_match('/^[a-f0-9]{64}$/', (string) ($package['sha256'] ?? '')) || ! is_int($package['size'] ?? null) || $package['size'] < 1 || $package['size'] > self::MAX_ARCHIVE_BYTES) {
            throw new RuntimeException('Release package metadata is invalid.');
        }
        $requirements = $release['requirements'] ?? [];
        if (! is_array($requirements) || (isset($requirements['php']) && version_compare(PHP_VERSION, (string) $requirements['php'], '<'))) {
            throw new RuntimeException('This release requires a newer PHP version.');
        }
        foreach (($requirements['extensions'] ?? []) as $extension) {
            if (! is_string($extension) || ! extension_loaded($extension)) {
                throw new RuntimeException('Missing required PHP extension: '.$extension);
            }
        }
    }

    /** @param array<string, mixed> $release */
    private function download(array $release): string
    {
        $path = $release['package']['path'];
        $file = $this->state->path('downloads/'.basename($path));
        Http::download('https://releases.filebeam.io/'.$path, $file, $release['package']['size']);
        if (hash_file('sha256', $file) !== $release['package']['sha256']) {
            throw new RuntimeException('Downloaded archive checksum does not match the catalog.');
        }

        return $file;
    }

    private function extract(string $archive, string $tag): string
    {
        $stage = $this->state->path('stage/'.preg_replace('/[^A-Za-z0-9.-]/', '_', $tag).'-'.bin2hex(random_bytes(4)));
        mkdir($stage, 0700, true);
        Package::extract($archive, $stage, self::MAX_UNPACKED_BYTES);

        return $stage.'/filebeam';
    }

    /**
     * @param  array{schema: 1, files: array<string, string>}  $manifest
     * @param  array{files: array<string, string>}  $old
     */
    private function replace(string $stage, array $manifest, array $old, string $backup): void
    {
        $paths = array_unique(array_merge(array_keys($old['files']), array_keys($manifest['files']), ['package-files.json']));
        foreach ($paths as $relative) {
            if (! Package::safePath($relative)) {
                throw new RuntimeException('Package manifest contains an unsafe path.');
            }
            if (Package::protected($relative)) {
                continue;
            }
            $source = $stage.'/'.$relative;
            $target = $this->root.'/'.$relative;
            if (is_file($target) || is_link($target)) {
                $saved = $backup.'/'.$relative;
                if (! is_dir(dirname($saved)) && ! mkdir(dirname($saved), 0700, true) && ! is_dir(dirname($saved))) {
                    throw new RuntimeException('Unable to create code backup directory.');
                }
                if (! copy($target, $saved)) {
                    throw new RuntimeException('Unable to back up '.$relative);
                }
            }
            if (! isset($manifest['files'][$relative]) && $relative !== 'package-files.json') {
                if (file_exists($target) && ! unlink($target)) {
                    throw new RuntimeException('Unable to remove obsolete '.$relative);
                }

                continue;
            }
            if (! is_dir(dirname($target)) && ! mkdir(dirname($target), 0755, true) && ! is_dir(dirname($target))) {
                throw new RuntimeException('Unable to create install directory.');
            }
            $temporary = $target.'.filebeam-new';
            if (! copy($source, $temporary) || ! rename($temporary, $target)) {
                throw new RuntimeException('Unable to install '.$relative);
            }
        }
    }

    /** @return array<string, mixed> */
    private function databasePreflight(): array
    {
        $database = Env::database($this->root.'/backend/.env');
        $driver = $database['driver'];
        if ($driver === 'pgsql') {
            require_once __DIR__.'/PostgresBackup.php';
            (new PostgresBackup($database, $this->state->path('database-backups')))->preflight();
        }
        if ($driver === 'sqlite' && ! extension_loaded('pdo_sqlite')) {
            throw new RuntimeException('The pdo_sqlite PHP extension is required for SQLite upgrades.');
        }
        if (in_array($driver, ['mysql', 'mariadb'], true) && ! extension_loaded('pdo_mysql')) {
            throw new RuntimeException('The pdo_mysql PHP extension is required for MySQL upgrades.');
        }
        if (in_array($driver, ['mysql', 'mariadb'], true) && ! $this->commandAvailable('mysqldump')) {
            throw new RuntimeException('mysqldump is required for safe MySQL upgrades.');
        }

        return $database;
    }

    /** @param array<string, mixed> $database */
    private function backupDatabase(array $database): string
    {
        $driver = $database['driver'];
        $dir = $this->state->path('database-backups');
        if (! is_dir($dir) && ! mkdir($dir, 0700, true) && ! is_dir($dir)) {
            throw new RuntimeException('Unable to create database backup directory.');
        }
        if ($driver === 'sqlite') {
            $database = $database['database'];
            if (! str_starts_with($database, '/')) {
                $database = $this->root.'/backend/'.$database;
            }
            if (! is_file($database)) {
                throw new RuntimeException('SQLite database does not exist.');
            }
            $pdo = new PDO('sqlite:'.$database, null, null, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]);
            $target = $dir.'/database-'.gmdate('YmdHis').'.sqlite';
            $pdo->exec("VACUUM INTO '".str_replace("'", "''", $target)."'");

            return $target;
        }
        if ($driver === 'pgsql') {
            return (new PostgresBackup($database, $dir))->backup();
        }
        if (! in_array($driver, ['mysql', 'mariadb'], true)) {
            throw new RuntimeException('Unsupported database driver: '.$driver);
        }
        $file = $dir.'/database-'.gmdate('YmdHis').'.sql';
        $out = fopen($file, 'xb') ?: throw new RuntimeException('Unable to create database backup.');
        $command = ['mysqldump', '--single-transaction', '--quick', '--routines', '--events', '--triggers', '--add-drop-table', '--no-tablespaces'];
        if ($database['unix_socket'] !== null) {
            $command[] = '--socket='.$database['unix_socket'];
        } else {
            $command[] = '--host='.$database['host'];
            $command[] = '--port='.$database['port'];
        }
        $command[] = '--user='.$database['username'];
        $command[] = $database['database'];
        $process = proc_open($command, [0 => ['pipe', 'r'], 1 => $out, 2 => ['pipe', 'w']], $pipes, null, ['MYSQL_PWD' => $database['password'], 'PATH' => getenv('PATH') ?: '']);
        if (! is_resource($process)) {
            fclose($out);
            unlink($file);
            throw new RuntimeException('mysqldump is required for safe MySQL upgrades.');
        }
        try {
            fclose($pipes[0]);
            $error = stream_get_contents($pipes[2]);
            fclose($pipes[2]);
            $exit = proc_close($process);
            fclose($out);
            if ($exit !== 0 || filesize($file) === 0) {
                unlink($file);
                throw new RuntimeException('mysqldump failed'.($error ? ': '.trim($error) : '.'));
            }

            return $file;
        } catch (\Throwable $e) {
            if (is_file($file)) {
                unlink($file);
            }
            throw $e;
        }
    }

    /** @return array{url: string, token: string, created_at: string} */
    private function prepareWebProbe(): array
    {
        $appUrl = rtrim((string) (Env::read($this->root.'/backend/.env')['APP_URL'] ?? ''), '/');
        if ($appUrl === '' || ! filter_var($appUrl, FILTER_VALIDATE_URL)) {
            throw new RuntimeException('APP_URL must be an absolute URL for web readiness verification.');
        }
        $probe = ['url' => $appUrl.'/updater/probe', 'token' => bin2hex(random_bytes(32)), 'created_at' => gmdate('c')];
        $this->state->write('probe.json', $probe);

        return $probe;
    }

    /**
     * @param  array<string, mixed>  $version
     * @param  array{url: string, token: string, created_at: string}  $probe
     */
    private function verifyWeb(array $version, array $probe, bool $maintenanceBypass = false): void
    {
        if ($probe['url'] === '' || preg_match('/^[a-f0-9]{64}$/', $probe['token']) !== 1) {
            throw new RuntimeException('Web readiness probe metadata is invalid.');
        }
        $url = $probe['url'].'?token='.rawurlencode($probe['token']);
        $response = $maintenanceBypass
            ? Http::jsonWithMaintenanceBypass(substr($probe['url'], 0, -strlen('/updater/probe')).'/'.$probe['token'], $url, 65536)
            : Http::json($url, 65536);
        if (($response['token'] ?? null) !== $probe['token'] || ($response['version'] ?? null) !== ($version['version'] ?? null) || ($response['built_at'] ?? null) !== ($version['built_at'] ?? null)) {
            throw new RuntimeException('Web PHP did not serve the updated build. The application was returned to maintenance mode; reload PHP workers, then verify and recover manually if needed.');
        }
        if (($response['activity_protocol'] ?? null) !== 1) {
            throw new RuntimeException('Web PHP does not support safe update draining. Deploy the activity-gate release manually before using this updater.');
        }
    }

    private function clearBootstrapCaches(): void
    {
        $cache = $this->root.'/backend/bootstrap/cache';
        foreach (['config.php', 'events.php', 'packages.php', 'routes-v7.php', 'services.php'] as $file) {
            $path = $cache.'/'.$file;
            if (is_file($path) && ! unlink($path)) {
                throw new RuntimeException('Unable to clear bootstrap cache '.$file);
            }
        }
    }

    private function commandAvailable(string $command): bool
    {
        $process = proc_open([$command, '--version'], [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
        if (! is_resource($process)) {
            return false;
        }
        fclose($pipes[0]);
        stream_get_contents($pipes[1]);
        stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);

        return proc_close($process) === 0;
    }

    /** @param list<string> $arguments */
    private function artisan(array $arguments): void
    {
        $command = array_merge([PHP_BINARY, $this->root.'/backend/artisan'], $arguments);
        $process = proc_open($command, [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes, $this->root.'/backend');
        if (! is_resource($process)) {
            throw new RuntimeException('Unable to start artisan.');
        }
        fclose($pipes[0]);
        $output = stream_get_contents($pipes[1]).stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        if (proc_close($process) !== 0) {
            throw new RuntimeException('Artisan '.implode(' ', $arguments).' failed: '.trim($output));
        }
    }

    private static function write(string $message): void
    {
        fwrite(STDOUT, $message."\n");
    }

    private static function help(): void
    {
        self::write("Usage: php update.php [TAG|--cron|--status|--recover|--check-backup]\n\nTAG installs that release; no tag installs the latest stable release. --cron processes .filebeam/pending.json and records a heartbeat. --check-backup verifies database backup prerequisites without dumping or migrating. --recover restores pre-migration code from a journal backup; it refuses after migrations begin.");
    }
}

final class State
{
    private string $dir;

    public function __construct(string $root)
    {
        $this->dir = $root.'/.filebeam';
        if (! is_dir($this->dir) && ! mkdir($this->dir, 0700, true) && ! is_dir($this->dir)) {
            throw new RuntimeException('Cannot create updater state directory.');
        }
        if (! chmod($this->dir, 0700)) {
            throw new RuntimeException('Cannot secure updater state directory.');
        }
    }

    public function path(string $name): string
    {
        if (! Package::safePath($name)) {
            throw new RuntimeException('Invalid updater state path.');
        }
        $path = $this->dir.'/'.$name;
        $parent = dirname($path);
        if (! is_dir($parent) && ! mkdir($parent, 0700, true) && ! is_dir($parent)) {
            throw new RuntimeException('Cannot create updater state directory.');
        }

        return $path;
    }

    /** @return array<string, mixed>|null */
    public function read(string $name): ?array
    {
        $file = $this->path($name);
        if (! is_file($file)) {
            return null;
        }
        $contents = file_get_contents($file);
        if ($contents === false || strlen($contents) > 1048576) {
            throw new RuntimeException('Cannot read updater state file '.$name);
        }
        $value = json_decode($contents, true, 64, JSON_THROW_ON_ERROR);

        return is_array($value) ? $value : throw new RuntimeException('Invalid state file '.$name);
    }

    /** @param array<string, mixed> $value */
    public function write(string $name, array $value): void
    {
        $file = $this->path($name);
        $temporary = $file.'.'.bin2hex(random_bytes(8)).'.tmp';
        $contents = json_encode($value, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        if (file_put_contents($temporary, $contents, LOCK_EX) === false || ! chmod($temporary, 0600) || ! rename($temporary, $file)) {
            @unlink($temporary);
            throw new RuntimeException('Cannot write updater state file '.$name);
        }
    }

    public function remove(string $name): void
    {
        $file = $this->path($name);
        if (is_file($file) && ! unlink($file)) {
            throw new RuntimeException('Cannot remove updater state file '.$name);
        }
    }

    /** @return resource */
    public function lock()
    {
        $lock = fopen($this->path('updater.lock'), 'c+') ?: throw new RuntimeException('Cannot open updater lock.');
        if (! flock($lock, LOCK_EX | LOCK_NB)) {
            fclose($lock);
            throw new RuntimeException('Another updater is running.');
        }

        return $lock;
    }
}

final class Recovery
{
    public function __construct(private string $root, private State $state) {}

    public function run(): void
    {
        $journal = $this->state->read('journal.json') ?? throw new RuntimeException('No interrupted update journal exists.');
        if (($journal['schema'] ?? null) !== 1) {
            throw new RuntimeException('Interrupted update journal has an unsupported schema.');
        }
        $phase = $journal['phase'] ?? null;
        if (! in_array($phase, ['prepared', 'maintenance', 'draining'], true)) {
            if ($phase === 'migrating') {
                throw new RuntimeException('Recovery refuses code rollback after migrations began. Restore the database backup at '.($journal['database_backup'] ?? 'the recorded backup').' and deploy manually.');
            }
            throw new RuntimeException('Interrupted update journal has an unsafe recovery phase.');
        }
        $backup = $journal['backup'] ?? null;
        $oldFiles = $journal['old_files'] ?? null;
        $newFiles = $journal['new_files'] ?? null;
        if (! is_string($backup) || ! str_starts_with($backup, $this->state->path('backups').'/') || ! is_dir($backup) || ! is_array($oldFiles) || ! is_array($newFiles)) {
            throw new RuntimeException('Interrupted update journal has no valid code backup.');
        }
        foreach (array_merge($oldFiles, $newFiles) as $relative) {
            if (! is_string($relative) || ! Package::safePath($relative)) {
                throw new RuntimeException('Interrupted update journal has an unsafe file path.');
            }
        }
        foreach ($newFiles as $relative) {
            if (! in_array($relative, $oldFiles, true)) {
                $target = $this->root.'/'.$relative;
                if ((is_file($target) || is_link($target)) && ! unlink($target)) {
                    throw new RuntimeException('Unable to remove newly installed '.$relative);
                }
            }
        }
        $iterator = new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator($backup, \FilesystemIterator::SKIP_DOTS));
        foreach ($iterator as $file) {
            if (! $file->isFile()) {
                continue;
            }
            $relative = substr($file->getPathname(), strlen($backup) + 1);
            if (! Package::safePath($relative)) {
                throw new RuntimeException('Code backup has an unsafe file path.');
            }
            $target = $this->root.'/'.$relative;
            if (! is_dir(dirname($target)) && ! mkdir(dirname($target), 0755, true) && ! is_dir(dirname($target))) {
                throw new RuntimeException('Unable to create restore directory.');
            }
            if (! copy($file->getPathname(), $target)) {
                throw new RuntimeException('Unable to restore '.$relative);
            }
        }
        $this->artisanUp();
        $this->state->remove('probe.json');
        $this->state->remove('journal.json');
    }

    private function artisanUp(): void
    {
        $process = proc_open([PHP_BINARY, $this->root.'/backend/artisan', 'up'], [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes, $this->root.'/backend');
        if (! is_resource($process)) {
            throw new RuntimeException('Unable to start artisan recovery.');
        } fclose($pipes[0]);
        $output = stream_get_contents($pipes[1]).stream_get_contents($pipes[2]);
        fclose($pipes[1]);
        fclose($pipes[2]);
        if (proc_close($process) !== 0) {
            throw new RuntimeException('Unable to leave maintenance mode: '.trim($output));
        }
    }
}

final class Package
{
    public static function extract(string $archive, string $destination, int $limit): void
    {
        $zip = new ZipArchive;
        if ($zip->open($archive) !== true) {
            throw new RuntimeException('Cannot open release archive.');
        } $total = 0;
        for ($i = 0; $i < $zip->numFiles; $i++) {
            $stat = $zip->statIndex($i);
            $name = rtrim((string) ($stat['name'] ?? ''), '/');
            if (($name !== 'filebeam' && ! str_starts_with($name, 'filebeam/')) || ! self::safePath($name) || (($stat['external_attributes'] ?? 0) >> 16 & 0170000) === 0120000) {
                $zip->close();
                throw new RuntimeException('Archive contains an unsafe path, symlink, or invalid root.');
            } $total += $stat['size'] ?? 0;
            if ($total > $limit) {
                $zip->close();
                throw new RuntimeException('Archive exceeds unpacked size limit.');
            }
        } if (! $zip->extractTo($destination)) {
            $zip->close();
            throw new RuntimeException('Cannot extract release archive.');
        } $zip->close();
    }

    /** @return array{schema: 1, files: array<string, string>} */
    public static function manifest(string $root): array
    {
        $file = $root.'/package-files.json';
        if (! is_file($file)) {
            throw new RuntimeException('Package manifest is missing: package-files.json.');
        }
        $contents = file_get_contents($file);
        if ($contents === false) {
            throw new RuntimeException('Package manifest cannot be read: package-files.json.');
        }
        try {
            $manifest = json_decode($contents, false, 512, JSON_THROW_ON_ERROR);
        } catch (\JsonException $exception) {
            throw new RuntimeException('Package manifest JSON is invalid: '.$exception->getMessage().'.');
        }
        if (! $manifest instanceof \stdClass) {
            throw new RuntimeException('Package manifest root must be an object.');
        }
        if (($manifest->schema ?? null) !== 1) {
            throw new RuntimeException('Package manifest schema must be 1.');
        }
        if (! isset($manifest->files) || ! $manifest->files instanceof \stdClass) {
            throw new RuntimeException('Package manifest files must be an object.');
        }
        $files = get_object_vars($manifest->files);
        foreach ($files as $path => $hash) {
            if (! self::packagePath($path)) {
                throw new RuntimeException('Package manifest entry has an unsafe or unapproved path: '.self::displayPath($path).'.');
            }
            if (! is_string($hash) || ! preg_match('/^[a-f0-9]{64}$/', $hash)) {
                throw new RuntimeException('Package manifest hash is invalid for '.self::displayPath($path).'.');
            }
        }

        return ['schema' => 1, 'files' => $files];
    }

    /** @param array{schema: 1, files: array<string, string>} $manifest */
    public static function verify(string $root, array $manifest): void
    {
        foreach ($manifest['files'] as $path => $hash) {
            if (! self::packagePath($path)) {
                throw new RuntimeException('Package manifest entry has an unsafe or unapproved path: '.self::displayPath($path).'.');
            }
            if (! preg_match('/^[a-f0-9]{64}$/', $hash)) {
                throw new RuntimeException('Package manifest hash is invalid for '.self::displayPath($path).'.');
            }
            if (! is_file($root.'/'.$path)) {
                throw new RuntimeException('Package manifest file is missing: '.self::displayPath($path).'.');
            }
            if (hash_file('sha256', $root.'/'.$path) !== $hash) {
                throw new RuntimeException('Package manifest hash mismatch for '.self::displayPath($path).'.');
            }
        }
        foreach (['update.php', 'updater/Updater.php', 'updater/ActivityLock.php', 'updater/PostgresBackup.php'] as $required) {
            if (! isset($manifest['files'][$required])) {
                throw new RuntimeException('Package is missing required updater file: '.$required.'.');
            }
        }
    }

    public static function protected(string $path): bool
    {
        return $path === 'backend/.env' || str_starts_with($path, 'backend/storage/') || preg_match('#^backend/database/.*\.sqlite$#', $path) === 1;
    }

    public static function safePath(string $path): bool
    {
        $parts = explode('/', $path);

        return $path !== '' && ! str_starts_with($path, '/') && ! str_contains($path, "\0") && ! str_contains($path, '\\') && ! in_array('', $parts, true) && ! in_array('.', $parts, true) && ! in_array('..', $parts, true) && ! str_starts_with($path, '.filebeam/');
    }

    private static function packagePath(string $path): bool
    {
        return self::safePath($path)
            && ($path === 'LICENSE' || $path === 'README.md' || $path === 'SECURITY.md' || $path === 'docs/deployment.md' || $path === 'docs/social-previews.md' || str_starts_with($path, 'backend/') || str_starts_with($path, 'updater/') || $path === 'update.php');
    }

    private static function displayPath(string $path): string
    {
        return json_encode($path, JSON_UNESCAPED_SLASHES | JSON_INVALID_UTF8_SUBSTITUTE) ?: '"<invalid>"';
    }
}

final class Catalog
{
    /** @return array{schema: int, generation: int, published_at?: string, expires_at: string, releases: array<int, mixed>} */
    public static function verify(string $raw, string $key, ?int $now = null): array
    {
        if ($key === '' || strlen($raw) > 1048576) {
            throw new RuntimeException('Release catalog is too large.');
        }
        $envelope = json_decode($raw, true, 16, JSON_THROW_ON_ERROR);
        if (! is_array($envelope)) {
            throw new RuntimeException('Release catalog envelope is invalid.');
        }
        $payload = base64_decode((string) ($envelope['signed'] ?? ''), true);
        $signature = base64_decode((string) ($envelope['signature'] ?? ''), true);
        if ($payload === false || $payload === '' || $signature === false || $signature === '' || strlen($signature) !== SODIUM_CRYPTO_SIGN_BYTES || ! sodium_crypto_sign_verify_detached($signature, $payload, $key)) {
            throw new RuntimeException('Release catalog signature verification failed.');
        }
        $catalog = json_decode($payload, true, 64, JSON_THROW_ON_ERROR);
        if (! is_array($catalog) || ($catalog['schema'] ?? null) !== 1 || ! is_int($catalog['generation'] ?? null) || $catalog['generation'] < 0 || ! is_array($catalog['releases'] ?? null) || ! is_string($catalog['expires_at'] ?? null)) {
            throw new RuntimeException('Release catalog has an unsupported schema.');
        }
        $expiresAt = strtotime($catalog['expires_at']);
        if ($expiresAt === false || $expiresAt <= ($now ?? time())) {
            throw new RuntimeException('Release catalog is expired.');
        }

        $normalized = [
            'schema' => 1,
            'generation' => $catalog['generation'],
            'expires_at' => $catalog['expires_at'],
            'releases' => $catalog['releases'],
        ];
        if (is_string($catalog['published_at'] ?? null)) {
            $normalized['published_at'] = $catalog['published_at'];
        }

        return $normalized;
    }
}

final class Http
{
    public static function get(string $url, int $max): string
    {
        $buffer = '';
        self::request($url, function ($chunk) use (&$buffer, $max) {
            $buffer .= $chunk;

            return strlen($buffer) <= $max ? strlen($chunk) : 0;
        });

        return $buffer;
    }

    public static function download(string $url, string $file, int $expected): void
    {
        $out = fopen($file, 'wb') ?: throw new RuntimeException('Cannot create download file.');
        $written = 0;
        try {
            self::request($url, function ($chunk) use ($out, &$written, $expected) {
                $written += strlen($chunk);

                return $written <= $expected ? fwrite($out, $chunk) : 0;
            });
        } finally {
            fclose($out);
        } if ($written !== $expected) {
            throw new RuntimeException('Downloaded archive has an unexpected size.');
        }
    }

    /** @return array<string, mixed> */
    public static function json(string $url, int $max, bool $followRedirects = false): array
    {
        $raw = '';
        self::request($url, function ($chunk) use (&$raw, $max) {
            $raw .= $chunk;

            return strlen($raw) <= $max ? strlen($chunk) : 0;
        }, false, $followRedirects);
        $decoded = json_decode($raw, true, 16, JSON_THROW_ON_ERROR);

        return is_array($decoded) ? $decoded : throw new RuntimeException('Web readiness probe returned invalid JSON.');
    }

    /** @return array<string, mixed> */
    public static function jsonWithMaintenanceBypass(string $secretUrl, string $probeUrl, int $max): array
    {
        foreach ([$secretUrl, $probeUrl] as $url) {
            $parts = parse_url($url);
            if (! is_array($parts) || ! in_array($parts['scheme'] ?? '', ['http', 'https'], true) || ! isset($parts['host'])) {
                throw new RuntimeException('Refusing invalid web readiness URL.');
            }
        }
        if ($secretUrl === '' || $probeUrl === '') {
            throw new RuntimeException('Refusing invalid web readiness URL.');
        }
        $curl = curl_init($secretUrl);
        curl_setopt_array($curl, [CURLOPT_RETURNTRANSFER => true, CURLOPT_FOLLOWLOCATION => false, CURLOPT_PROTOCOLS => CURLPROTO_HTTP | CURLPROTO_HTTPS, CURLOPT_TIMEOUT => 120, CURLOPT_CONNECTTIMEOUT => 15, CURLOPT_USERAGENT => 'Filebeam-Updater/1', CURLOPT_COOKIEFILE => '']);
        $first = curl_exec($curl);
        $firstCode = curl_getinfo($curl, CURLINFO_RESPONSE_CODE);
        if ($first === false || $firstCode < 300 || $firstCode >= 400) {
            $error = curl_error($curl);
            curl_close($curl);
            throw new RuntimeException('Maintenance readiness bypass failed'.($error ? ': '.$error : ' (HTTP '.$firstCode.')'));
        }
        curl_setopt_array($curl, [CURLOPT_URL => $probeUrl, CURLOPT_RETURNTRANSFER => true]);
        $raw = curl_exec($curl);
        $code = curl_getinfo($curl, CURLINFO_RESPONSE_CODE);
        $error = curl_error($curl);
        curl_close($curl);
        if (! is_string($raw) || strlen($raw) > $max || $code !== 200) {
            throw new RuntimeException('Web readiness probe failed'.($error ? ': '.$error : ' (HTTP '.$code.')'));
        }
        $decoded = json_decode($raw, true, 16, JSON_THROW_ON_ERROR);

        return is_array($decoded) ? $decoded : throw new RuntimeException('Web readiness probe returned invalid JSON.');
    }

    private static function request(string $url, callable $sink, bool $releaseHost = true, bool $followRedirects = false): void
    {
        $parts = parse_url($url);
        if (! is_array($parts) || ! isset($parts['scheme'], $parts['host']) || ! in_array($parts['scheme'], ['http', 'https'], true) || ($releaseHost && ($parts['scheme'] !== 'https' || $parts['host'] !== 'releases.filebeam.io'))) {
            throw new RuntimeException('Refusing invalid update URL.');
        } $curl = curl_init($url);
        curl_setopt_array($curl, [CURLOPT_RETURNTRANSFER => false, CURLOPT_FOLLOWLOCATION => $followRedirects, CURLOPT_PROTOCOLS => CURLPROTO_HTTP | CURLPROTO_HTTPS, CURLOPT_TIMEOUT => 120, CURLOPT_CONNECTTIMEOUT => 15, CURLOPT_USERAGENT => 'Filebeam-Updater/1', CURLOPT_WRITEFUNCTION => fn ($c, $data) => $sink($data)]);
        if ($followRedirects) {
            curl_setopt($curl, CURLOPT_COOKIEFILE, '');
        } $ok = curl_exec($curl);
        $code = curl_getinfo($curl, CURLINFO_RESPONSE_CODE);
        $error = curl_error($curl);
        curl_close($curl);
        if ($ok !== true || $code !== 200) {
            throw new RuntimeException('Release server request failed'.($error ? ': '.$error : ' (HTTP '.$code.')'));
        }
    }
}

final class Env
{
    /** @var list<string> */
    private const PROCESS_OVERRIDES = [
        'APP_ENV', 'APP_KEY', 'APP_URL', 'APP_CONFIG_CACHE',
        'DB_CONNECTION', 'DB_URL', 'DB_DATABASE', 'DB_HOST', 'DB_PORT', 'DB_USERNAME', 'DB_PASSWORD', 'DB_SOCKET',
        'DB_SSLMODE', 'DB_SSLROOTCERT', 'DB_SSLCERT', 'DB_SSLKEY', 'DB_APPLICATION_NAME', 'MYSQL_ATTR_SSL_CA',
    ];

    /** @var list<string> */
    private const POSTGRES_PROCESS_OVERRIDES = [
        'PGHOSTADDR', 'PGSERVICE', 'PGSERVICEFILE', 'PGOPTIONS', 'PGTARGETSESSIONATTRS', 'PGLOADBALANCEHOSTS',
    ];

    /** @return array<string, string> */
    public static function read(string $file): array
    {
        if (! is_file($file)) {
            throw new RuntimeException('Missing backend/.env required for database backup.');
        }
        if (! class_exists(Dotenv::class)) {
            $autoload = dirname($file).'/vendor/autoload.php';
            if (! is_file($autoload)) {
                throw new RuntimeException('The bundled dotenv parser is required for database backup.');
            }
            require_once $autoload;
        }
        if (! class_exists(Dotenv::class)) {
            throw new RuntimeException('The bundled dotenv parser is required for database backup.');
        }

        try {
            $loaded = Dotenv::createArrayBacked(dirname($file), basename($file))->load();
        } catch (\Throwable) {
            throw new RuntimeException('Unable to parse backend/.env required for database backup.');
        }
        $values = [];
        foreach ($loaded as $key => $value) {
            if (is_string($value)) {
                $values[$key] = $value;
            }
        }
        foreach (self::PROCESS_OVERRIDES as $key) {
            $value = getenv($key);
            if (is_string($value)) {
                $values[$key] = $value;
            }
        }

        return $values;
    }

    /**
     * @return array{driver: string, host: string, port: string, database: string, username: string, password: string, unix_socket: ?string, socket: ?string, sslmode: ?string, sslrootcert: ?string, sslcert: ?string, sslkey: ?string, application_name: ?string}
     */
    public static function database(string $file): array
    {
        $env = self::read($file);
        $driver = self::string($env['DB_CONNECTION'] ?? 'sqlite', 'database driver');
        if (in_array($driver, ['pgsql', 'postgres', 'postgresql'], true)) {
            self::rejectPostgresProcessOverrides();
        }
        $url = self::optional($env['DB_URL'] ?? null);
        $socket = self::optional($env['DB_SOCKET'] ?? null);
        $configuration = match ($driver) {
            'sqlite' => [
                'driver' => 'sqlite',
                'database' => $env['DB_DATABASE'] ?? dirname($file).'/database/database.sqlite',
                'url' => $url,
            ],
            'mysql', 'mariadb' => [
                'driver' => $driver,
                'host' => $env['DB_HOST'] ?? '127.0.0.1',
                'port' => $env['DB_PORT'] ?? '3306',
                'database' => $env['DB_DATABASE'] ?? 'laravel',
                'username' => $env['DB_USERNAME'] ?? 'root',
                'password' => $env['DB_PASSWORD'] ?? '',
                'unix_socket' => $env['DB_SOCKET'] ?? '',
                'url' => $url,
            ],
            'pgsql', 'postgres', 'postgresql' => [
                'driver' => $driver,
                'host' => $socket ?? ($env['DB_HOST'] ?? '127.0.0.1'),
                'port' => $env['DB_PORT'] ?? '5432',
                'database' => $env['DB_DATABASE'] ?? 'laravel',
                'username' => $env['DB_USERNAME'] ?? 'root',
                'password' => $env['DB_PASSWORD'] ?? '',
                'sslmode' => $env['DB_SSLMODE'] ?? 'prefer',
                'sslrootcert' => $env['DB_SSLROOTCERT'] ?? null,
                'sslcert' => $env['DB_SSLCERT'] ?? null,
                'sslkey' => $env['DB_SSLKEY'] ?? null,
                'application_name' => $env['DB_APPLICATION_NAME'] ?? null,
                'url' => $url,
            ],
            default => throw new RuntimeException('Unsupported database driver.'),
        };

        $database = self::normalize(self::parseUrl($configuration));

        self::assertCachedDatabase($file, $database);

        return $database;
    }

    /** @param array<string, mixed> $configuration
     * @return array<array-key, mixed>
     */
    private static function parseUrl(array $configuration): array
    {
        $url = $configuration['url'] ?? null;
        unset($configuration['url']);
        if (! $url) {
            return $configuration;
        }
        if (! is_string($url)) {
            throw new RuntimeException('Database configuration URL is invalid.');
        }

        // Match Laravel's SQLite workaround so absolute paths retain their leading slash.
        $url = preg_replace('#^(sqlite3?):///#', '$1://null/', $url);
        try {
            $raw = parse_url($url);
        } catch (\Throwable) {
            throw new RuntimeException('Database configuration URL is invalid.');
        }
        if (! is_array($raw)) {
            throw new RuntimeException('Database configuration URL is invalid.');
        }
        $coerce = static function (mixed $value): mixed {
            if (! is_string($value)) {
                return $value;
            }
            $decoded = json_decode($value, true);

            return json_last_error() === JSON_ERROR_NONE ? $decoded : $value;
        };
        $components = array_map(static fn (mixed $value): mixed => is_string($value) ? rawurldecode($value) : $value, $raw);
        $components = array_map($coerce, $components);
        $query = [];
        if (isset($raw['query'])) {
            parse_str($raw['query'], $query);
            $query = array_map($coerce, $query);
        }
        $driver = $components['scheme'] ?? null;
        $database = $components['path'] ?? null;
        $parsed = array_merge($configuration, array_filter([
            'driver' => is_string($driver) ? match ($driver) {
                'mysql2' => 'mysql', 'postgres' => 'pgsql', 'postgresql' => 'pgsql', 'sqlite3' => 'sqlite', default => $driver,
            } : $driver,
            'database' => is_string($database) && $database !== '/' ? substr($database, 1) : null,
            'host' => $components['host'] ?? null,
            'port' => $components['port'] ?? null,
            'username' => $components['user'] ?? null,
            'password' => $components['pass'] ?? null,
        ], static fn (mixed $value): bool => $value !== null), $query);
        foreach ($parsed as $value) {
            if (is_array($value) || is_object($value) || is_resource($value)) {
                throw new RuntimeException('Database configuration URL is invalid.');
            }
        }

        return $parsed;
    }

    private static function rejectPostgresProcessOverrides(): void
    {
        foreach (self::POSTGRES_PROCESS_OVERRIDES as $key) {
            $value = getenv($key);
            if (is_string($value) && $value !== '') {
                throw new RuntimeException('Configure PostgreSQL connections with DB_* variables only.');
            }
        }
    }

    /**
     * @param  array<string, mixed>  $configuration
     * @return array{driver: string, host: string, port: string, database: string, username: string, password: string, unix_socket: ?string, socket: ?string, sslmode: ?string, sslrootcert: ?string, sslcert: ?string, sslkey: ?string, application_name: ?string}
     */
    private static function normalize(array $configuration): array
    {
        $driver = self::string($configuration['driver'] ?? null, 'database driver');
        $driver = match ($driver) {
            'postgres', 'postgresql' => 'pgsql',
            'mysql2' => 'mysql',
            'sqlite3' => 'sqlite',
            default => $driver,
        };
        if (! in_array($driver, ['sqlite', 'mysql', 'mariadb', 'pgsql'], true)) {
            throw new RuntimeException('Unsupported database driver.');
        }

        $database = self::string($configuration['database'] ?? null, 'database name');
        if ($driver === 'sqlite') {
            return [
                'driver' => 'sqlite', 'host' => '', 'port' => '', 'database' => $database, 'username' => '', 'password' => '',
                'unix_socket' => null, 'socket' => null, 'sslmode' => null, 'sslrootcert' => null, 'sslcert' => null, 'sslkey' => null, 'application_name' => null,
            ];
        }

        $port = self::port($configuration['port'] ?? ($driver === 'pgsql' ? '5432' : '3306'));
        if (preg_match('/^[1-9][0-9]{0,4}$/', $port) !== 1 || (int) $port > 65535) {
            throw new RuntimeException('Database port is invalid.');
        }
        $socket = self::optional($configuration['unix_socket'] ?? $configuration['socket'] ?? null);
        if ($driver === 'pgsql') {
            // Laravel and libpq use a directory-valued host for PostgreSQL sockets.
            $socket = null;
        }

        return [
            'driver' => $driver,
            'host' => self::string($configuration['host'] ?? null, 'database host'),
            'port' => $port,
            'database' => $database,
            'username' => self::string($configuration['username'] ?? null, 'database username'),
            'password' => self::password($configuration['password'] ?? ''),
            'unix_socket' => $socket,
            'socket' => $socket,
            'sslmode' => self::optional($configuration['sslmode'] ?? null),
            'sslrootcert' => self::optional($configuration['sslrootcert'] ?? null),
            'sslcert' => self::optional($configuration['sslcert'] ?? null),
            'sslkey' => self::optional($configuration['sslkey'] ?? null),
            'application_name' => self::optional($configuration['application_name'] ?? null),
        ];
    }

    /** @param array{driver: string, host: string, port: string, database: string, username: string, password: string, unix_socket: ?string, socket: ?string, sslmode: ?string, sslrootcert: ?string, sslcert: ?string, sslkey: ?string, application_name: ?string} $database */
    private static function assertCachedDatabase(string $file, array $database): void
    {
        $configured = getenv('APP_CONFIG_CACHE');
        $cache = is_string($configured) && $configured !== ''
            ? (str_starts_with($configured, '/') ? $configured : dirname($file).'/'.$configured)
            : dirname($file).'/bootstrap/cache/config.php';
        if (! is_file($cache)) {
            return;
        }
        try {
            $config = require $cache;
            $default = $config['database']['default'] ?? null;
            $connection = is_string($default) ? ($config['database']['connections'][$default] ?? null) : null;
            if (! is_array($connection)) {
                throw new \UnexpectedValueException;
            }
            if (array_key_exists('read', $connection) || array_key_exists('write', $connection)) {
                throw new \UnexpectedValueException;
            }
            // PDO options are not endpoint identity and are normally an array in Laravel's cache.
            unset($connection['options']);
            $cached = self::normalize(self::parseUrl($connection));
            foreach (['driver', 'host', 'port', 'database', 'username', 'password', 'unix_socket', 'sslmode', 'sslrootcert', 'sslcert', 'sslkey'] as $key) {
                if ($database[$key] !== $cached[$key]) {
                    throw new \UnexpectedValueException;
                }
            }
        } catch (\Throwable) {
            throw new RuntimeException('Database configuration changed; run php artisan config:cache before updating.');
        }
    }

    private static function string(mixed $value, string $name): string
    {
        if (! is_string($value) || $value === '' || self::reserved($value)) {
            throw new RuntimeException('Invalid '.$name.'.');
        }

        return $value;
    }

    private static function password(mixed $value): string
    {
        if ($value === null) {
            return '';
        }
        if (! is_string($value)) {
            throw new RuntimeException('Invalid database password.');
        }

        return self::reserved($value) ? '' : $value;
    }

    private static function port(mixed $value): string
    {
        if (is_int($value)) {
            return (string) $value;
        }

        return self::string($value, 'database port');
    }

    private static function optional(mixed $value): ?string
    {
        if ($value === null || $value === '' || (is_string($value) && self::reserved($value))) {
            return null;
        }
        if (! is_string($value)) {
            throw new RuntimeException('Invalid database configuration.');
        }

        return $value;
    }

    private static function reserved(string $value): bool
    {
        return in_array(strtolower($value), ['null', '(null)', 'empty', '(empty)'], true);
    }
}

final class Semver
{
    public static function parseTag(string $value, bool $tag = false): ?string
    {
        $value = str_starts_with($value, 'v') ? substr($value, 1) : $value;
        $identifier = '(?:0|[1-9][0-9]*|(?=[0-9A-Za-z-]*[A-Za-z-])[0-9A-Za-z-]+)';
        $pattern = '/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-('.$identifier.'(?:\.'.$identifier.')*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/D';
        if (! preg_match($pattern, $value, $matches)) {
            return null;
        }
        $preRelease = $matches[4] ?? '';

        return $matches[1].'.'.$matches[2].'.'.$matches[3].($preRelease === '' ? '' : '-'.$preRelease);
    }

    public static function compare(string $a, string $b): int
    {
        $a = self::parseTag($a, true) ?? throw new RuntimeException('Invalid semantic version.');
        $b = self::parseTag($b, true) ?? throw new RuntimeException('Invalid semantic version.');
        [$aNumeric, $aPreRelease] = array_pad(explode('-', $a, 2), 2, null);
        [$bNumeric, $bPreRelease] = array_pad(explode('-', $b, 2), 2, null);
        foreach (array_map(null, explode('.', $aNumeric), explode('.', $bNumeric)) as [$left, $right]) {
            if ((int) $left !== (int) $right) {
                return (int) $left <=> (int) $right;
            }
        }
        if ($aPreRelease === null || $bPreRelease === null) {
            return $aPreRelease === $bPreRelease ? 0 : ($aPreRelease === null ? 1 : -1);
        }
        $leftIdentifiers = explode('.', $aPreRelease);
        $rightIdentifiers = explode('.', $bPreRelease);
        foreach (array_map(null, $leftIdentifiers, $rightIdentifiers) as [$left, $right]) {
            if ($left === null || $right === null) {
                return $left === $right ? 0 : ($left === null ? -1 : 1);
            }
            if (ctype_digit($left) && ctype_digit($right)) {
                if ((int) $left !== (int) $right) {
                    return (int) $left <=> (int) $right;
                }

                continue;
            }
            if (ctype_digit($left) !== ctype_digit($right)) {
                return ctype_digit($left) ? -1 : 1;
            }
            if ($left !== $right) {
                return $left <=> $right;
            }
        }

        return 0;
    }
}
