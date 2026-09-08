<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Models\AdminAudit;
use App\Models\User;
use Illuminate\Support\Facades\Hash;

test('it creates a verified administrator with normalized account details and no credential audit data', function () {
    $password = 'Valid8!x';

    $this->artisan('filebeam:make-admin')
        ->expectsQuestion('Email', 'ADMIN@EXAMPLE.TEST ')
        ->expectsQuestion('Name', '  Admin User  ')
        ->expectsQuestion('Username', ' Admin_User ')
        ->expectsQuestion('Password', $password)
        ->expectsQuestion('Confirm password', $password)
        ->expectsConfirmation('I independently verified ownership of this email address.', 'yes')
        ->expectsQuestion('Audit reason', 'Initial administrator')
        ->expectsConfirmation('Create this verified administrator account?', 'yes')
        ->assertSuccessful();

    $user = User::query()->sole();
    $audit = AdminAudit::query()->sole();

    expect($user->email)->toBe('admin@example.test')
        ->and($user->name)->toBe('Admin User')
        ->and($user->username)->toBe('admin_user')
        ->and($user->normalized_username)->toBe('admin_user')
        ->and($user->email_verified_at)->not->toBeNull()
        ->and($user->role)->toBe(UserRole::Admin)
        ->and(Hash::check($password, $user->password))->toBeTrue()
        ->and($audit->action)->toBe('user.admin_created_via_cli')
        ->and(json_encode($audit->changes))->not->toContain($password)
        ->and(json_encode($audit->changes))->not->toContain('password');
});

test('it promotes an existing verified unsuspended user without changing account fields', function () {
    $user = User::factory()->create([
        'role' => UserRole::Moderator,
        'password' => 'original password hash remains unchanged',
    ]);
    $password = $user->password;
    $suspendedAt = $user->suspended_at;

    $this->artisan('filebeam:make-admin')
        ->expectsQuestion('Email', $user->email)
        ->expectsConfirmation('Promote this verified account to administrator?', 'yes')
        ->expectsQuestion('Audit reason', 'Expanded operations responsibilities')
        ->assertSuccessful();

    expect($user->refresh()->role)->toBe(UserRole::Admin)
        ->and($user->password)->toBe($password)
        ->and($user->suspended_at)->toEqual($suspendedAt)
        ->and(AdminAudit::query()->sole()->action)->toBe('user.admin_granted_via_cli');
});

test('it refuses unverified and suspended existing users', function () {
    $unverified = User::factory()->unverified()->create();
    $suspended = User::factory()->create(['suspended_at' => now()]);

    $this->artisan('filebeam:make-admin')
        ->expectsQuestion('Email', $unverified->email)
        ->expectsOutputToContain('not verified')
        ->assertFailed();
    $this->artisan('filebeam:make-admin')
        ->expectsQuestion('Email', $suspended->email)
        ->expectsOutputToContain('suspended')
        ->assertFailed();

    expect($unverified->refresh()->role)->toBe(UserRole::User)
        ->and($suspended->refresh()->role)->toBe(UserRole::User)
        ->and(AdminAudit::query()->doesntExist())->toBeTrue();
});

test('it makes no changes when either confirmation is declined', function () {
    $existing = User::factory()->create();

    $this->artisan('filebeam:make-admin')
        ->expectsQuestion('Email', $existing->email)
        ->expectsConfirmation('Promote this verified account to administrator?', 'no')
        ->assertSuccessful();
    $this->artisan('filebeam:make-admin')
        ->expectsQuestion('Email', 'new-admin@example.test')
        ->expectsQuestion('Name', 'New Admin')
        ->expectsQuestion('Username', 'new_admin')
        ->expectsQuestion('Password', 'Valid8!x')
        ->expectsQuestion('Confirm password', 'Valid8!x')
        ->expectsConfirmation('I independently verified ownership of this email address.', 'no')
        ->assertSuccessful();

    expect($existing->refresh()->role)->toBe(UserRole::User)
        ->and(User::query()->count())->toBe(1)
        ->and(AdminAudit::query()->doesntExist())->toBeTrue();
});

test('it retries invalid email username and password input', function () {
    $password = 'Valid8!x';

    $this->artisan('filebeam:make-admin')
        ->expectsQuestion('Email', 'not an email')
        ->expectsQuestion('Email', 'new-admin@example.test')
        ->expectsQuestion('Name', 'New Admin')
        ->expectsQuestion('Username', 'invalid username')
        ->expectsQuestion('Password', 'short')
        ->expectsQuestion('Confirm password', 'short')
        ->expectsQuestion('Name', 'New Admin')
        ->expectsQuestion('Username', 'new_admin')
        ->expectsQuestion('Password', $password)
        ->expectsQuestion('Confirm password', $password)
        ->expectsConfirmation('I independently verified ownership of this email address.', 'yes')
        ->expectsQuestion('Audit reason', '')
        ->expectsQuestion('Audit reason', 'Initial administrator')
        ->expectsConfirmation('Create this verified administrator account?', 'yes')
        ->assertSuccessful();

    expect(User::query()->sole()->username)->toBe('new_admin');
});

test('it refuses non-interactive execution', function () {
    $this->artisan('filebeam:make-admin', ['--no-interaction' => true])
        ->expectsOutputToContain('requires an interactive terminal')
        ->assertFailed();

    expect(User::query()->doesntExist())->toBeTrue()
        ->and(AdminAudit::query()->doesntExist())->toBeTrue();
});
