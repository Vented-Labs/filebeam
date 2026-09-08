<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Enums\ReportStatus;
use App\Models\AdminAudit;
use App\Models\FileReport;
use App\Models\User;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Gate;
use Illuminate\Validation\ValidationException;
use Throwable;

class ReviewFileReport
{
    /**
     * @throws Throwable
     */
    public function handle(User $actor, FileReport $report, ReportStatus $status, ?string $resolution): void
    {
        $actor = $this->authorizedActor($actor);
        Gate::forUser($actor)->authorize('review', $report);
        $resolution = $this->validatedResolution($status, $resolution);

        DB::transaction(function () use ($actor, $report, $status, $resolution): void {
            $report = FileReport::query()->lockForUpdate()->findOrFail($report->id);
            $this->ensureTransitionIsAllowed($report, $status);
            $resolvedAt = match ($status) {
                ReportStatus::Resolved, ReportStatus::Dismissed => now(),
                default => null,
            };
            $changes = [
                'status' => ['from' => $report->status->value, 'to' => $status->value],
                'resolution' => ['from' => $report->resolution, 'to' => $resolution],
                'resolved_at' => ['from' => $report->resolved_at?->toISOString(), 'to' => $resolvedAt?->toISOString()],
            ];

            $report->update([
                'status' => $status,
                'resolution' => $resolution,
                'resolved_at' => $resolvedAt,
            ]);

            AdminAudit::query()->create([
                'actor_id' => $actor->id,
                'action' => 'file_report.reviewed',
                'target_type' => FileReport::class,
                'target_id' => $report->id,
                'reason' => $resolution,
                'changes' => $changes,
            ]);
        });

        $report->refresh();
    }

    private function authorizedActor(User $actor): User
    {
        $actor = $actor->fresh();

        if ($actor === null || ! $actor->isStaff()) {
            throw new AuthorizationException;
        }

        return $actor;
    }

    private function validatedResolution(ReportStatus $status, ?string $resolution): ?string
    {
        $resolution = $resolution === null ? null : trim($resolution);

        if (in_array($status, [ReportStatus::Resolved, ReportStatus::Dismissed], true) && blank($resolution)) {
            throw ValidationException::withMessages(['resolution' => 'A resolution is required when closing a report.']);
        }

        if ($resolution !== null && mb_strlen($resolution) > 5000) {
            throw ValidationException::withMessages(['resolution' => 'The resolution may not be greater than 5000 characters.']);
        }

        return $resolution ?: null;
    }

    private function ensureTransitionIsAllowed(FileReport $report, ReportStatus $status): void
    {
        if (in_array($report->status, [ReportStatus::Resolved, ReportStatus::Dismissed], true)) {
            throw ValidationException::withMessages(['status' => 'Closed reports must be explicitly reopened before they can be reviewed.']);
        }

        if (! in_array($status, [ReportStatus::InReview, ReportStatus::Resolved, ReportStatus::Dismissed], true)) {
            throw ValidationException::withMessages(['status' => 'Reports can only be started, resolved, or dismissed during review.']);
        }
    }

    /**
     * @throws Throwable
     */
    public function reopen(User $actor, FileReport $report, string $reason): void
    {
        $actor = $this->authorizedActor($actor);
        Gate::forUser($actor)->authorize('review', $report);
        $reason = $this->validatedReopenReason($reason);

        DB::transaction(function () use ($actor, $report, $reason): void {
            $report = FileReport::query()->lockForUpdate()->findOrFail($report->id);

            if (! in_array($report->status, [ReportStatus::Resolved, ReportStatus::Dismissed], true)) {
                throw ValidationException::withMessages(['status' => 'Only closed reports can be reopened.']);
            }

            $changes = [
                'status' => ['from' => $report->status->value, 'to' => ReportStatus::Open->value],
                'resolution' => ['from' => $report->resolution, 'to' => null],
                'resolved_at' => ['from' => $report->resolved_at?->toISOString(), 'to' => null],
            ];

            $report->update([
                'status' => ReportStatus::Open,
                'resolution' => null,
                'resolved_at' => null,
            ]);

            AdminAudit::query()->create([
                'actor_id' => $actor->id,
                'action' => 'file_report.reopened',
                'target_type' => FileReport::class,
                'target_id' => $report->id,
                'reason' => $reason,
                'changes' => $changes,
            ]);
        });

        $report->refresh();
    }

    private function validatedReopenReason(string $reason): string
    {
        $reason = trim($reason);

        if (blank($reason)) {
            throw ValidationException::withMessages(['reason' => 'A reason is required when reopening a report.']);
        }

        if (mb_strlen($reason) > 5000) {
            throw ValidationException::withMessages(['reason' => 'The reason may not be greater than 5000 characters.']);
        }

        return $reason;
    }
}
