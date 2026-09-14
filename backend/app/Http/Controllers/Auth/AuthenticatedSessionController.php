<?php

declare(strict_types=1);

namespace App\Http\Controllers\Auth;

use App\Http\Controllers\Controller;
use App\Http\Requests\Auth\LoginRequest;
use App\Support\AuthIdentifier;
use App\Support\Branding;
use App\Support\InstanceSettings;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\RedirectResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Auth;
use Illuminate\Support\Facades\RateLimiter;
use Illuminate\Validation\ValidationException;
use Inertia\Inertia;
use Inertia\Response;

class AuthenticatedSessionController extends Controller
{
    public function create(): Response
    {
        $branding = app(Branding::class)->resolve();

        return Inertia::render('auth/AuthScreen', [
            'mode' => 'login',
            'githubUrl' => $branding['github_url'],
            'copyrightHolder' => $branding['copyright_holder'],
            'registrationEnabled' => app(InstanceSettings::class)->boolean('registration'),
        ]);
    }

    public function store(LoginRequest $request): RedirectResponse|JsonResponse
    {
        $credentials = $request->validated();
        $key = 'login:'.sha1($credentials['email'].'|'.$request->ip());
        $ipKey = 'login-ip:'.sha1((string) $request->ip());

        $rateLimitKey = RateLimiter::tooManyAttempts($key, 5)
            ? $key
            : (RateLimiter::tooManyAttempts($ipKey, 5) ? $ipKey : null);

        if ($rateLimitKey !== null) {
            throw ValidationException::withMessages([
                'email' => 'Too many login attempts. Please try again in '.RateLimiter::availableIn($rateLimitKey).' seconds.',
            ])->redirectTo(route('login'));
        }

        if (! Auth::attempt([...AuthIdentifier::credentials($credentials['email']), 'password' => $credentials['password'], 'suspended_at' => null], $request->boolean('remember'))) {
            RateLimiter::hit($key, 60);
            RateLimiter::hit($ipKey, 60);

            throw ValidationException::withMessages(['email' => 'The provided credentials do not match our records.'])->redirectTo(route('login'));
        }

        RateLimiter::clear($key);
        RateLimiter::clear($ipKey);
        $request->session()->regenerate();

        if ($request->expectsJson()) {
            return response()->json(['data' => $this->session($request)]);
        }

        return to_route('account');
    }

    public function destroy(Request $request): RedirectResponse|JsonResponse
    {
        Auth::logout();
        $request->session()->invalidate();
        $request->session()->regenerateToken();

        if ($request->expectsJson()) {
            return response()->json(status: 204);
        }

        return to_route('home');
    }

    /** @return array<string, bool|int|string|null> */
    private function session(Request $request): array
    {
        $user = $request->user();
        assert($user !== null);

        return [
            'id' => $user->id,
            'name' => $user->name,
            'username' => $user->username,
            'email' => $user->email,
            'inboxEnabled' => $user->inbox_enabled,
            'usernameRoutingEnabled' => app(InstanceSettings::class)->boolean('username_routing'),
        ];
    }
}
