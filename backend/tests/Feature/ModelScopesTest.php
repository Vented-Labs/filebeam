<?php

declare(strict_types=1);

use App\Enums\ReportStatus;
use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Models\AccountKeyBundle;
use App\Models\FileReport;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\TransferKeyEnvelope;
use App\Models\User;

test('plan and account key scopes select active records only', function (): void {
    $activePlan = Plan::factory()->create(['slug' => 'active-plan', 'is_active' => true]);
    Plan::factory()->create(['slug' => 'inactive-plan', 'is_active' => false]);
    $activeBundle = AccountKeyBundle::factory()->create(['is_active' => true]);
    AccountKeyBundle::factory()->create(['is_active' => false]);

    expect(Plan::query()->default('active-plan')->active()->sole()->is($activePlan))->toBeTrue()
        ->and(AccountKeyBundle::query()->active()->pluck('id')->all())->toBe([$activeBundle->id]);
});

test('user scopes distinguish active staff from inbox-eligible recipients', function (): void {
    $staff = User::factory()->create(['role' => UserRole::Admin, 'email_verified_at' => now(), 'suspended_at' => null, 'inbox_enabled' => false]);
    $recipient = User::factory()->create(['role' => UserRole::User, 'suspended_at' => null, 'inbox_enabled' => true]);
    $unverifiedRecipient = User::factory()->create(['role' => UserRole::Moderator, 'email_verified_at' => null, 'suspended_at' => null, 'inbox_enabled' => true]);
    User::factory()->create(['role' => UserRole::Admin, 'email_verified_at' => now(), 'suspended_at' => now(), 'inbox_enabled' => true]);

    expect(User::query()->activeStaff()->pluck('id')->all())->toBe([$staff->id])
        ->and(User::query()->inboxEnabled()->pluck('id')->all())->toBe([$recipient->id, $unverifiedRecipient->id]);
});

test('transfer lifecycle scopes retain their status and expiry boundaries', function (): void {
    $pending = Transfer::factory()->create(['status' => TransferStatus::Pending]);
    $available = Transfer::factory()->create(['status' => TransferStatus::Available, 'expires_at' => now()->addMinute()]);
    $deleting = Transfer::factory()->create(['status' => TransferStatus::Deleting]);
    Transfer::factory()->create(['status' => TransferStatus::Available, 'expires_at' => now()->subSecond()]);

    expect(Transfer::query()->pending()->pluck('id')->all())->toBe([$pending->id])
        ->and(Transfer::query()->availableAndUnexpired()->pluck('id')->all())->toBe([$available->id])
        ->and(Transfer::query()->awaitingCleanup()->pluck('id')->all())->toBe([$deleting->id]);
});

test('report queue and recipient envelope scopes compose with caller constraints', function (): void {
    $assignee = User::factory()->create();
    $assignedActive = FileReport::factory()->create(['status' => ReportStatus::Open, 'assigned_to' => $assignee->id]);
    $unassignedActive = FileReport::factory()->create(['status' => ReportStatus::InReview, 'assigned_to' => null]);
    $closed = FileReport::factory()->create(['status' => ReportStatus::Resolved]);
    $recipient = TransferKeyEnvelope::factory()->create(['role' => 'recipient']);
    TransferKeyEnvelope::factory()->create(['role' => 'owner']);

    expect(FileReport::query()->active()->assignedTo($assignee->id)->pluck('id')->all())->toBe([$assignedActive->id])
        ->and(FileReport::query()->active()->unassigned()->pluck('id')->all())->toBe([$unassignedActive->id])
        ->and(FileReport::query()->closed()->pluck('id')->all())->toBe([$closed->id])
        ->and(TransferKeyEnvelope::query()->recipient()->pluck('id')->all())->toBe([$recipient->id]);
});
