<?php

declare(strict_types=1);

namespace App\Console\Commands;

use App\Services\Changelog\ReleaseSynchronizer;
use Illuminate\Console\Attributes\Description;
use Illuminate\Console\Attributes\Signature;
use Illuminate\Console\Command;

#[Signature('filebeam:sync-changelog {--full : Reconcile the complete release archive} {--force : Ignore the failure backoff window}')]
#[Description('Mirror published GitHub release notes for the public changelog')]
class SyncChangelog extends Command
{
    public function handle(ReleaseSynchronizer $synchronizer): int
    {
        $result = $synchronizer->sync((bool) $this->option('full'), (bool) $this->option('force'));

        $message = match ($result['status']) {
            'synced' => sprintf(
                'Release notes synchronized (%d new, %d updated, %d hidden%s).',
                $result['imported'] ?? 0,
                $result['updated'] ?? 0,
                $result['hidden'] ?? 0,
                ($result['complete'] ?? false) ? '' : ', archive scan incomplete',
            ),
            'skipped' => 'Release note synchronization is skipped: '.($result['reason'] ?? 'disabled').'.',
            'locked' => 'Another release note synchronization is running.',
            'backoff' => 'Waiting before retrying after: '.($result['reason'] ?? 'an earlier failure').'. Use --force to retry now.',
            'failed' => 'Release note synchronization failed: '.($result['reason'] ?? 'unknown').'. Saved notes remain available.',
        };

        if ($result['status'] === 'failed') {
            $this->error($message);

            return self::FAILURE;
        }

        $this->info($message);

        return self::SUCCESS;
    }
}
