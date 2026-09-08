<?php

declare(strict_types=1);

use Filebeam\Updater\Recovery;
use Filebeam\Updater\State;
use Illuminate\Support\Facades\File;
use Illuminate\Support\Facades\Process;

beforeEach(function (): void {
    $this->updateRoot = storage_path('framework/testing/updater-'.bin2hex(random_bytes(4)));
    File::ensureDirectoryExists($this->updateRoot.'/backend');
    File::put($this->updateRoot.'/backend/artisan', "#!/usr/bin/env php\n<?php exit(0);\n");
});

afterEach(function (): void {
    File::deleteDirectory($this->updateRoot);
});

test('recovery rolls back a pre-migration update while preserving its database backup', function (string $phase): void {
    require_once base_path('../updater/Updater.php');

    $state = new State($this->updateRoot);
    $backup = $state->path('backups/interrupted');
    $databaseBackup = $state->path('database-backups/pre-migration.dump');
    File::ensureDirectoryExists($backup.'/backend');
    File::put($backup.'/backend/old.php', 'old');
    File::put($databaseBackup, 'database backup');
    File::put($this->updateRoot.'/backend/old.php', 'new');
    File::put($this->updateRoot.'/backend/introduced.php', 'introduced');
    $state->write('journal.json', [
        'schema' => 1,
        'phase' => $phase,
        'backup' => $backup,
        'database_backup' => $databaseBackup,
        'old_files' => ['backend/old.php'],
        'new_files' => ['backend/old.php', 'backend/introduced.php'],
    ]);

    (new Recovery($this->updateRoot, $state))->run();

    expect(File::get($this->updateRoot.'/backend/old.php'))->toBe('old')
        ->and(File::exists($this->updateRoot.'/backend/introduced.php'))->toBeFalse()
        ->and(File::get($databaseBackup))->toBe('database backup')
        ->and(File::exists($state->path('journal.json')))->toBeFalse();
})->with(['maintenance', 'draining']);

test('standalone recovery starts when replacement stopped before new helper files arrived', function (): void {
    $state = new State($this->updateRoot);
    $backup = $state->path('backups/interrupted');
    File::ensureDirectoryExists($backup);
    File::copy(base_path('../updater/Updater.php'), $this->updateRoot.'/Updater.php');
    $state->write('journal.json', [
        'schema' => 1, 'phase' => 'draining', 'backup' => $backup,
        'old_files' => [], 'new_files' => [],
    ]);

    $result = Process::timeout(10)->run([
        PHP_BINARY, '-r',
        'require $argv[1]; exit(\Filebeam\Updater\Command::run(["update.php", "--recover"], $argv[2]));',
        $this->updateRoot.'/Updater.php', $this->updateRoot,
    ]);

    expect($result->successful())->toBeTrue();
    expect($state->read('journal.json'))->toBeNull();
});

test('recovery refuses a migration-phase journal without changing its code or backups', function (): void {
    require_once base_path('../updater/Updater.php');

    $state = new State($this->updateRoot);
    $backup = $state->path('backups/interrupted');
    $databaseBackup = $state->path('database-backups/migrated.dump');
    $probeToken = 'secret-probe-token';
    File::ensureDirectoryExists($backup.'/backend');
    File::put($backup.'/backend/old.php', 'old');
    File::put($databaseBackup, 'database backup');
    File::put($this->updateRoot.'/backend/old.php', 'new');
    File::put($this->updateRoot.'/backend/introduced.php', 'introduced');
    $journal = [
        'schema' => 1,
        'phase' => 'migrating',
        'backup' => $backup,
        'database_backup' => $databaseBackup,
        'old_files' => ['backend/old.php'],
        'new_files' => ['backend/old.php', 'backend/introduced.php'],
    ];
    $state->write('journal.json', $journal);
    $state->write('probe.json', ['token' => $probeToken]);

    try {
        (new Recovery($this->updateRoot, $state))->run();
        throw new RuntimeException('Recovery unexpectedly succeeded.');
    } catch (RuntimeException $exception) {
        expect($exception->getMessage())->toContain($databaseBackup)
            ->not->toContain($probeToken);
    }

    expect(File::get($this->updateRoot.'/backend/old.php'))->toBe('new')
        ->and(File::get($this->updateRoot.'/backend/introduced.php'))->toBe('introduced')
        ->and(File::get($backup.'/backend/old.php'))->toBe('old')
        ->and(File::get($databaseBackup))->toBe('database backup')
        ->and($state->read('journal.json'))->toBe($journal);
});
