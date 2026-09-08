<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Filament\Pages\StaffProfile;
use App\Models\Plan;
use App\Models\User;
use Filament\Facades\Filament;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Facades\Http;
use Illuminate\Support\Facades\Notification;
use Illuminate\Support\Facades\Password as PasswordBroker;
use Illuminate\Support\Facades\Validator;
use Illuminate\Validation\Rules\Password;
use Livewire\Livewire;

test('the global password minimum is eight in every environment', function (string $environment): void {
    $this->app->detectEnvironment(fn (): string => $environment);
    Http::fake(['*' => Http::response('', 200)]);

    expect(Validator::make(['password' => 'Valid8!x'], ['password' => Password::defaults()])->passes())->toBeTrue()
        ->and(Validator::make(['password' => 'Short7!'], ['password' => Password::defaults()])->passes())->toBeFalse();
})->with(['local', 'testing', 'production']);

test('registration accepts eight characters and rejects seven', function (): void {
    Notification::fake();
    Plan::factory()->create(['slug' => config('filebeam.transfers.default_plan')]);
    $data = ['username' => 'new_member', 'name' => 'New member', 'email' => 'member@example.test'];

    $this->post('/register', [...$data, 'password' => 'Short7!', 'password_confirmation' => 'Short7!'])
        ->assertSessionHasErrors('password');
    $this->post('/register', [...$data, 'password' => 'Valid8!x', 'password_confirmation' => 'Valid8!x'])
        ->assertSessionHasNoErrors()->assertRedirect(route('verification.notice'));
});

test('password resets accept eight characters and reject seven', function (): void {
    $user = User::factory()->create();
    $data = ['email' => $user->email, 'token' => PasswordBroker::createToken($user)];

    $this->post('/reset-password', [...$data, 'password' => 'Short7!', 'password_confirmation' => 'Short7!'])
        ->assertSessionHasErrors('password');
    $this->post('/reset-password', [...$data, 'password' => 'Valid8!x', 'password_confirmation' => 'Valid8!x'])
        ->assertSessionHasNoErrors()->assertRedirect(route('login'));
    expect(Hash::check('Valid8!x', $user->refresh()->password))->toBeTrue();
});

test('staff profile password changes use the same global minimum', function (): void {
    Filament::setCurrentPanel(Filament::getPanel('admin'));
    $user = User::factory()->create(['role' => UserRole::Admin]);
    $this->actingAs($user, 'admin');

    Livewire::test(StaffProfile::class)
        ->fillForm(['password' => 'Short7!', 'passwordConfirmation' => 'Short7!', 'currentPassword' => 'password'])
        ->call('save')
        ->assertHasFormErrors(['password']);

    Livewire::test(StaffProfile::class)
        ->fillForm(['password' => 'Valid8!x', 'passwordConfirmation' => 'Valid8!x', 'currentPassword' => 'password'])
        ->call('save')
        ->assertHasNoFormErrors();

    expect(Hash::check('Valid8!x', $user->refresh()->password))->toBeTrue();
});
