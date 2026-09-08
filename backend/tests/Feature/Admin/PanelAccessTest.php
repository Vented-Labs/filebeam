<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Models\User;
use Filament\Auth\Pages\Login;
use Filament\Facades\Filament;
use Illuminate\Support\Facades\Auth;
use Livewire\Livewire;

beforeEach(function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
});

test('admin login is public but the panel requires its own staff session', function () {
    $this->get('/admin/login')->assertOk();
    $this->get('/admin')->assertRedirect('/admin/login');

    $admin = User::factory()->create(['role' => UserRole::Admin]);
    $this->actingAs($admin, 'web')->get('/admin')->assertRedirect('/admin/login');
});

test('ordinary unverified and suspended accounts cannot enter the panel', function () {
    foreach ([
        User::factory()->create(),
        User::factory()->unverified()->create(['role' => UserRole::Admin]),
        User::factory()->create(['role' => UserRole::Moderator, 'suspended_at' => now()]),
    ] as $user) {
        $this->actingAs($user, 'admin')->get('/admin')->assertForbidden();
    }
});

test('admins can sign in and access resources without enrolling MFA', function () {
    $admin = User::factory()->create(['role' => UserRole::Admin]);

    Livewire::test(Login::class)
        ->fillForm(['email' => $admin->email, 'password' => 'password'])
        ->call('authenticate')
        ->assertHasNoFormErrors();

    $this->assertAuthenticatedAs($admin, 'admin');
    $this->get('/admin')->assertOk();
    $this->get('/admin/transfers')->assertOk();
});

test('panel login challenges enrolled staff before authenticating', function () {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    $admin->saveAppAuthenticationSecret('JBSWY3DPEHPK3PXP');

    Livewire::test(Login::class)
        ->fillForm(['email' => $admin->email, 'password' => 'password'])
        ->call('authenticate')
        ->assertHasNoFormErrors();

    expect(Auth::guard('admin')->check())->toBeFalse();
});

test('suspended accounts cannot login or use existing web sessions', function () {
    $user = User::factory()->create(['suspended_at' => now()]);

    $this->post('/login', ['email' => $user->email, 'password' => 'password'])
        ->assertSessionHasErrors('email');
    $this->assertGuest('web');
    $this->actingAs($user, 'web')->get('/account')->assertForbidden();
    $this->postJson('/api/v1/transfers', [])->assertForbidden();
    $this->post('/logout')->assertRedirect();
    $this->assertGuest('web');
});

test('staff security fields cannot be mass assigned and MFA material is hidden and encrypted', function () {
    $user = User::factory()->create();
    $user->fill(['role' => UserRole::Admin, 'suspended_at' => now(), 'app_authentication_secret' => 'injected']);

    expect($user->role)->toBe(UserRole::User)
        ->and($user->suspended_at)->toBeNull()
        ->and($user->getAppAuthenticationSecret())->toBeNull();

    $user->saveAppAuthenticationSecret('JBSWY3DPEHPK3PXP');
    $user->saveAppAuthenticationRecoveryCodes(['recovery-test']);
    expect($user->getRawOriginal('app_authentication_secret'))->not->toContain('JBSWY3DPEHPK3PXP')
        ->and($user->toArray())->not->toHaveKeys(['app_authentication_secret', 'app_authentication_recovery_codes']);
});

test('Filament CSP allowances are restricted to admin documents', function () {
    $this->app->detectEnvironment(fn (): string => 'production');

    $adminPolicy = $this->get('/admin/login')->assertOk()->headers->get('Content-Security-Policy');
    $publicPolicy = $this->get('/reports/create')->assertOk()->headers->get('Content-Security-Policy');

    expect($adminPolicy)->toContain("script-src 'self' 'unsafe-inline' 'unsafe-eval'")
        ->and($publicPolicy)->toContain("script-src 'self' 'nonce-")
        ->not->toContain("'unsafe-eval'");
});
