<?php

declare(strict_types=1);

use App\Actions\Admin\ManageUser;
use App\Enums\UserRole;
use App\Filament\Resources\Users\UserResource;
use App\Models\AdminAudit;
use App\Models\Plan;
use App\Models\User;
use App\Models\UserInvitation;
use App\Notifications\UserInvitationNotification;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Auth\Notifications\VerifyEmail;
use Illuminate\Support\Facades\Notification;
use Illuminate\Validation\ValidationException;

function provisioningAdmin(): User
{
    return User::factory()->create(['role' => UserRole::Admin, 'suspended_at' => null]);
}

function defaultProvisioningPlan(): Plan
{
    return Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan'), 'is_active' => true]);
}

function provisionedUserData(array $overrides = []): array
{
    return [...[
        'username' => 'provisioned_user',
        'name' => 'Provisioned User',
        'email' => 'provisioned@example.test',
        'password' => 'Valid8!x',
        'password_confirmation' => 'Valid8!x',
    ], ...$overrides];
}

test('only administrators may provision or invite users', function (): void {
    $user = User::factory()->create(['role' => UserRole::User]);

    $this->actingAs($user);

    expect(UserResource::canCreate())->toBeFalse()
        ->and(fn () => app(ManageUser::class)->create($user, provisionedUserData()))->toThrow(AuthorizationException::class)
        ->and(fn () => app(ManageUser::class)->invite($user, ['email' => 'invitee@example.test']))->toThrow(AuthorizationException::class);
});

test('administrators can create unverified regular users when registration is disabled', function (): void {
    Notification::fake();
    config()->set('filebeam.features.registration', false);
    $plan = defaultProvisioningPlan();
    $admin = provisioningAdmin();

    $user = app(ManageUser::class)->create($admin, provisionedUserData());

    expect($user->role)->toBe(UserRole::User)
        ->and($user->email_verified_at)->toBeNull()
        ->and($user->plan_id)->toBe($plan->id);
    $this->assertDatabaseHas('admin_audits', ['action' => 'user.created', 'target_id' => (string) $user->id]);
    Notification::assertSentTo($user, VerifyEmail::class);
});

test('invitations are queued after commit, rotate on resend, and do not audit secrets', function (): void {
    Notification::fake();
    config()->set('filebeam.features.registration', false);
    $admin = provisioningAdmin();
    $manager = app(ManageUser::class);

    $first = $manager->invite($admin, ['email' => 'INVITEE@EXAMPLE.TEST']);
    $firstHash = $first->token_hash;
    $second = $manager->invite($admin, ['email' => 'invitee@example.test']);

    expect($second->id)->toBe($first->id)
        ->and($second->fresh()->token_hash)->not->toBe($firstHash)
        ->and(UserInvitation::query()->count())->toBe(1);
    Notification::assertSentTo($second, UserInvitationNotification::class, 2);
    Notification::assertSentTo($second, UserInvitationNotification::class, fn (UserInvitationNotification $notification): bool => $notification->afterCommit === true);

    $audits = AdminAudit::query()->whereIn('action', ['invitation.sent', 'invitation.resent'])->get();
    expect($audits)->toHaveCount(2)
        ->and($audits->pluck('changes')->flatten()->all())->not->toContain('password', 'token', $firstHash);
});

test('invitations reject email addresses that already belong to users', function (): void {
    $admin = provisioningAdmin();
    User::factory()->create(['email' => 'existing@example.test']);

    expect(fn () => app(ManageUser::class)->invite($admin, ['email' => 'EXISTING@EXAMPLE.TEST']))->toThrow(ValidationException::class);
});
