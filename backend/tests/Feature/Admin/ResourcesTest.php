<?php

declare(strict_types=1);

use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Filament\Resources\AdminAudits\AdminAuditResource;
use App\Filament\Resources\AdminAudits\Pages\ListAdminAudits;
use App\Filament\Resources\FileReports\FileReportResource;
use App\Filament\Resources\FileReports\Pages\ListFileReports;
use App\Filament\Resources\Transfers\Pages\ListTransfers;
use App\Filament\Resources\Transfers\TransferResource;
use App\Filament\Resources\Users\Pages\ListUsers;
use App\Filament\Resources\Users\UserResource;
use App\Models\AdminAudit;
use App\Models\FileReport;
use App\Models\Transfer;
use App\Models\User;
use Filament\Actions\Testing\TestAction;
use Filament\Facades\Filament;
use Illuminate\Support\Facades\Queue;
use Livewire\Livewire;

function resourceStaff(): User
{
    return User::factory()->create(['role' => UserRole::Moderator]);
}

function resourceAdmin(): User
{
    return User::factory()->create(['role' => UserRole::Admin]);
}

test('staff can render the upload and file report tables', function () {
    $staff = resourceStaff();
    $transfer = Transfer::factory()->for($staff, 'owner')->create();
    $report = FileReport::factory()->for($transfer)->create();

    $this->actingAs($staff, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListTransfers::class)
        ->assertCanSeeTableRecords([$transfer])
        ->assertTableActionExists('view', null, $transfer)
        ->assertTableActionExists('takedown', null, $transfer);

    Livewire::test(ListFileReports::class)
        ->assertCanSeeTableRecords([$report])
        ->assertTableActionExists('view', null, $report)
        ->assertTableActionExists('assign', null, $report)
        ->assertTableActionExists('resolve', null, $report);
});

test('staff can request a takedown from the upload table and it is audited', function () {
    Queue::fake();
    $staff = resourceStaff();
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Available]);

    $this->actingAs($staff, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListTransfers::class)
        ->callTableAction('takedown', $transfer, ['reason' => 'Copyright infringement'])
        ->assertHasNoActionErrors();

    expect($transfer->refresh()->status)->toBe(TransferStatus::Deleting);
    $this->assertDatabaseHas('admin_audits', [
        'actor_id' => $staff->id,
        'action' => 'transfer.takedown_requested',
        'target_id' => $transfer->id,
        'reason' => 'Copyright infringement',
    ]);
});

test('staff can take down an upload from a report and resolve the report', function () {
    Queue::fake();
    $staff = resourceStaff();
    $transfer = Transfer::factory()->create(['status' => TransferStatus::Available]);
    $report = FileReport::factory()->for($transfer)->create();

    $this->actingAs($staff, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListFileReports::class)
        ->callTableAction('takedown', $report, ['reason' => 'Confirmed copyright infringement'])
        ->assertHasNoActionErrors();

    expect($transfer->refresh()->status)->toBe(TransferStatus::Deleting)
        ->and($report->refresh()->status->value)->toBe('resolved')
        ->and($report->resolution)->toBe('Confirmed copyright infringement');
    $this->assertDatabaseHas('admin_audits', ['action' => 'transfer.takedown_requested', 'target_id' => $transfer->id]);
    $this->assertDatabaseHas('admin_audits', ['action' => 'file_report.reviewed', 'target_id' => $report->id]);
});

test('upload and audit views exclude secrets', function () {
    $admin = resourceAdmin();
    $transfer = Transfer::factory()->create(['encrypted_manifest' => 'highly-secret-manifest']);
    $audit = AdminAudit::factory()->create(['changes' => ['upload_token_hash' => 'highly-secret-token']]);

    $this->actingAs($admin, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListTransfers::class)
        ->mountAction(TestAction::make('view')->table($transfer))
        ->assertDontSee('highly-secret-manifest');

    Livewire::test(ListAdminAudits::class)
        ->mountAction(TestAction::make('view')->table($audit))
        ->assertDontSee('highly-secret-token');
});

test('assignment only accepts active verified staff', function () {
    $staff = resourceStaff();
    $activeAssignee = resourceStaff();
    $unverifiedAssignee = User::factory()->unverified()->create(['role' => UserRole::Moderator]);
    $suspendedAssignee = User::factory()->create(['role' => UserRole::Moderator, 'suspended_at' => now()]);
    $report = FileReport::factory()->create();

    $this->actingAs($staff, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListFileReports::class)
        ->callTableAction('assign', $report, ['assigned_to' => $activeAssignee->id])
        ->assertHasNoActionErrors();

    expect($report->refresh()->assigned_to)->toBe($activeAssignee->id);

    Livewire::test(ListFileReports::class)
        ->callTableAction('assign', $report, ['assigned_to' => $unverifiedAssignee->id])
        ->assertHasActionErrors();

    Livewire::test(ListFileReports::class)
        ->callTableAction('assign', $report, ['assigned_to' => $suspendedAssignee->id])
        ->assertHasActionErrors();
});

test('non-staff cannot access moderation resource tables or actions', function () {
    $user = User::factory()->create(['role' => UserRole::User]);
    $transfer = Transfer::factory()->create();
    $report = FileReport::factory()->create();

    $this->actingAs($user, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    expect(TransferResource::canViewAny())->toBeFalse()
        ->and(FileReportResource::canViewAny())->toBeFalse();

    Livewire::test(ListTransfers::class)->assertForbidden();
    Livewire::test(ListFileReports::class)->assertForbidden();
});

test('moderators cannot invoke user management or audit history resources', function () {
    $audit = AdminAudit::factory()->create();
    $staff = resourceStaff();

    $this->actingAs($staff, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    expect(AdminAuditResource::canViewAny())->toBeFalse()
        ->and(UserResource::canViewAny())->toBeFalse();
    Livewire::test(ListAdminAudits::class)->assertForbidden();
    Livewire::test(ListUsers::class)->assertForbidden();

    $admin = resourceAdmin();
    $this->actingAs($admin, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListAdminAudits::class)
        ->assertCanSeeTableRecords([$audit])
        ->assertTableActionExists('view', null, $audit);
});
