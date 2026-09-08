<?php

declare(strict_types=1);

use App\Models\User;
use Illuminate\Auth\Notifications\ResetPassword;
use Illuminate\Support\Facades\Notification;
use Illuminate\Support\Facades\Password;

test('users can sign in with a case-insensitive username while email login remains supported', function (): void {
    $user = User::factory()->create([
        'email' => 'member@example.test',
        'username' => 'Member_Name',
        'normalized_username' => 'member_name',
    ]);

    $this->post('/login', ['email' => ' MEMBER_NAME ', 'password' => 'password'])
        ->assertRedirect(route('account'));
    $this->assertAuthenticatedAs($user);

    auth()->logout();

    $this->post('/login', ['email' => 'MEMBER@EXAMPLE.TEST', 'password' => 'password'])
        ->assertRedirect(route('account'));
    $this->assertAuthenticatedAs($user);
});

test('invalid and suspended username logins receive the same generic credential error', function (): void {
    User::factory()->create([
        'username' => 'suspended_member',
        'normalized_username' => 'suspended_member',
        'suspended_at' => now(),
    ]);

    $invalid = $this->from('/login')->post('/login', [
        'email' => 'unknown_member',
        'password' => 'password',
    ]);
    $suspended = $this->from('/login')->post('/login', [
        'email' => 'SUSPENDED_MEMBER',
        'password' => 'password',
    ]);

    $invalid->assertRedirect(route('login'))->assertSessionHasErrors(['email' => 'The provided credentials do not match our records.']);
    $suspended->assertRedirect(route('login'))->assertSessionHasErrors(['email' => 'The provided credentials do not match our records.']);
    $this->assertGuest();
});

test('username password reset links use the account email and reset with the username identifier', function (): void {
    Notification::fake();
    $user = User::factory()->create([
        'email' => 'recover@example.test',
        'username' => 'recover_member',
        'normalized_username' => 'recover_member',
    ]);

    $this->post('/forgot-password', ['email' => ' RECOVER_MEMBER '])
        ->assertRedirect(route('password.request'))
        ->assertSessionHas('status', 'If an account matches that address, a password reset link will be sent.');

    Notification::assertSentTo($user, ResetPassword::class, function (ResetPassword $notification) use ($user): bool {
        $this->post('/reset-password', [
            'token' => $notification->token,
            'email' => 'RECOVER_MEMBER',
            'password' => 'another-secure-password',
            'password_confirmation' => 'another-secure-password',
        ])->assertRedirect(route('login'))->assertSessionHas('status', 'Your password has been reset. Existing encrypted inbox keys still need their original password. If it is lost, a replacement key can receive new files but cannot recover old ones.');

        return Password::tokenExists($user, $notification->token) === false;
    });

    expect(auth()->validate(['email' => $user->email, 'password' => 'another-secure-password']))->toBeTrue();
});

test('unknown username password reset requests do not disclose account existence', function (): void {
    Notification::fake();
    $user = User::factory()->create();

    $known = $this->post('/forgot-password', ['email' => $user->email]);
    $unknown = $this->post('/forgot-password', ['email' => 'unknown_member']);

    $known->assertRedirect(route('password.request'))->assertSessionHas('status', 'If an account matches that address, a password reset link will be sent.');
    $unknown->assertRedirect(route('password.request'))->assertSessionHas('status', 'If an account matches that address, a password reset link will be sent.');
    Notification::assertSentTo($user, ResetPassword::class);
});
