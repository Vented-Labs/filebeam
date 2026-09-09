<?php

declare(strict_types=1);

use App\Services\ReleaseChecker;
use App\Support\Installation\InstallationState;
use Filebeam\Updater\ActivityLock;
use Illuminate\Foundation\Inspiring;
use Illuminate\Support\Facades\Artisan;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schedule;
use Symfony\Component\Process\PhpExecutableFinder;

Artisan::command('filebeam:inspire', function () {
    $this->comment(Inspiring::quote());
})->purpose('Display an inspiring quote');

Schedule::call(function (): void {
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    $handle = null;
    if (config('version.distribution') === 'package') {
        try {
            $handle = $lock->acquireShared();
        } catch (RuntimeException) {
            return;
        }
    }
    try {
        DB::table('scheduler_heartbeats')->upsert(
            [['name' => 'scheduler', 'last_run_at' => now('UTC')]],
            ['name'],
            ['last_run_at'],
        );
    } finally {
        if (is_resource($handle)) {
            $lock->release($handle);
        }
    }
})->name('filebeam:scheduler-heartbeat')->everyMinute();

Schedule::command('filebeam:prune-transfers')->everyFifteenMinutes()->withoutOverlapping();

Schedule::call(function (): void {
    $lock = new ActivityLock(storage_path('app/update-activity.lock'));
    $handle = null;
    if (config('version.distribution') === 'package') {
        try {
            $handle = $lock->acquireShared();
        } catch (RuntimeException) {
            return;
        }
    }
    try {
        DB::statement('PRAGMA optimize');
        DB::statement('VACUUM');
    } finally {
        if (is_resource($handle)) {
            $lock->release($handle);
        }
    }
})->name('filebeam:maintain-sqlite')
    ->daily()
    ->when(fn (): bool => DB::connection()->getDriverName() === 'sqlite')
    ->withoutOverlapping();

Schedule::command('queue:work', [
    'database',
    '--stop-when-empty',
    '--max-time' => 50,
    '--max-jobs' => 25,
    '--sleep' => 1,
    '--tries' => 5,
    '--backoff' => 60,
    '--timeout' => 60,
])->name('filebeam:process-background-work')
    ->everyMinute()
    ->when(fn (): bool => (bool) config('queue.cron_enabled')
        && config('queue.default') === 'database'
        && ! app(InstallationState::class)->requiresSetup())
    ->withoutOverlapping(10);

Schedule::call(fn (): array => app(ReleaseChecker::class)->checkAndInstallAutomaticUpdate())
    ->name('filebeam:check-updates')
    ->daily()
    ->withoutOverlapping();

$php = (new PhpExecutableFinder)->find(false) ?: 'php';
Schedule::exec(escapeshellarg($php), [escapeshellarg(dirname(base_path()).'/update.php'), '--cron'])
    ->name('filebeam:process-updates')
    ->everyMinute()
    ->when(fn (): bool => config('version.distribution') === 'package')
    ->withoutOverlapping(60);
