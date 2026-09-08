<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Enums\ReportStatus;
use App\Models\FileReport;
use App\Models\Transfer;
use App\Models\User;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Gate;
use Illuminate\Validation\ValidationException;
use Throwable;

class TakeDownReportedTransfer
{
    /**
     * @throws Throwable
     */
    public function handle(User $actor, FileReport $report, string $reason): void
    {
        $actor = $actor->fresh() ?? throw new AuthorizationException;
        Gate::forUser($actor)->authorize('takedown', $report);

        DB::transaction(function () use ($actor, $report, $reason): void {
            $report = FileReport::query()->findOrFail($report->id);

            if ($report->transfer_id === null) {
                throw ValidationException::withMessages(['transfer' => 'This upload has already been deleted.']);
            }

            // Always acquire the upload lock before its report to match deletion work.
            $transfer = Transfer::query()->lockForUpdate()->find($report->transfer_id);

            if ($transfer === null) {
                throw ValidationException::withMessages(['transfer' => 'This upload has already been deleted.']);
            }

            $report = FileReport::query()->lockForUpdate()->findOrFail($report->id);

            app(RequestTransferTakedown::class)->handle($actor, $transfer, $reason);
            app(ReviewFileReport::class)->handle($actor, $report, ReportStatus::Resolved, $reason);
        }, attempts: 3);
    }
}
