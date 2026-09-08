<?php

declare(strict_types=1);

use App\Actions\Admin\AddReportNote;
use App\Actions\Admin\AssignFileReport;
use App\Actions\Admin\ReviewFileReport;
use App\Actions\Admin\TakeDownReportedTransfer;
use App\Enums\ReportStatus;
use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Filament\Resources\FileReports\FileReportResource;
use App\Filament\Resources\FileReports\Pages\ListFileReports;
use App\Filament\Resources\FileReports\Pages\ViewFileReport;
use App\Filament\Resources\FileReports\RelationManagers\ActivityRelationManager;
use App\Filament\Resources\FileReports\RelationManagers\NotesRelationManager;
use App\Models\AdminAudit;
use App\Models\FileReport;
use App\Models\ReportNote;
use App\Models\Transfer;
use App\Models\User;
use Filament\Actions\Testing\TestAction;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Support\Facades\Gate;
use Illuminate\Support\Facades\Queue;
use Livewire\Livewire;

function reportWorkspaceStaff(): User
{
    return User::factory()->create(['role' => UserRole::Moderator]);
}

test('the case workspace uses a view URL and does not expose upload secrets', function () {
    $staff = reportWorkspaceStaff();
    $transfer = Transfer::factory()->create(['encrypted_manifest' => 'never-render-this-secret']);
    $report = FileReport::factory()->for($transfer)->create();

    $this->actingAs($staff, 'admin')->get(FileReportResource::getUrl('view', ['record' => $report]))
        ->assertOk()
        ->assertDontSee('never-render-this-secret');
});

test('assignment preserves a resolved decision', function () {
    $actor = reportWorkspaceStaff();
    $assignee = reportWorkspaceStaff();
    $report = FileReport::factory()->create([
        'status' => ReportStatus::Resolved,
        'resolution' => 'Prior decision',
        'resolved_at' => now(),
    ]);

    app(AssignFileReport::class)->handle($actor, $report, $assignee->id);

    expect($report->refresh())
        ->status->toBe(ReportStatus::Resolved)
        ->resolution->toBe('Prior decision')
        ->assigned_to->toBe($assignee->id);
});

test('a stale reviewer decision preserves a newer assignment', function () {
    $actor = reportWorkspaceStaff();
    $newAssignee = reportWorkspaceStaff();
    $report = FileReport::factory()->create();
    $staleReport = FileReport::query()->findOrFail($report->id);

    app(AssignFileReport::class)->handle($actor, $report, $newAssignee->id);
    app(ReviewFileReport::class)->handle($actor, $staleReport, ReportStatus::Resolved, 'Confirmed after review');

    expect($report->refresh())
        ->assigned_to->toBe($newAssignee->id)
        ->status->toBe(ReportStatus::Resolved);
    expect(AdminAudit::query()->where('action', 'file_report.reviewed')->sole()->changes)
        ->toHaveKeys(['status', 'resolution', 'resolved_at'])
        ->not->toHaveKey('assigned_to');
});

test('notes are staff-only and append-only', function () {
    $report = FileReport::factory()->create();

    expect(fn () => app(AddReportNote::class)->handle(User::factory()->create(), $report, 'Internal context'))->toThrow(AuthorizationException::class);

    app(AddReportNote::class)->handle(reportWorkspaceStaff(), $report, '  Internal context  ');

    $note = ReportNote::query()->sole();

    expect($note)->body->toBe('Internal context');
    expect(fn () => Gate::forUser(reportWorkspaceStaff())->authorize('delete', $note))->toThrow(AuthorizationException::class);
});

test('closed reports require an explicit audited reopen before another decision', function () {
    $staff = reportWorkspaceStaff();
    $report = FileReport::factory()->create();
    $review = app(ReviewFileReport::class);

    $review->handle($staff, $report, ReportStatus::Resolved, 'Confirmed');
    $review->reopen($staff, $report, 'New evidence received');

    expect($report->refresh())
        ->status->toBe(ReportStatus::Open)
        ->resolution->toBeNull()
        ->resolved_at->toBeNull();
    expect(AdminAudit::query()->latest('id')->first())
        ->action->toBe('file_report.reopened')
        ->reason->toBe('New evidence received');
});

test('taking down a reported upload queues deletion and resolves only this report', function () {
    Queue::fake();
    $staff = reportWorkspaceStaff();
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Available]);
    $report = FileReport::factory()->for($transfer)->create();
    $sibling = FileReport::factory()->for($transfer)->create();

    app(TakeDownReportedTransfer::class)->handle($staff, $report, 'Copyright confirmed');

    expect($transfer->refresh()->status)->toBe(TransferStatus::Deleting)
        ->and($report->refresh()->status)->toBe(ReportStatus::Resolved)
        ->and($sibling->refresh()->status)->toBe(ReportStatus::Open);
});

test('active queues exclude closed cases and show oldest active reports first', function () {
    $staff = reportWorkspaceStaff();
    $mine = FileReport::factory()->create(['assigned_to' => $staff->id, 'created_at' => now()->subHour()]);
    $unassigned = FileReport::factory()->create(['assigned_to' => null]);
    $closedMine = FileReport::factory()->create(['assigned_to' => $staff->id, 'status' => ReportStatus::Resolved, 'resolved_at' => now()]);

    $this->actingAs($staff, 'admin');

    Livewire::test(ListFileReports::class)
        ->set('activeTab', 'mine')
        ->assertCanSeeTableRecords([$mine])
        ->assertCanNotSeeTableRecords([$unassigned, $closedMine])
        ->set('activeTab', 'unassigned')
        ->assertCanSeeTableRecords([$unassigned])
        ->assertCanNotSeeTableRecords([$mine, $closedMine]);
});

test('staff can assign, resolve, reopen, and add an internal note from the case workspace', function () {
    $staff = reportWorkspaceStaff();
    $report = FileReport::factory()->create();

    $this->actingAs($staff, 'admin');

    Livewire::test(ViewFileReport::class, ['record' => $report->id])
        ->assertActionExists('assignToMe')
        ->assertActionExists('startReview')
        ->callAction('assignToMe')
        ->callAction('startReview')
        ->callAction('resolve', ['resolution' => 'No policy violation found'])
        ->callAction('reopen', ['reason' => 'Additional evidence received'])
        ->assertHasNoFormErrors();

    expect($report->refresh()->assigned_to)->toBe($staff->id)
        ->and($report->status)->toBe(ReportStatus::Open);

    Livewire::test(NotesRelationManager::class, [
        'ownerRecord' => $report,
        'pageClass' => ViewFileReport::class,
    ])->callAction(TestAction::make('addNote')->table(), ['body' => 'Internal follow-up required']);

    expect(ReportNote::query()->where('file_report_id', $report->id)->sole()->body)->toBe('Internal follow-up required');
});

test('case queue return is allowlisted and non-staff cannot access note or activity tabs', function () {
    $report = FileReport::factory()->create();
    $staff = reportWorkspaceStaff();

    $this->actingAs($staff, 'admin');

    Livewire::test(ViewFileReport::class, ['record' => $report->id, 'queueTab' => 'mine'])
        ->assertActionExists('returnToQueue')
        ->assertActionHasUrl('returnToQueue', FileReportResource::getUrl('index', ['tab' => 'mine']));

    Livewire::test(ViewFileReport::class, ['record' => $report->id, 'queueTab' => 'https://example.test'])
        ->assertActionHidden('returnToQueue');

    $this->actingAs(User::factory()->create(), 'admin');

    Livewire::test(ViewFileReport::class, ['record' => $report->id])->assertForbidden();
});

test('moderators see only case moderation activity and non-staff are forbidden from the activity relation', function () {
    $staff = reportWorkspaceStaff();
    $report = FileReport::factory()->create();
    $included = AdminAudit::factory()->create([
        'actor_id' => $staff->id,
        'action' => 'file_report.reviewed',
        'target_type' => FileReport::class,
        'target_id' => $report->id,
    ]);
    $excluded = AdminAudit::factory()->create([
        'actor_id' => $staff->id,
        'action' => 'user.suspended',
        'target_type' => FileReport::class,
        'target_id' => $report->id,
    ]);

    $this->actingAs($staff, 'admin');

    $case = Livewire::test(ViewFileReport::class, ['record' => $report->id]);
    expect($case->instance()->getRelationManagers())->toContain(ActivityRelationManager::class);

    Livewire::test(ActivityRelationManager::class, [
        'ownerRecord' => $report,
        'pageClass' => ViewFileReport::class,
    ])
        ->assertCanSeeTableRecords([$included])
        ->assertCanNotSeeTableRecords([$excluded]);

    $this->actingAs(User::factory()->create(), 'admin');

    Livewire::test(ActivityRelationManager::class, [
        'ownerRecord' => $report,
        'pageClass' => ViewFileReport::class,
    ])->assertForbidden();
});

test('notes relation rejects non-staff and exposes no mutation actions for existing notes', function () {
    $staff = reportWorkspaceStaff();
    $report = FileReport::factory()->create();
    $note = ReportNote::factory()->for($report, 'report')->for($staff, 'author')->create();

    $this->actingAs($staff, 'admin');

    Livewire::test(NotesRelationManager::class, [
        'ownerRecord' => $report,
        'pageClass' => ViewFileReport::class,
    ])
        ->assertActionDoesNotExist(TestAction::make('edit')->table($note))
        ->assertActionDoesNotExist(TestAction::make('delete')->table($note));

    $this->actingAs(User::factory()->create(), 'admin');

    Livewire::test(NotesRelationManager::class, [
        'ownerRecord' => $report,
        'pageClass' => ViewFileReport::class,
    ])->assertForbidden();
});
