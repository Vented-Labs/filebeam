<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Models\FileReport;
use App\Models\ReportNote;
use App\Models\User;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Gate;
use Illuminate\Validation\ValidationException;
use Throwable;

class AddReportNote
{
    /**
     * @throws Throwable
     */
    public function handle(User $actor, FileReport $report, string $body): void
    {
        $actor = $actor->fresh();

        if ($actor === null || ! $actor->isStaff()) {
            throw new AuthorizationException;
        }

        Gate::forUser($actor)->authorize('create', ReportNote::class);

        $body = trim($body);

        if (blank($body) || mb_strlen($body) > 5000) {
            throw ValidationException::withMessages(['body' => 'A note is required and may not be greater than 5000 characters.']);
        }

        DB::transaction(function () use ($actor, $report, $body): void {
            $report = FileReport::query()->lockForUpdate()->findOrFail($report->id);

            ReportNote::query()->create([
                'file_report_id' => $report->id,
                'author_id' => $actor->id,
                'body' => $body,
            ]);
        });
    }
}
