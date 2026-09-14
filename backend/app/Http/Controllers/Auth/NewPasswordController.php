<?php

declare(strict_types=1);

namespace App\Http\Controllers\Auth;

use App\Http\Controllers\Controller;
use App\Http\Requests\Auth\ResetPasswordRequest;
use App\Models\User;
use App\Support\AuthIdentifier;
use App\Support\Branding;
use Illuminate\Auth\Events\PasswordReset;
use Illuminate\Http\RedirectResponse;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Facades\Password;
use Illuminate\Support\Str;
use Illuminate\Validation\ValidationException;
use Inertia\Inertia;
use Inertia\Response;

class NewPasswordController extends Controller
{
    public function create(string $token): Response
    {
        $branding = app(Branding::class)->resolve();

        return Inertia::render('auth/AuthScreen', [
            'mode' => 'reset',
            'token' => $token,
            'email' => request('email'),
            'githubUrl' => $branding['github_url'],
            'copyrightHolder' => $branding['copyright_holder'],
        ]);
    }

    public function store(ResetPasswordRequest $request): RedirectResponse
    {
        $credentials = [
            ...$request->safe()->except('email'),
            ...AuthIdentifier::credentials($request->validated('email')),
        ];

        $status = Password::reset($credentials, function (User $user, string $password): void {
            $user->forceFill([
                'password' => Hash::make($password),
                'remember_token' => Str::random(60),
            ])->save();

            event(new PasswordReset($user));
        });

        if ($status !== Password::PASSWORD_RESET) {
            throw ValidationException::withMessages(['email' => __($status)])->redirectTo(route('password.reset', [
                'token' => $request->validated('token'),
                'email' => $request->validated('email'),
            ]));
        }

        return to_route('login')->with('status', 'Your password has been reset. Existing encrypted inbox keys still need their original password. If it is lost, a replacement key can receive new files but cannot recover old ones.');
    }
}
