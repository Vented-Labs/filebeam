<?php

declare(strict_types=1);

use Filebeam\Updater\PostgresBackup;
use Illuminate\Support\Facades\File;

beforeEach(function (): void {
    require_once base_path('../updater/PostgresBackup.php');
    $this->backupRoot = storage_path('framework/testing/postgres-backup-'.bin2hex(random_bytes(4)));
    File::ensureDirectoryExists($this->backupRoot, 0700);
    $this->log = $this->backupRoot.'/commands.jsonl';
    $this->dump = $this->backupRoot.'/pg_dump';
    $this->restore = $this->backupRoot.'/pg_restore';
    $this->environment = [];
    foreach (['POSTGRES_BACKUP_TEST_LOG', 'POSTGRES_BACKUP_TEST_TRUNCATE', 'POSTGRES_BACKUP_TEST_FAIL_RESTORE', 'POSTGRES_BACKUP_TEST_FAIL_DUMP', 'POSTGRES_BACKUP_TEST_SLEEP', 'POSTGRES_BACKUP_TEST_DIAGNOSTIC', 'PGPASSWORD', 'PGOPTIONS'] as $key) {
        $this->environment[$key] = getenv($key);
    }
    $script = <<<'PHP'
#!/usr/bin/env php
<?php
declare(strict_types=1);
$log = getenv('POSTGRES_BACKUP_TEST_LOG');
if (in_array('--version', $argv, true)) { echo "pg tool (PostgreSQL) 16.15\n"; exit(0); }
$passfile = getenv('PGPASSFILE');
$record = ['argv' => $argv, 'passfile' => $passfile, 'passfile_contents' => $passfile ? file_get_contents($passfile) : null, 'passfile_mode' => $passfile ? (fileperms($passfile) & 0777) : null, 'pgpassword' => getenv('PGPASSWORD'), 'pgoptions' => getenv('PGOPTIONS'), 'sslmode' => getenv('PGSSLMODE'), 'sslrootcert' => getenv('PGSSLROOTCERT')];
file_put_contents($log, json_encode($record)."\n", FILE_APPEND);
$operation = basename($argv[0]) === 'pg_dump' ? 'dump' : (in_array('--list', $argv, true) ? 'catalog' : 'data');
if (getenv('POSTGRES_BACKUP_TEST_DIAGNOSTIC') === $operation) {
    fwrite(STDOUT, "stdout must not appear in diagnostics\n");
    fwrite(STDERR, "permission denied: secret:with space / secret%3Awith%20space / secret%3Awith+space / secret\\:with space\n".str_repeat('x', 4080)."secret:with space".str_repeat('x', 70000));
    exit(7);
}
if (getenv('POSTGRES_BACKUP_TEST_FAIL_DUMP') && basename($argv[0]) === 'pg_dump') { exit(1); }
if (getenv('POSTGRES_BACKUP_TEST_SLEEP')) { sleep((int) getenv('POSTGRES_BACKUP_TEST_SLEEP')); }
if (basename($argv[0]) === 'pg_dump') { foreach ($argv as $argument) { if (str_starts_with($argument, '--file=')) { file_put_contents(substr($argument, 7), getenv('POSTGRES_BACKUP_TEST_TRUNCATE') ? 'bad' : 'PGDMPtest'); exit(0); } } }
if (in_array('--list', $argv, true)) { echo "1; 0 0 TABLE public example\n"; }
exit(getenv('POSTGRES_BACKUP_TEST_FAIL_RESTORE') ? 1 : 0);
PHP;
    foreach ([$this->dump, $this->restore] as $binary) {
        File::put($binary, $script);
        chmod($binary, 0700);
    }
    putenv('POSTGRES_BACKUP_TEST_LOG='.$this->log);
});

afterEach(function (): void {
    foreach ($this->environment as $key => $value) {
        putenv($value === false ? $key : $key.'='.$value);
    }
    File::deleteDirectory($this->backupRoot);
});

test('creates and validates a private custom archive without exposing credentials', function (): void {
    $backup = new PostgresBackup([
        'driver' => 'pgsql',
        'socket' => '/var/run/postgresql',
        'port' => '5432',
        'database' => 'filebeam',
        'username' => 'backup user',
        'password' => 'quotes \\ and spaces:$secret',
        'sslmode' => 'verify-full',
        'sslrootcert' => '/private/ca.pem',
    ], $this->backupRoot.'/private', $this->dump, $this->restore);
    putenv('PGOPTIONS=-c search_path=unsafe');

    $file = $backup->backup();
    $records = array_map(json_decode(...), file($this->log, FILE_IGNORE_NEW_LINES));

    expect($file)->toEndWith('.dump')
        ->and(file_get_contents($file, false, null, 0, 5))->toBe('PGDMP')
        ->and(fileperms($file) & 0777)->toBe(0600)
        ->and(fileperms(dirname($file)) & 0777)->toBe(0700)
        ->and($records)->toHaveCount(3)
        ->and($records[0]->argv)->toContain('--host=/var/run/postgresql', '--port=5432', '--username=backup user', '--dbname=filebeam', '--no-password')
        ->and($records[0]->passfile_mode)->toBe(0600)
        ->and($records[0]->passfile_contents)->toBe('/var/run/postgresql:5432:filebeam:backup user:quotes \\\\ and spaces\:$secret'."\n")
        ->and($records[0]->pgpassword)->toBeFalse()
        ->and($records[0]->pgoptions)->toBeFalse()
        ->and($records[0]->sslmode)->toBe('verify-full')
        ->and($records[0]->sslrootcert)->toBe('/private/ca.pem')
        ->and(glob(dirname($file).'/.pgpass-*'))->toBe([]);
});

test('removes partial archives and credentials when validation fails', function (): void {
    putenv('POSTGRES_BACKUP_TEST_TRUNCATE=1');
    $backup = new PostgresBackup(['database' => 'filebeam', 'username' => 'backup', 'password' => 'not-in-errors'], $this->backupRoot.'/private', $this->dump, $this->restore);

    expect(fn (): string => $backup->backup())->toThrow(RuntimeException::class, 'PostgreSQL backup validation failed.')
        ->and(glob($this->backupRoot.'/private/*'))->toBe([]);
});

test('does not retain credentials when a required client tool is missing or fails', function (): void {
    $settings = ['database' => 'filebeam', 'username' => 'backup', 'password' => 'not-in-errors'];
    $missing = new PostgresBackup($settings, $this->backupRoot.'/missing', $this->backupRoot.'/not-pg-dump', $this->restore);
    $failing = new PostgresBackup($settings, $this->backupRoot.'/failing', $this->dump, $this->restore);
    putenv('POSTGRES_BACKUP_TEST_FAIL_RESTORE=1');

    expect(fn (): string => $missing->backup())->toThrow(RuntimeException::class, 'Required PostgreSQL client tool is unavailable.')
        ->and(glob($this->backupRoot.'/missing/*'))->toBe([])
        ->and(fn (): string => $failing->backup())->toThrow(RuntimeException::class, 'PostgreSQL client tool failed.')
        ->and(glob($this->backupRoot.'/failing/*'))->toBe([]);
});

test('removes partial archives when pg_dump fails or times out', function (): void {
    $settings = ['database' => 'filebeam', 'username' => 'backup', 'password' => 'not-in-errors'];
    $failing = new PostgresBackup($settings, $this->backupRoot.'/failing-dump', $this->dump, $this->restore);
    putenv('POSTGRES_BACKUP_TEST_FAIL_DUMP=1');

    expect(fn (): string => $failing->backup())->toThrow(RuntimeException::class, 'PostgreSQL client tool failed.')
        ->and(glob($this->backupRoot.'/failing-dump/*'))->toBe([]);

    putenv('POSTGRES_BACKUP_TEST_FAIL_DUMP');
    putenv('POSTGRES_BACKUP_TEST_SLEEP=2');
    $timingOut = new PostgresBackup($settings, $this->backupRoot.'/timeout', $this->dump, $this->restore, 1);

    expect(fn (): string => $timingOut->backup())->toThrow(RuntimeException::class, 'PostgreSQL client tool timed out.')
        ->and(glob($this->backupRoot.'/timeout/*'))->toBe([]);
});

test('rejects unsafe credentials and connection-string database names', function (): void {
    expect(fn () => new PostgresBackup(['database' => 'postgresql://host/db', 'username' => 'backup'], $this->backupRoot, $this->dump, $this->restore))->toThrow(RuntimeException::class)
        ->and(fn () => new PostgresBackup(['database' => 'filebeam', 'username' => 'backup', 'password' => "line\nbreak"], $this->backupRoot, $this->dump, $this->restore))->toThrow(RuntimeException::class);
});

test('reports the failed backup operation and bounded redacted stderr', function (string $stage, string $operation): void {
    putenv('POSTGRES_BACKUP_TEST_DIAGNOSTIC='.$stage);
    $backup = new PostgresBackup(['database' => 'filebeam', 'username' => 'backup', 'password' => 'secret:with space'], $this->backupRoot.'/diagnostic', $this->dump, $this->restore);

    try {
        $backup->backup();
        $this->fail('Expected backup failure.');
    } catch (RuntimeException $exception) {
        expect($exception->getMessage())->toContain('Operation: '.$operation, 'exit code: 7', 'permission denied', '[redacted]', '[truncated]')
            ->not->toContain('secret', 'stdout must not appear');
        expect(strlen($exception->getMessage()))->toBeLessThan(4400);
    }
    expect(glob($this->backupRoot.'/diagnostic/*'))->toBe([])
        ->and(glob($this->backupRoot.'/diagnostic/.pgpass-*'))->toBe([]);
})->with([
    ['dump', 'pg_dump backup'],
    ['catalog', 'pg_restore archive catalog validation'],
    ['data', 'pg_restore archive data validation'],
]);
