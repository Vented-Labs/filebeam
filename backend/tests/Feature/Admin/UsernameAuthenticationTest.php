<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Filament\Pages\AdminLogin;
use App\Models\User;
use Filament\Facades\Filament;
use Illuminate\Support\Facades\Auth;
use Livewire\Livewire;

beforeEach(function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
});

test('admin login accepts usernames, still challenges MFA, and rejects non-staff accounts', function (): void {
    $admin = User::factory()->create([
        'role' => UserRole::Admin,
        'username' => 'admin_member',
        'normalized_username' => 'admin_member',
    ]);
    $admin->saveAppAuthenticationSecret('JBSWY3DPEHPK3PXP');

    Livewire::test(AdminLogin::class)
        ->fillForm(['email' => 'ADMIN_MEMBER', 'password' => 'password'])
        ->call('authenticate')
        ->assertHasNoFormErrors();

    expect(Auth::guard('admin')->check())->toBeFalse();

    $member = User::factory()->create([
        'username' => 'ordinary_member',
        'normalized_username' => 'ordinary_member',
    ]);

    Livewire::test(AdminLogin::class)
        ->fillForm(['email' => $member->username, 'password' => 'password'])
        ->call('authenticate')
        ->assertHasFormErrors(['email']);

    expect(Auth::guard('admin')->check())->toBeFalse();
});
