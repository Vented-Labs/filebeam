<?php

declare(strict_types=1);

use App\Http\Middleware\RequireInstallation;
use App\Models\InstanceSetting;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\User;
use Database\Seeders\FilestoreSeeder;
use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\Route;
use Illuminate\Testing\TestResponse;
use Symfony\Component\HttpFoundation\Cookie;

beforeEach(function (): void {
    $this->withoutMiddleware(RequireInstallation::class);
    config()->set('app.key', 'base64:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=');
    config()->set('session.driver', 'cookie');
    config()->set('session.cookie', 'transfer-creator-session');
    config()->set('filebeam.transport_policy.environment.enabled_drivers', ['http', 'webrtc']);
    config()->set('filebeam.transport_policy.environment.default_driver', 'http');
    app('session')->forgetDrivers();
    app()->forgetInstance('session.store');

    Route::middleware('web')->get('/__tests/creator-session/{user}', function (User $user) {
        Auth::login($user, remember: false);

        return response()->json(['id' => Auth::id()]);
    });
});

/** @return array<string, mixed> */
function creatorPayload(string $driver = 'http'): array
{
    return [
        'kind' => 'files',
        'driver' => $driver,
        'protocol_version' => 1,
        'chunk_bytes' => config('filebeam.transfers.chunk_bytes'),
        'items' => [['ciphertext_bytes' => 16, 'chunk_count' => 1]],
    ];
}

function initializeCreatorPlanStorage(): void
{
    test()->seed(FilestoreSeeder::class);
}

/** @return array<string, string> */
function persistedCreatorCookie(User $user): array
{
    app('session')->forgetDrivers();
    app()->forgetInstance('session.store');
    Auth::forgetGuards();
    $response = test()->get("/__tests/creator-session/{$user->id}")->assertOk()->assertJsonPath('id', $user->id);
    $cookies = collect($response->headers->getCookies())
        ->mapWithKeys(fn (Cookie $cookie): array => [$cookie->getName() => $cookie->getValue()])
        ->all();

    app('session')->flush();
    app('session')->forgetDrivers();
    // A fresh HTTP request must not reuse the guard's old session store from the test container.
    app()->forgetInstance('session.store');
    Auth::forgetGuards();

    return $cookies;
}

/** @param array<string, string> $cookies @param array<string, mixed> $payload */
function createWithCreatorCookie(array $cookies, array $payload, array $headers = []): TestResponse
{
    return test()->withUnencryptedCookies($cookies)->withCredentials()->postJson('/api/v1/transfers', $payload, $headers);
}

test('a fresh encrypted cookie session assigns HTTP transfer ownership and the user plan', function (): void {
    Plan::factory()->create(['slug' => 'default']);
    $plan = Plan::factory()->create(['maximum_transfer_bytes' => 16]);
    $user = User::factory()->create(['plan_id' => $plan->id]);
    initializeCreatorPlanStorage();

    createWithCreatorCookie(persistedCreatorCookie($user), creatorPayload(), ['Sec-Fetch-Site' => 'same-origin'])->assertCreated();

    $transfer = Transfer::query()->sole();
    expect($transfer->owner_id)->toBe($user->id)->and($transfer->plan_id)->toBe($plan->id);
});

test('a fresh encrypted cookie session assigns WebRTC transfer ownership and the user plan', function (): void {
    Plan::factory()->create(['slug' => 'default']);
    $plan = Plan::factory()->create(['webrtc_maximum_transfer_bytes' => 16]);
    $user = User::factory()->create(['plan_id' => $plan->id]);
    initializeCreatorPlanStorage();

    createWithCreatorCookie(persistedCreatorCookie($user), creatorPayload('webrtc'), ['Sec-Fetch-Site' => 'same-origin'])->assertCreated();

    $transfer = Transfer::query()->sole();
    expect($transfer->owner_id)->toBe($user->id)->and($transfer->plan_id)->toBe($plan->id)->and($transfer->driver->value)->toBe('webrtc');
});

test('a signed-in creator can use a larger assigned plan when anonymous uploads are disabled', function (string $driver): void {
    Plan::factory()->create(['slug' => 'default', 'maximum_transfer_bytes' => 15, 'webrtc_maximum_transfer_bytes' => 15]);
    $plan = Plan::factory()->create(['maximum_transfer_bytes' => 16, 'webrtc_maximum_transfer_bytes' => 16]);
    $user = User::factory()->create(['plan_id' => $plan->id]);
    initializeCreatorPlanStorage();
    InstanceSetting::query()->create(['key' => 'anonymous_uploads', 'value' => false]);

    createWithCreatorCookie(persistedCreatorCookie($user), creatorPayload($driver), ['Sec-Fetch-Site' => 'same-origin'])->assertCreated();
    expect(Transfer::query()->sole()->owner_id)->toBe($user->id);
})->with(['http', 'webrtc']);

test('the assigned HTTP plan limit differs from the anonymous default plan', function (): void {
    Plan::factory()->create(['slug' => 'default', 'maximum_transfer_bytes' => 16]);
    $plan = Plan::factory()->create(['maximum_transfer_bytes' => 15]);
    $user = User::factory()->create(['plan_id' => $plan->id]);
    initializeCreatorPlanStorage();

    createWithCreatorCookie(persistedCreatorCookie($user), creatorPayload(), ['Sec-Fetch-Site' => 'same-origin'])->assertUnprocessable()->assertJsonValidationErrors('items');
    $this->unencryptedCookies = [];
    $this->defaultCookies = [];
    $this->withCredentials = false;
    app('session')->forgetDrivers();
    app()->forgetInstance('session.store');
    Auth::forgetGuards();
    Auth::clearResolvedInstance('auth');
    $this->postJson('/api/v1/transfers', creatorPayload())->assertCreated();
    expect(Transfer::query()->sole()->owner_id)->toBeNull();
});

test('the assigned WebRTC plan limit differs from the anonymous default plan', function (): void {
    Plan::factory()->create(['slug' => 'default', 'webrtc_maximum_transfer_bytes' => 16]);
    $plan = Plan::factory()->create(['webrtc_maximum_transfer_bytes' => 15]);
    $user = User::factory()->create(['plan_id' => $plan->id]);
    initializeCreatorPlanStorage();

    createWithCreatorCookie(persistedCreatorCookie($user), creatorPayload('webrtc'), ['Sec-Fetch-Site' => 'same-origin'])->assertUnprocessable()->assertJsonValidationErrors('items');
    $this->unencryptedCookies = [];
    $this->defaultCookies = [];
    $this->withCredentials = false;
    app('session')->forgetDrivers();
    app()->forgetInstance('session.store');
    Auth::forgetGuards();
    Auth::clearResolvedInstance('auth');
    $this->postJson('/api/v1/transfers', creatorPayload('webrtc'))->assertCreated();
    expect(Transfer::query()->sole()->owner_id)->toBeNull();
});

test('a cookie-authenticated creation requires same-origin CSRF protection outside testing mode', function (): void {
    app()->detectEnvironment(static fn (): string => 'local');
    Plan::factory()->create(['slug' => 'default']);
    $user = User::factory()->create();
    initializeCreatorPlanStorage();
    $cookies = persistedCreatorCookie($user);

    createWithCreatorCookie($cookies, creatorPayload())->assertStatus(419);
    createWithCreatorCookie($cookies, creatorPayload(), ['Sec-Fetch-Site' => 'same-origin'])->assertCreated();
});

test('suspended cookie sessions are denied while anonymous creation follows the upload setting', function (): void {
    Plan::factory()->create(['slug' => 'default']);
    $user = User::factory()->create(['suspended_at' => now()]);
    initializeCreatorPlanStorage();
    createWithCreatorCookie(persistedCreatorCookie($user), creatorPayload(), ['Sec-Fetch-Site' => 'same-origin'])->assertForbidden();

    InstanceSetting::query()->create(['key' => 'anonymous_uploads', 'value' => false]);
    $this->unencryptedCookies = [];
    $this->defaultCookies = [];
    $this->withCredentials = false;
    app('session')->forgetDrivers();
    app()->forgetInstance('session.store');
    Auth::forgetGuards();
    Auth::clearResolvedInstance('auth');
    $this->postJson('/api/v1/transfers', creatorPayload())->assertForbidden();
});
