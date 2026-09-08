<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Enums\TransferStatus;
use App\Jobs\DeleteTransfer;
use App\Models\AdminAudit;
use App\Models\Transfer;
use App\Models\User;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Validator;
use Throwable;

class RequestTransferTakedown
{
    /**
     * @throws Throwable
     */
    public function handle(User $actor, Transfer $transfer, string $reason): void
    {
        $this->authorize($actor);
        $reason = $this->validatedReason($reason);

        DB::transaction(function () use ($actor, $transfer, $reason): void {
            $transfer = Transfer::query()->lockForUpdate()->findOrFail($transfer->id);

            if ($transfer->status === TransferStatus::Deleting) {
                return;
            }

            $previousStatus = $transfer->status;
            $transfer->update(['status' => TransferStatus::Deleting]);

            AdminAudit::query()->create([
                'actor_id' => $actor->id,
                'action' => 'transfer.takedown_requested',
                'target_type' => Transfer::class,
                'target_id' => $transfer->id,
                'reason' => $reason,
                'changes' => ['status' => ['from' => $previousStatus->value, 'to' => TransferStatus::Deleting->value]],
            ]);

            DeleteTransfer::dispatch($transfer->id)->afterCommit();
        });
    }

    private function authorize(User $actor): void
    {
        if (! $actor->isStaff()) {
            throw new AuthorizationException;
        }
    }

    private function validatedReason(string $reason): string
    {
        $reason = trim($reason);

        return Validator::validate(['reason' => $reason], ['reason' => ['required', 'string', 'max:2000']])['reason'];
    }
}
