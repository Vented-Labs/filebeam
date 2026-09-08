<?php

declare(strict_types=1);

namespace App\Console\Commands;

use App\Enums\TransferStatus;
use App\Jobs\DeleteTransfer;
use App\Models\Transfer;
use App\Models\TransferChunkUpload;
use App\Support\FilestoreRegistry;
use Filebeam\Updater\ActivityLock;
use Illuminate\Console\Attributes\Description;
use Illuminate\Console\Attributes\Signature;
use Illuminate\Console\Command;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Foundation\Bus\PendingDispatch;
use Illuminate\Support\Facades\DB;
use RuntimeException;
use Throwable;

#[Signature('filebeam:prune-transfers')]
#[Description('Queue expired and abandoned encrypted transfers for deletion')]
class PruneTransfers extends Command
{
    /**
     * Execute the console command.
     */
    public function handle(): int
    {
        $activityLock = null;

        if (config('version.distribution') === 'package') {
            try {
                $activityLock = (new ActivityLock(storage_path('app/update-activity.lock')))->acquireShared();
            } catch (RuntimeException) {
                $this->error('Transfer pruning is unavailable while an update is in progress.');

                return self::FAILURE;
            }
        }

        try {
            $this->reapUploadAttempts();
            $now = now();
            $staleDeletingBefore = $now->copy()->subMinutes(15);
            $queued = 0;

            Transfer::query()
                ->where(function (Builder $query) use ($now, $staleDeletingBefore): void {
                    $query
                        ->where(fn (Builder $pending): Builder => $pending
                            ->where('status', TransferStatus::Pending->value)
                            ->where('expires_at', '<=', $now))
                        ->orWhere(fn (Builder $available): Builder => $available
                            ->where('status', TransferStatus::Available->value)
                            ->where('expires_at', '<=', $now))
                        ->orWhere(fn (Builder $deleting): Builder => $deleting
                            ->where('status', TransferStatus::Deleting->value)
                            ->where('updated_at', '<=', $staleDeletingBefore));
                })
                ->select('id')
                ->chunkById(100, function ($transfers) use (&$queued, $now, $staleDeletingBefore): void {
                    foreach ($transfers as $transfer) {
                        $dispatched = DB::transaction(function () use ($transfer, $now, $staleDeletingBefore): bool {
                            $lockedTransfer = Transfer::query()->lockForUpdate()->find($transfer->id);

                            if ($lockedTransfer === null) {
                                return false;
                            }

                            $isEligible = match ($lockedTransfer->status) {
                                TransferStatus::Pending, TransferStatus::Available => $lockedTransfer->expires_at->lessThanOrEqualTo($now),
                                TransferStatus::Deleting => $lockedTransfer->updated_at->lessThanOrEqualTo($staleDeletingBefore),
                            };

                            if (! $isEligible) {
                                return false;
                            }

                            if ($lockedTransfer->status !== TransferStatus::Deleting) {
                                $lockedTransfer->update(['status' => TransferStatus::Deleting]);
                            } else {
                                $lockedTransfer->touch();
                            }

                            DB::afterCommit(fn (): PendingDispatch => DeleteTransfer::dispatch($lockedTransfer->id));

                            return true;
                        }, attempts: 3);

                        if ($dispatched) {
                            $queued++;
                        }
                    }
                });

            $this->info("Queued {$queued} transfer(s) for deletion.");

            return self::SUCCESS;
        } finally {
            if (is_resource($activityLock)) {
                (new ActivityLock(storage_path('app/update-activity.lock')))->release($activityLock);
            }
        }
    }

    private function reapUploadAttempts(): void
    {
        TransferChunkUpload::query()
            ->where('valid_until', '<=', now())
            ->orderBy('id')
            ->chunkById(100, function ($attempts): void {
                foreach ($attempts as $attempt) {
                    $claimedAttempt = DB::transaction(function () use ($attempt): ?TransferChunkUpload {
                        $lockedAttempt = TransferChunkUpload::query()->lockForUpdate()->find($attempt->id);

                        if ($lockedAttempt === null || $lockedAttempt->valid_until->isFuture()) {
                            return null;
                        }

                        if ($lockedAttempt->is_reaping && $lockedAttempt->cleanup_started_at?->greaterThan(now()->subMinutes(15))) {
                            return null;
                        }

                        $lockedAttempt->update([
                            'is_reaping' => true,
                            'cleanup_started_at' => now(),
                        ]);

                        return $lockedAttempt;
                    });

                    if ($claimedAttempt === null) {
                        continue;
                    }

                    try {
                        $deleted = app(FilestoreRegistry::class)->disk($claimedAttempt->filestore)->delete($claimedAttempt->storage_path);
                    } catch (Throwable) {
                        $deleted = false;
                    }

                    if ($deleted === true) {
                        TransferChunkUpload::query()
                            ->whereKey($claimedAttempt->id)
                            ->where('is_reaping', true)
                            ->delete();

                        continue;
                    }

                    TransferChunkUpload::query()
                        ->whereKey($claimedAttempt->id)
                        ->where('is_reaping', true)
                        ->update(['is_reaping' => false, 'cleanup_started_at' => null]);
                }
            });
    }
}
