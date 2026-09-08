<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Models\AdminAudit;
use App\Models\FileReport;
use App\Models\User;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Gate;
use Illuminate\Validation\ValidationException;
use Throwable;

class AssignFileReport
{
    /**
     * @throws Throwable
     */
    public function handle(User $actor, FileReport $report, ?int $assigneeId): void
    {
        $actor = $this->authorizedActor($actor);
        Gate::forUser($actor)->authorize('assign', $report);
        $assignee = $this->validatedAssignee($assigneeId);

        DB::transaction(function () use ($actor, $report, $assignee): void {
            $report = FileReport::query()->lockForUpdate()->findOrFail($report->id);
            $previousAssignee = $report->assigned_to;

            if ($previousAssignee === $assignee?->id) {
                return;
            }

            $report->update(['assigned_to' => $assignee?->id]);

            AdminAudit::query()->create([
                'actor_id' => $actor->id,
                'action' => 'file_report.assigned',
                'target_type' => FileReport::class,
                'target_id' => $report->id,
                'changes' => ['assigned_to' => ['from' => $previousAssignee, 'to' => $assignee?->id]],
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

    private function validatedAssignee(?int $assigneeId): ?User
    {
        if ($assigneeId === null) {
            return null;
        }

        $assignee = User::query()->find($assigneeId);

        if ($assignee === null || ! $assignee->isStaff()) {
            throw ValidationException::withMessages(['assigned_to' => 'The assignee must be an active, verified staff member.']);
        }

        return $assignee;
    }
}
