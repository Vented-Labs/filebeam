<?php

declare(strict_types=1);

use App\Actions\Admin\AssignFileReport;
use App\Actions\Admin\RequestTransferTakedown;
use App\Actions\Admin\RetryTransferCleanup;
use App\Actions\Admin\ReviewFileReport;
use App\Enums\ReportStatus;
use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Jobs\DeleteTransfer;
use App\Models\AdminAudit;
use App\Models\FileReport;
use App\Models\Transfer;
use App\Models\User;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\Queue;
use Illuminate\Validation\ValidationException;

function moderationStaffUser(): User
{
    return User::factory()->create(['role' => UserRole::Moderator]);
}

test('staff can request a transfer takedown once and it is audited', function () {
    Queue::fake();
    $actor = moderationStaffUser();
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Available]);

    app(RequestTransferTakedown::class)->handle($actor, $transfer, '  Copyright infringement  ');
    app(RequestTransferTakedown::class)->handle($actor, $transfer, 'Duplicate request');

    expect($transfer->refresh()->status)->toBe(TransferStatus::Deleting);
    expect(AdminAudit::query()->where('action', 'transfer.takedown_requested')->count())->toBe(1);
    expect(AdminAudit::query()->sole()->reason)->toBe('Copyright infringement');
    Queue::assertPushed(DeleteTransfer::class, 1);
});

test('only staff can request a transfer takedown', function () {
    Queue::fake();
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Available]);

    expect(fn () => app(RequestTransferTakedown::class)->handle(User::factory()->create(), $transfer, 'Abuse'))->toThrow(AuthorizationException::class);

    expect($transfer->refresh()->status)->toBe(TransferStatus::Available);
    expect(AdminAudit::query()->count())->toBe(0);
    Queue::assertNothingPushed();
});

test('cleanup retries require a deleting transfer and create an audit entry', function () {
    Queue::fake();
    $actor = moderationStaffUser();
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);

    app(RetryTransferCleanup::class)->handle($actor, $transfer, 'Storage recovered');

    expect(AdminAudit::query()->sole())
        ->action->toBe('transfer.cleanup_retried')
        ->reason->toBe('Storage recovered');
    Queue::assertPushed(DeleteTransfer::class, fn (DeleteTransfer $job): bool => $job->transferId === $transfer->id);

    $availableTransfer = Transfer::factory()->create(['status' => TransferStatus::Available]);

    expect(fn () => app(RetryTransferCleanup::class)->handle($actor, $availableTransfer, 'Retry'))->toThrow(ValidationException::class);
});

test('staff can assign then review reports with an active verified staff assignee', function () {
    $actor = moderationStaffUser();
    $assignee = moderationStaffUser();
    $report = FileReport::factory()->create();

    app(AssignFileReport::class)->handle($actor, $report, $assignee->id);
    app(ReviewFileReport::class)->handle($actor, $report, ReportStatus::Resolved, 'Confirmed and removed.');

    expect($report->refresh())
        ->status->toBe(ReportStatus::Resolved)
        ->assigned_to->toBe($assignee->id)
        ->resolution->toBe('Confirmed and removed.')
        ->resolved_at->not->toBeNull();
    expect(AdminAudit::query()->where('action', 'file_report.reviewed')->sole())
        ->action->toBe('file_report.reviewed')
        ->changes->toHaveKeys(['status', 'resolution', 'resolved_at']);
});

test('report review requires a resolution when closing', function () {
    $actor = moderationStaffUser();
    $report = FileReport::factory()->create();

    expect(fn () => app(ReviewFileReport::class)->handle($actor, $report, ReportStatus::Dismissed, null))->toThrow(ValidationException::class);
    expect(fn () => app(ReviewFileReport::class)->handle(User::factory()->create(), $report, ReportStatus::InReview, null))->toThrow(AuthorizationException::class);
});

test('transfer deletion retains reports and their transfer identifier', function () {
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    $report = FileReport::factory()->for($transfer)->create(['transfer_identifier' => $transfer->id]);

    (new DeleteTransfer($transfer->id))->handle();

    expect($report->refresh())
        ->transfer_id->toBeNull()
        ->transfer_identifier->toBe($transfer->id);
});
