<?php

declare(strict_types=1);

namespace App\Actions\Transfers;

use App\Enums\TransferDriver;
use App\Enums\TransferRemovalReason;
use App\Enums\TransferStatus;
use App\Jobs\DeleteTransfer;
use App\Models\Transfer;
use Illuminate\Support\Facades\Cache;
use Illuminate\Support\Facades\DB;

class RemoveTransfer
{
    public function handle(Transfer $transfer, TransferRemovalReason $reason): void
    {
        DB::transaction(function () use ($transfer, $reason): void {
            $locked = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);
            if ($locked->status === TransferStatus::Deleting) {
                return;
            }
            $locked->update(['status' => TransferStatus::Deleting, 'removal_reason' => $reason]);
            DB::afterCommit(function () use ($locked): void {
                if ($locked->driver === TransferDriver::WebRtc) {
                    Cache::forget("filebeam:webrtc:{$locked->id}:sessions");
                    Cache::forget("filebeam:webrtc:{$locked->id}:sender");
                }
                DeleteTransfer::dispatch($locked->id);
            });
        });
    }
}
