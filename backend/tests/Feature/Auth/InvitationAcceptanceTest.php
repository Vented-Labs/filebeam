<?php

declare(strict_types=1);

use App\Models\Plan;
use App\Models\User;
use App\Models\UserInvitation;
use Illuminate\Support\Facades\Hash;
use Inertia\Testing\AssertableInertia as Assert;

function acceptancePlan(): Plan
{
    return Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan'), 'is_active' => true]);
}

function invitationWithToken(array $attributes = []): array
{
    $token = 'invite-token-'.fake()->uuid();
    $invitation = UserInvitation::factory()->create([
        'email' => 'invitee@example.test',
        'token_hash' => hash('sha256', $token),
        ...$attributes,
    ]);

    return [$invitation, $token];
}

function invitationData(array $overrides = []): array
{
    return [...[
        'username' => 'invited_user',
        'name' => 'Invited User',
        'email' => 'invitee@example.test',
        'password' => 'Valid8!x',
        'password_confirmation' => 'Valid8!x',
    ], ...$overrides];
}

test('a valid invitation creates a verified account while registration is disabled', function (): void {
    config()->set('filebeam.features.registration', false);
    $plan = acceptancePlan();
    [$invitation, $token] = invitationWithToken();

    $this->get("/invitations/{$token}")->assertOk()->assertInertia(fn (Assert $page) => $page
        ->component('auth/AcceptInvitation')
        ->where('email', $invitation->email));
    $response = $this->post("/invitations/{$token}", invitationData());

    $user = User::query()->where('email', $invitation->email)->firstOrFail();
    $response->assertRedirect(route('account'));
    $this->assertAuthenticatedAs($user);
    expect($user->hasVerifiedEmail())->toBeTrue()
        ->and($user->plan_id)->toBe($plan->id)
        ->and(Hash::check('Valid8!x', $user->password))->toBeTrue()
        ->and($invitation->fresh()->accepted_at)->not->toBeNull();
});

test('expired and replayed invitation tokens cannot be accepted', function (): void {
    acceptancePlan();
    [$expired, $expiredToken] = invitationWithToken(['expires_at' => now()->subSecond()]);

    $this->get("/invitations/{$expiredToken}")->assertNotFound();
    $this->post("/invitations/{$expiredToken}", invitationData())->assertSessionHasErrors('email');

    [$invitation, $token] = invitationWithToken(['email' => 'second-invitee@example.test']);
    $this->post("/invitations/{$token}", invitationData(['email' => $invitation->email]))->assertRedirect(route('account'));
    auth()->logout();
    $this->post("/invitations/{$token}", invitationData(['email' => $invitation->email, 'username' => 'another_user']))->assertSessionHasErrors('email');
    expect(User::query()->where('email', $invitation->email)->count())->toBe(1);
});

test('invitation acceptance is bound to its invited email address', function (): void {
    acceptancePlan();
    [, $token] = invitationWithToken();

    $this->post("/invitations/{$token}", invitationData(['email' => 'other@example.test']))->assertSessionHasErrors('email');

    expect(User::query()->where('email', 'invitee@example.test')->exists())->toBeFalse();
});

test('invitation acceptance rejects an email address that has since been registered', function (): void {
    acceptancePlan();
    [, $token] = invitationWithToken();
    User::factory()->create(['email' => 'invitee@example.test']);

    $this->post("/invitations/{$token}", invitationData())->assertSessionHasErrors('email');
});
