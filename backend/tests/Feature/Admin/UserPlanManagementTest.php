<?php

declare(strict_types=1);

use App\Actions\Admin\ManagePlan;
use App\Actions\Admin\ManageUser;
use App\Enums\UserRole;
use App\Filament\Resources\Plans\Pages\EditPlan;
use App\Filament\Resources\Plans\Pages\ListPlans;
use App\Filament\Resources\Plans\PlanResource;
use App\Filament\Resources\Users\Pages\ListUsers;
use App\Filament\Resources\Users\UserResource;
use App\Models\AdminAudit;
use App\Models\Plan;
use App\Models\User;
use Filament\Facades\Filament;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Validation\ValidationException;
use Livewire\Livewire;

uses(RefreshDatabase::class);

function administrator(): User
{
    return User::factory()->create(['role' => UserRole::Admin, 'suspended_at' => null]);
}

function planAttributes(Plan $plan): array
{
    return [
        'maximum_transfer_bytes' => $plan->maximum_transfer_bytes,
        'maximum_file_count' => $plan->maximum_file_count,
        'maximum_note_bytes' => $plan->maximum_note_bytes,
        'default_file_retention_hours' => $plan->default_file_retention_hours,
        'maximum_file_retention_hours' => $plan->maximum_file_retention_hours,
        'default_note_retention_hours' => $plan->default_note_retention_hours,
        'maximum_note_retention_hours' => $plan->maximum_note_retention_hours,
        'is_active' => true,
    ];
}

test('only active administrators can access the management resources and service mutations', function () {
    $user = User::factory()->create(['role' => UserRole::User]);
    $target = User::factory()->create(['role' => UserRole::User]);
    $plan = Plan::factory()->create();

    $this->actingAs($user);

    expect(UserResource::canViewAny())->toBeFalse()
        ->and(PlanResource::canViewAny())->toBeFalse()
        ->and(fn () => app(ManageUser::class)->changePlan($user, $target, $plan, 'Support request'))
        ->toThrow(AuthorizationException::class)
        ->and(fn () => app(ManagePlan::class)->update($user, $plan, planAttributes($plan)))
        ->toThrow(AuthorizationException::class);
});

test('user role plan and suspension changes require a reason and are audited', function () {
    $actor = administrator();
    $targetPlan = Plan::factory()->create();
    $target = User::factory()->create(['role' => UserRole::User, 'plan_id' => null]);
    $manager = app(ManageUser::class);

    expect(fn () => $manager->changeRole($actor, $target, UserRole::Moderator, '   '))
        ->toThrow(ValidationException::class);
    expect(fn () => $manager->changeRole($actor, $target, UserRole::Moderator, str_repeat('a', 2001)))
        ->toThrow(ValidationException::class);

    $manager->changeRole($actor, $target, UserRole::Moderator, 'Escalated support duties');
    $manager->changePlan($actor, $target, $targetPlan, 'Customer upgrade');
    $manager->setSuspension($actor, $target, true, 'Abusive activity');

    $inactivePlan = Plan::factory()->create(['is_active' => false]);
    expect(fn () => $manager->changePlan($actor, $target, $inactivePlan, 'Invalid downgrade'))
        ->toThrow(ValidationException::class);

    $target->refresh();

    expect($target->role)->toBe(UserRole::Moderator)
        ->and($target->plan_id)->toBe($targetPlan->id)
        ->and($target->suspended_at)->not->toBeNull();

    $this->assertDatabaseHas('admin_audits', ['action' => 'user.role_changed', 'target_id' => $target->id, 'reason' => 'Escalated support duties']);
    $this->assertDatabaseHas('admin_audits', ['action' => 'user.plan_changed', 'target_id' => $target->id, 'reason' => 'Customer upgrade']);
    $this->assertDatabaseHas('admin_audits', ['action' => 'user.suspended', 'target_id' => $target->id, 'reason' => 'Abusive activity']);
});

test('the last verified unsuspended administrator cannot be demoted or suspended', function () {
    $admin = administrator();
    $manager = app(ManageUser::class);

    expect(fn () => $manager->changeRole($admin, $admin, UserRole::Moderator, 'Changing responsibilities'))
        ->toThrow(ValidationException::class)
        ->and(fn () => $manager->setSuspension($admin, $admin, true, 'Temporary leave'))
        ->toThrow(ValidationException::class);

    $admin->refresh();

    expect($admin->role)->toBe(UserRole::Admin)
        ->and($admin->suspended_at)->toBeNull();
});

test('plan updates validate retention limits, preserve the slug, and keep the configured default active', function () {
    $actor = administrator();
    $plan = Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan')]);
    $manager = app(ManagePlan::class);

    $invalidRetention = planAttributes($plan);
    $invalidRetention['default_file_retention_hours'] = $invalidRetention['maximum_file_retention_hours'] + 1;

    expect(fn () => $manager->update($actor, $plan, $invalidRetention))->toThrow(ValidationException::class);

    $invalidFileCount = planAttributes($plan);
    $invalidFileCount['maximum_file_count'] = 32768;

    expect(fn () => $manager->update($actor, $plan, $invalidFileCount))->toThrow(ValidationException::class);

    $disabledDefault = planAttributes($plan);
    $disabledDefault['is_active'] = false;

    expect(fn () => $manager->update($actor, $plan, $disabledDefault))->toThrow(ValidationException::class);

    $updated = planAttributes($plan);
    $updated['maximum_file_count'] = 50;
    $manager->update($actor, $plan, $updated);

    $plan->refresh();

    expect($plan->slug)->toBe(config('filebeam.transfers.default_plan'))
        ->and($plan->maximum_file_count)->toBe(50)
        ->and($plan->is_active)->toBeTrue()
        ->and(AdminAudit::query()->where('action', 'plan.updated')->where('target_id', $plan->id)->exists())->toBeTrue();
});

test('administrators invoke user and plan table actions through the admin guard', function () {
    $actor = administrator();
    $target = User::factory()->create(['role' => UserRole::User]);
    $plan = Plan::factory()->create();

    $this->actingAs($actor, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListUsers::class)
        ->assertCanSeeTableRecords([$target])
        ->mountTableAction('changeRole', $target)
        ->assertTableActionDataSet(['role' => UserRole::User->value])
        ->setTableActionData(['role' => UserRole::Moderator->value, 'reason' => 'Support escalation'])
        ->callMountedTableAction()
        ->assertHasNoActionErrors()
        ->callTableAction('changePlan', $target, ['plan_id' => $plan->id, 'reason' => 'Customer upgrade'])
        ->assertHasNoActionErrors()
        ->callTableAction('suspend', $target, ['reason' => 'Abuse investigation'])
        ->assertHasNoActionErrors();

    expect($target->refresh())
        ->role->toBe(UserRole::Moderator)
        ->plan_id->toBe($plan->id)
        ->suspended_at->not->toBeNull();

    Livewire::test(ListPlans::class)->assertCanSeeTableRecords([$plan]);

    Livewire::test(EditPlan::class, ['record' => $plan->getKey()])
        ->fillForm(fn (array $data): array => [...$data, 'maximum_file_count' => 40])
        ->call('save')
        ->assertHasNoFormErrors();

    expect($plan->refresh()->maximum_file_count)->toBe(40);
});

test('plan changes roll back if their audit cannot be recorded', function () {
    $actor = administrator();
    $plan = Plan::factory()->create(['maximum_file_count' => 20]);
    AdminAudit::creating(function (): never {
        throw new RuntimeException('Audit storage unavailable');
    });

    expect(fn () => app(ManagePlan::class)->update($actor, $plan, [...planAttributes($plan), 'maximum_file_count' => 50]))
        ->toThrow(RuntimeException::class);
    expect($plan->refresh()->maximum_file_count)->toBe(20);
});

test('table action forms validate their target values', function () {
    $actor = administrator();
    $target = User::factory()->create(['role' => UserRole::User]);
    $plan = Plan::factory()->create();

    $this->actingAs($actor, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    Livewire::test(ListUsers::class)
        ->callTableAction('changeRole', $target, ['role' => UserRole::Moderator->value, 'reason' => ''])
        ->assertHasActionErrors(['reason' => 'required']);

    Livewire::test(EditPlan::class, ['record' => $plan->getKey()])
        ->fillForm(fn (array $data): array => [
            ...$data,
            'default_note_retention_quantity' => 2,
            'default_note_retention_unit' => 'days',
            'maximum_note_retention_quantity' => 1,
            'maximum_note_retention_unit' => 'days',
        ])
        ->call('save')
        ->assertHasErrors(['data.default_note_retention_quantity']);
});

test('moderators cannot access user or plan resource tables or invoke their mutations', function () {
    $moderator = User::factory()->create(['role' => UserRole::Moderator]);
    $target = User::factory()->create();
    $plan = Plan::factory()->create();

    $this->actingAs($moderator, 'admin');
    Filament::setCurrentPanel(Filament::getPanel('admin'));

    expect(UserResource::canViewAny())->toBeFalse()
        ->and(PlanResource::canViewAny())->toBeFalse()
        ->and(fn () => app(ManageUser::class)->changePlan($moderator, $target, $plan, 'Unauthorized change'))
        ->toThrow(AuthorizationException::class)
        ->and(fn () => app(ManagePlan::class)->update($moderator, $plan, planAttributes($plan)))
        ->toThrow(AuthorizationException::class);

    Livewire::test(ListUsers::class)->assertForbidden();
    Livewire::test(ListPlans::class)->assertForbidden();
});
