<?php

declare(strict_types=1);

use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\User;
use Illuminate\Auth\Notifications\ResetPassword;
use Illuminate\Auth\Notifications\VerifyEmail;
use Illuminate\Support\Facades\Notification;
use Illuminate\Support\Facades\Password;
use Illuminate\Support\Facades\URL;
use Inertia\Testing\AssertableInertia as Assert;

function freePlan(): Plan
{
    return Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan')]);
}

test('auth pages receive their route props during the normal application boot', function (): void {
    $this->get('/login')->assertInertia(fn (Assert $page) => $page
        ->component('auth/AuthScreen')
        ->where('mode', 'login')
        ->has('githubUrl')
        ->has('copyrightHolder')
        ->has('registrationEnabled'));

    $this->get('/register')->assertInertia(fn (Assert $page) => $page
        ->component('auth/AuthScreen')
        ->where('mode', 'register'));

    $user = User::factory()->create();

    $this->actingAs($user)->get('/account')->assertInertia(fn (Assert $page) => $page
        ->component('Account')
        ->where('user.email', $user->email)
        ->where('user.name', $user->name)
        ->has('user.profileUrl'));
});

test('a visitor can register and receives a verification notification', function (): void {
    Notification::fake();
    $plan = freePlan();

    $response = $this->post('/register', [
        'username' => 'qa_user',
        'name' => '',
        'email' => 'QA@EXAMPLE.TEST',
        'password' => 'a-long-secure-password',
        'password_confirmation' => 'a-long-secure-password',
    ]);

    $user = User::query()->where('email', 'qa@example.test')->firstOrFail();

    $response->assertRedirect(route('verification.notice'));
    $this->assertAuthenticatedAs($user);
    expect($user->username)->toBe('qa_user')
        ->and($user->normalized_username)->toBe('qa_user')
        ->and($user->name)->toBe('qa_user')
        ->and($user->plan_id)->toBe($plan->id)
        ->and($user->inbox_enabled)->toBeFalse();
    Notification::assertSentTo($user, VerifyEmail::class);
});

test('registration rejects duplicate and reserved usernames', function (): void {
    freePlan();
    User::factory()->create(['normalized_username' => 'existing', 'username' => 'existing']);

    $this->from('/register')->post('/register', [
        'username' => 'EXISTING',
        'email' => 'new@example.test',
        'password' => 'a-long-secure-password',
        'password_confirmation' => 'a-long-secure-password',
    ])->assertRedirect('/register')->assertSessionHasErrors('username');

    $this->from('/register')->post('/register', [
        'username' => 'login',
        'email' => 'another@example.test',
        'password' => 'a-long-secure-password',
        'password_confirmation' => 'a-long-secure-password',
    ])->assertRedirect('/register')->assertSessionHasErrors('username');
});

test('registration is unavailable when disabled', function (): void {
    config()->set('filebeam.features.registration', false);

    $this->get('/register')->assertNotFound();
    $this->post('/register', [])->assertNotFound();
});

test('registration follows the database instance setting', function (): void {
    InstanceSetting::query()->create(['key' => 'registration', 'value' => false]);

    $this->get('/login')->assertInertia(fn (Assert $page) => $page
        ->where('registrationEnabled', false));
    $this->get('/register')->assertNotFound();
    $this->post('/register', [])->assertNotFound();
});

test('users can sign in and invalid credentials are rejected', function (): void {
    $user = User::factory()->create(['email' => 'member@example.test']);

    $this->from('/login')->post('/login', [
        'email' => 'MEMBER@EXAMPLE.TEST',
        'password' => 'password',
    ])->assertRedirect(route('account'));
    $this->assertAuthenticatedAs($user);

    auth()->logout();

    $this->from('/login')->post('/login', [
        'email' => 'member@example.test',
        'password' => 'not-the-password',
    ])->assertRedirect('/login')->assertSessionHasErrors('email');
    $this->assertGuest();
});

test('login attempts are rate limited by normalized email and IP address', function (): void {
    User::factory()->create(['email' => 'limited@example.test']);

    foreach (range(1, 5) as $_) {
        $this->from('/login')->post('/login', [
            'email' => 'LIMITED@EXAMPLE.TEST',
            'password' => 'not-the-password',
        ])->assertSessionHasErrors('email');
    }

    $this->from('/login')->post('/login', [
        'email' => 'limited@example.test',
        'password' => 'not-the-password',
    ])->assertSessionHasErrors('email');
});

test('a valid password reset token resets the password', function (): void {
    Notification::fake();
    $user = User::factory()->create(['email' => 'recover@example.test']);

    Password::sendResetLink(['email' => $user->email]);

    Notification::assertSentTo($user, ResetPassword::class, function (ResetPassword $notification): bool {
        $this->post('/reset-password', [
            'token' => $notification->token,
            'email' => 'RECOVER@EXAMPLE.TEST',
            'password' => 'another-secure-password',
            'password_confirmation' => 'another-secure-password',
        ])->assertRedirect(route('login'));

        return true;
    });

    expect(auth()->validate(['email' => $user->email, 'password' => 'another-secure-password']))->toBeTrue();
});

test('a signed verification link verifies the authenticated user', function (): void {
    $user = User::factory()->unverified()->create();
    $url = URL::temporarySignedRoute('verification.verify', now()->addMinutes(10), [
        'id' => $user->id,
        'hash' => sha1($user->email),
    ]);

    $this->actingAs($user)->get($url)->assertRedirect(route('account'));

    expect($user->fresh()->hasVerifiedEmail())->toBeTrue();
});

test('logging out invalidates the authenticated session', function (): void {
    $user = User::factory()->create();

    $this->actingAs($user)->post('/logout')->assertRedirect(route('home'));

    $this->assertGuest();
});
