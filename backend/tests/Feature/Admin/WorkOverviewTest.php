<?php

declare(strict_types=1);

use App\Enums\ReportStatus;
use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Filament\Resources\AdminAudits\AdminAuditResource;
use App\Filament\Resources\AdminAudits\Pages\ViewAdminAudit;
use App\Filament\Resources\FileReports\FileReportResource;
use App\Filament\Resources\FileReports\Pages\ListFileReports;
use App\Filament\Resources\Transfers\TransferResource;
use App\Filament\Resources\Users\Pages\ViewUser;
use App\Filament\Resources\Users\RelationManagers\OwnedTransfersRelationManager;
use App\Filament\Support\AuditPresentation;
use App\Filament\Widgets\ActiveReports;
use App\Filament\Widgets\ModerationOverview;
use App\Models\AdminAudit;
use App\Models\FileReport;
use App\Models\Transfer;
use App\Models\User;
use Filament\Facades\Filament;
use Livewire\Livewire;

beforeEach(function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
});

test('overview links to active work and the oldest report table excludes closed cases', function (): void {
    $staff = User::factory()->create(['role' => UserRole::Moderator]);
    $mine = FileReport::factory()->create(['assigned_to' => $staff->id]);
    $unassigned = FileReport::factory()->create(['assigned_to' => null, 'created_at' => now()->subDay()]);
    $closed = FileReport::factory()->create(['status' => ReportStatus::Resolved]);
    Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    $this->actingAs($staff, 'admin');

    Livewire::test(ModerationOverview::class)
        ->assertSeeHtml(FileReportResource::getUrl('index', ['tab' => 'mine']))
        ->assertSeeHtml(FileReportResource::getUrl('index', ['tab' => 'unassigned']))
        ->assertSeeHtml(TransferResource::getUrl('index', ['tab' => 'deleting']));
    Livewire::test(ActiveReports::class)
        ->assertCanSeeTableRecords([$unassigned, $mine], inOrder: true)
        ->assertCanNotSeeTableRecords([$closed]);

    $this->get(TransferResource::getUrl('index', ['tab' => 'deleting']))->assertOk();
});

test('case links keep the current queue after a Livewire tab change', function (): void {
    $staff = User::factory()->create(['role' => UserRole::Moderator]);
    $report = FileReport::factory()->create(['assigned_to' => $staff->id]);
    $this->actingAs($staff, 'admin');

    Livewire::test(ListFileReports::class)
        ->set('activeTab', 'mine')
        ->assertTableActionHasUrl('view', FileReportResource::getUrl('view', ['record' => $report, 'queue' => 'mine']), $report);
});

test('audit detail shows readable changes and never exposes unknown or credential fields', function (): void {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    $assignee = User::factory()->create(['role' => UserRole::Moderator]);
    $report = FileReport::factory()->create();
    $audit = AdminAudit::factory()->create([
        'target_type' => FileReport::class,
        'target_id' => $report->id,
        'action' => 'file_report.assigned',
        'changes' => [
            'assigned_to' => ['from' => null, 'to' => $assignee->id],
            'password' => ['from' => 'do-not-render-this', 'to' => 'nor-this'],
            'unknown_field' => 'also-private',
        ],
    ]);
    $this->actingAs($admin, 'admin');

    Livewire::test(ViewAdminAudit::class, ['record' => $audit->id])
        ->assertSee('Assignment changed')->assertSee('Unassigned')->assertSee($assignee->email)
        ->assertDontSee('do-not-render-this')->assertDontSee('nor-this')->assertDontSee('also-private');
    expect(AuditPresentation::changes($audit))->toHaveCount(1);

    $this->actingAs($assignee, 'admin')->get(AdminAuditResource::getUrl('view', ['record' => $audit]))->assertForbidden();
});

test('account upload relations require account access even when the viewer can moderate uploads', function (): void {
    $moderator = User::factory()->create(['role' => UserRole::Moderator]);
    $owner = User::factory()->create();
    $upload = Transfer::factory()->for($owner, 'owner')->create();
    $this->actingAs($moderator, 'admin');

    Livewire::test(OwnedTransfersRelationManager::class, ['ownerRecord' => $owner, 'pageClass' => ViewUser::class])->assertForbidden();

    $this->actingAs(User::factory()->create(['role' => UserRole::Admin]), 'admin');
    Livewire::test(OwnedTransfersRelationManager::class, ['ownerRecord' => $owner, 'pageClass' => ViewUser::class])->assertCanSeeTableRecords([$upload]);
});
