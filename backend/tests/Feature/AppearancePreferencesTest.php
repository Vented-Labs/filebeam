<?php

declare(strict_types=1);

use App\Enums\UserRole;
use App\Models\User;
use App\Support\FeatureAvailability;
use App\Support\Theming\Appearance;
use App\Support\Theming\Palette;
use Illuminate\Support\Fluent;
use Inertia\Testing\AssertableInertia as Assert;

beforeEach(function () {
    config()->set('app.key', 'base64:'.base64_encode(str_repeat('a', 32)));
});

test('every user role can save its own Fluent appearance while retaining other settings', function (UserRole $role) {
    $user = User::factory()->create(['role' => $role, 'settings' => ['notifications' => ['weekly' => true]]]);
    $this->actingAs($user)->patchJson('/account/appearance', ['account' => $user->id, 'mode' => 'dark', 'preset' => 'teal'])
        ->assertOk()->assertJsonPath('preference.preset', 'teal');
    $settings = $user->fresh()->settings;
    expect($settings)->toBeInstanceOf(Fluent::class)
        ->and($settings->get('appearance'))->toBe(['mode' => 'dark', 'preset' => 'teal'])
        ->and($settings->get('notifications'))->toBe(['weekly' => true])
        ->and($user->fresh()->toArray())->not->toHaveKey('settings');
})->with(UserRole::cases());

test('guest adoption is atomic and never overwrites an initialized account', function () {
    $user = User::factory()->create();
    expect($user->settings)->toBeNull();
    $this->actingAs($user)->patchJson('/account/appearance', ['account' => $user->id, 'mode' => 'light', 'preset' => 'blue', 'adopt' => true])->assertOk();
    $this->patchJson('/account/appearance', ['account' => $user->id, 'mode' => 'dark', 'preset' => 'rose', 'adopt' => true])
        ->assertOk()->assertJsonPath('preference', ['mode' => 'light', 'preset' => 'blue']);
    $this->patchJson('/account/appearance', ['account' => $user->id, 'mode' => 'system', 'preset' => 'instance'])->assertOk();
    $this->patchJson('/account/appearance', ['account' => $user->id, 'mode' => 'dark', 'preset' => 'rose', 'adopt' => true])
        ->assertOk()->assertJsonPath('preference', ['mode' => 'system', 'preset' => 'instance']);
});

test('guest cookie preferences are resolved without creating user settings', function () {
    app()->instance(FeatureAvailability::class, new FeatureAvailability(['gd']));
    $this->withUnencryptedCookie(Appearance::COOKIE, json_encode(['mode' => 'light', 'preset' => 'blue']))
        ->get('/')->assertOk()->assertInertia(fn (Assert $page) => $page
        ->where('appearance.account', null)
        ->where('appearance.preference', ['mode' => 'light', 'preset' => 'blue'])
        ->where('appearance.needs_adoption', false));
});

test('account settings take precedence over guest cookies and instance social artwork', function () {
    app()->instance(FeatureAvailability::class, new FeatureAvailability(['gd']));
    $user = User::factory()->create(['settings' => ['appearance' => ['mode' => 'dark', 'preset' => 'green']]]);
    $this->actingAs($user)->withUnencryptedCookie(Appearance::COOKIE, json_encode(['mode' => 'light', 'preset' => 'blue']))
        ->get('/')->assertOk()->assertInertia(fn (Assert $page) => $page
        ->where('appearance.account', $user->id)
        ->where('appearance.preference', ['mode' => 'dark', 'preset' => 'green'])
        ->where('appearance.needs_adoption', false)
        ->where('theme.primary', Palette::DEFAULT_PRIMARY));
});

test('a newly signed-in account offers its guest choices for adoption', function () {
    $user = User::factory()->create();
    $this->actingAs($user)->withUnencryptedCookie(Appearance::COOKIE, json_encode(['mode' => 'light', 'preset' => 'rose']))
        ->get('/')->assertOk()->assertInertia(fn (Assert $page) => $page
        ->where('appearance.needs_adoption', true)
        ->where('appearance.preference', ['mode' => 'light', 'preset' => 'rose']));
    expect($user->fresh()->settings)->toBeNull();
});

test('admin sessions can save preferences but the public account wins when both guards are signed in', function () {
    $admin = User::factory()->create(['role' => UserRole::Admin]);
    $public = User::factory()->create();
    $this->actingAs($admin, 'admin')->patchJson('/account/appearance', ['account' => $admin->id, 'mode' => 'light', 'preset' => 'blue'])->assertOk();
    $this->actingAs($public, 'web')->patchJson('/account/appearance', ['account' => $admin->id, 'mode' => 'dark', 'preset' => 'rose'])->assertConflict();
    expect($admin->fresh()->settings->get('appearance.preset'))->toBe('blue')
        ->and($public->fresh()->settings)->toBeNull();
});

test('appearance updates reject guests suspended users other accounts and invalid choices', function () {
    $user = User::factory()->create();
    $other = User::factory()->create();
    $this->patchJson('/account/appearance', ['account' => $user->id, 'mode' => 'light', 'preset' => 'blue'])->assertUnauthorized();
    $this->actingAs($user)->patchJson('/account/appearance', ['account' => $other->id, 'mode' => 'light', 'preset' => 'blue'])->assertConflict();
    $this->patchJson('/account/appearance', ['account' => $user->id, 'mode' => 'invalid', 'preset' => '#ff00ff'])->assertUnprocessable();
    expect($other->fresh()->settings)->toBeNull();
    $user->forceFill(['suspended_at' => now()])->save();
    $this->patchJson('/account/appearance', ['account' => $user->id, 'mode' => 'light', 'preset' => 'blue'])->assertForbidden();
});

test('palette styles are versioned and independent of authentication', function () {
    $url = route('theme.stylesheet', ['version' => Appearance::revision()]);
    $response = $this->get($url)->assertOk()->assertHeader('content-type', 'text/css; charset=UTF-8');
    expect($response->getContent())->toContain('data-fb-preset="blue"')->toContain('data-fb-preset="rose"')
        ->and($response->headers->getCookies())->toBe([]);
    $this->withHeader('If-None-Match', $response->headers->get('etag'))->get($url)->assertNotModified();
});
