<?php

declare(strict_types=1);

namespace App\Http\Controllers\Auth;

use App\Http\Controllers\Controller;
use Illuminate\Http\RedirectResponse;
use Illuminate\Http\Request;
use Inertia\Inertia;
use Inertia\Response;

class EmailVerificationPromptController extends Controller
{
    public function __invoke(Request $request): Response|RedirectResponse
    {
        if ($request->user()->hasVerifiedEmail()) {
            return to_route('account');
        }

        return Inertia::render('auth/AuthScreen', [
            'mode' => 'verify',
            'githubUrl' => config('filebeam.github_url'),
            'copyrightHolder' => config('filebeam.copyright_holder'),
        ]);
    }
}
