<?php

declare(strict_types=1);

namespace App\Jobs;

use App\Enums\TransferStatus;
use App\Models\Transfer;
use App\Support\ChunkStaging;
use App\Support\FilestoreRegistry;
use Illuminate\Contracts\Queue\ShouldQueue;
use Illuminate\Foundation\Queue\Queueable;
use Illuminate\Support\Facades\DB;
use RuntimeException;
use Throwable;

class DeleteTransfer implements ShouldQueue
{
    use Queueable;

    public int $tries = 5;

    /** @var list<int> */
    public array $backoff = [30, 120, 600, 1800];

    public function __construct(public string $transferId) {}

    /**
     * Execute the job.
     *
     * @throws Throwable
     */
    public function handle(): void
    {
        $transfer = Transfer::query()
            ->with('items.chunks.locations.filestore')
            ->awaitingCleanup()
            ->find($this->transferId);

        if ($transfer === null) {
            return;
        }

        if (! app(ChunkStaging::class)->removeTransfer($transfer->id)) {
            throw new RuntimeException("Staged ciphertext for transfer {$transfer->id} is still active.");
        }

        $locations = collect();
        foreach ($transfer->items as $item) {
            foreach ($item->chunks as $chunk) {
                $locations = $locations->concat($chunk->locations);
            }
        }

        foreach ($locations->groupBy('filestore_id') as $copies) {
            if (app(FilestoreRegistry::class)->disk($copies->first()->filestore)->delete($copies->pluck('storage_path')->all()) !== true) {
                throw new RuntimeException("Unable to remove ciphertext for transfer {$transfer->id}.");
            }
        }

        DB::transaction(function (): void {
            $transfer = Transfer::query()->lockForUpdate()->find($this->transferId);

            if ($transfer === null || $transfer->status !== TransferStatus::Deleting) {
                return;
            }

            $transfer->delete();
        }, attempts: 3);
    }
}
