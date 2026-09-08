<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Filament\Resources\Plans\Pages\EditPlan;
use App\Filament\Resources\Plans\Pages\ViewPlan;
use App\Filament\Resources\Plans\PlanResource;
use App\Filament\Resources\Plans\RelationManagers\AssignedUsersRelationManager;
use App\Filament\Resources\Users\Pages\ViewUser;
use App\Filament\Resources\Users\RelationManagers\OwnedTransfersRelationManager;
use App\Filament\Resources\Users\UserResource;
use App\Models\Plan;
use App\Models\User;
use Filament\Facades\Filament;
use Livewire\Livewire;

function workspaceAdministrator(): User
{
    return User::factory()->create(['role' => UserRole::Admin, 'suspended_at' => null]);
}

function useAdminWorkspace(User $user): void
{
    test()->actingAs($user, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));
}

test('administrators can globally search users by email and username', function () {
    useAdminWorkspace(workspaceAdministrator());

    expect(UserResource::canGloballySearch())->toBeTrue()
        ->and(UserResource::getGloballySearchableAttributes())->toBe(['email', 'username']);
});

test('moderators cannot access account workspace records', function () {
    useAdminWorkspace(User::factory()->create(['role' => UserRole::Moderator]));
    $user = User::factory()->create();
    $plan = Plan::factory()->create();

    Livewire::test(ViewUser::class, ['record' => $user->getKey()])->assertForbidden();
    Livewire::test(ViewPlan::class, ['record' => $plan->getKey()])->assertForbidden();
});

test('user view contains account details but no password form field', function () {
    $administrator = workspaceAdministrator();
    $user = User::factory()->create(['role' => UserRole::User, 'email_verified_at' => null]);
    useAdminWorkspace($administrator);

    Livewire::test(ViewUser::class, ['record' => $user->getKey()])
        ->assertSchemaComponentExists('email')
        ->assertSchemaComponentExists('email_verified_at')
        ->assertSchemaComponentExists('owned_transfers_count')
        ->assertSchemaComponentDoesNotExist('password')
        ->assertSchemaComponentDoesNotExist('two_factor_secret');
});

test('user view exposes owned transfer relation only', function () {
    $administrator = workspaceAdministrator();
    $user = User::factory()->create();
    useAdminWorkspace($administrator);

    expect(UserResource::getRelations())->toContain(OwnedTransfersRelationManager::class)
        ->not->toContain('ReceivedTransfersRelationManager');
});

test('last administrator protection is enforced through the user view action', function () {
    $administrator = workspaceAdministrator();
    useAdminWorkspace($administrator);

    Livewire::test(ViewUser::class, ['record' => $administrator->getKey()])
        ->callAction('changeRole', ['role' => UserRole::Moderator->value, 'reason' => 'Role review']);

    expect($administrator->refresh()->role)->toBe(UserRole::Admin);
});

test('plan edit hydrates arbitrary byte and hour values without rounding', function () {
    $administrator = workspaceAdministrator();
    $plan = Plan::factory()->create([
        'maximum_transfer_bytes' => 1537,
        'maximum_note_bytes' => 1048576,
        'default_file_retention_hours' => 25,
        'maximum_file_retention_hours' => 48,
        'default_note_retention_hours' => 1,
        'maximum_note_retention_hours' => 72,
    ]);
    useAdminWorkspace($administrator);

    Livewire::test(EditPlan::class, ['record' => $plan->getKey()])
        ->assertSchemaStateSet([
            'maximum_transfer_quantity' => 1537,
            'maximum_transfer_unit' => 'B',
            'maximum_note_quantity' => 1,
            'maximum_note_unit' => 'MiB',
            'default_file_retention_quantity' => 25,
            'default_file_retention_unit' => 'hours',
            'maximum_file_retention_quantity' => 2,
            'maximum_file_retention_unit' => 'days',
        ]);
});

test('plan edit persists exact quantities in selected units through manage plan', function () {
    $administrator = workspaceAdministrator();
    $plan = Plan::factory()->create();
    useAdminWorkspace($administrator);

    Livewire::test(EditPlan::class, ['record' => $plan->getKey()])
        ->fillForm([
            'maximum_transfer_quantity' => 2, 'maximum_transfer_unit' => 'GiB',
            'maximum_file_count' => 10,
            'maximum_note_quantity' => 1537, 'maximum_note_unit' => 'B',
            'default_file_retention_quantity' => 25, 'default_file_retention_unit' => 'hours',
            'maximum_file_retention_quantity' => 2, 'maximum_file_retention_unit' => 'days',
            'default_note_retention_quantity' => 1, 'default_note_retention_unit' => 'days',
            'maximum_note_retention_quantity' => 2, 'maximum_note_retention_unit' => 'days',
            'is_active' => true,
        ])
        ->call('save')
        ->assertHasNoFormErrors();

    expect($plan->refresh())
        ->maximum_transfer_bytes->toBe(2 * 1024 * 1024 * 1024)
        ->maximum_note_bytes->toBe(1537)
        ->default_file_retention_hours->toBe(25)
        ->maximum_file_retention_hours->toBe(48);
});

test('plan edit rejects overflowing unit conversions before casting', function () {
    $administrator = workspaceAdministrator();
    $plan = Plan::factory()->create();
    useAdminWorkspace($administrator);

    Livewire::test(EditPlan::class, ['record' => $plan->getKey()])
        ->fillForm(fn (array $data): array => [
            ...$data,
            'maximum_transfer_quantity' => '8589934592',
            'maximum_transfer_unit' => 'GiB',
        ])
        ->call('save')
        ->assertHasErrors(['data.maximum_transfer_quantity']);
});

test('saving unchanged arbitrary plan limits preserves exact canonical values', function () {
    $administrator = workspaceAdministrator();
    $plan = Plan::factory()->create([
        'maximum_transfer_bytes' => 1537,
        'maximum_note_bytes' => 1537,
        'default_file_retention_hours' => 25,
        'maximum_file_retention_hours' => 49,
        'default_note_retention_hours' => 25,
        'maximum_note_retention_hours' => 49,
    ]);
    useAdminWorkspace($administrator);

    Livewire::test(EditPlan::class, ['record' => $plan->getKey()])
        ->call('save')
        ->assertHasNoFormErrors();

    expect($plan->refresh())
        ->maximum_transfer_bytes->toBe(1537)
        ->maximum_note_bytes->toBe(1537)
        ->default_file_retention_hours->toBe(25)
        ->maximum_file_retention_hours->toBe(49)
        ->default_note_retention_hours->toBe(25)
        ->maximum_note_retention_hours->toBe(49);
});

test('the configured default plan cannot be disabled from its edit page', function () {
    $administrator = workspaceAdministrator();
    $plan = Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan')]);
    useAdminWorkspace($administrator);

    Livewire::test(EditPlan::class, ['record' => $plan->getKey()])
        ->fillForm(fn (array $data): array => [...$data, 'is_active' => false])
        ->call('save');

    expect($plan->refresh()->is_active)->toBeTrue();
});

test('plan details include assigned users relation', function () {
    $administrator = workspaceAdministrator();
    $plan = Plan::factory()->create();
    User::factory()->create(['plan_id' => $plan->id]);
    useAdminWorkspace($administrator);

    Livewire::test(ViewPlan::class, ['record' => $plan->getKey()])
        ->assertSchemaComponentExists('maximum_transfer_bytes')
        ->assertSchemaComponentExists('default_file_retention_hours');

    expect(PlanResource::getRelations())->toContain(AssignedUsersRelationManager::class);
});
