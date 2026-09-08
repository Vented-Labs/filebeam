<?php

declare(strict_types=1);

namespace App\Http\Controllers\Auth;

use App\Http\Controllers\Controller;
use App\Http\Requests\Auth\ForgotPasswordRequest;
use App\Support\AuthIdentifier;
use Illuminate\Http\RedirectResponse;
use Illuminate\Support\Facades\Password;
use Inertia\Inertia;
use Inertia\Response;

class PasswordResetLinkController extends Controller
{
    public function create(): Response
    {
        return Inertia::render('auth/AuthScreen', [
            'mode' => 'forgot',
            'githubUrl' => config('filebeam.github_url'),
            'copyrightHolder' => config('filebeam.copyright_holder'),
        ]);
    }

    public function store(ForgotPasswordRequest $request): RedirectResponse
    {
        Password::sendResetLink(AuthIdentifier::credentials($request->validated('email')));

        return to_route('password.request')->with('status', 'If an account matches that address, a password reset link will be sent.');
    }
}
