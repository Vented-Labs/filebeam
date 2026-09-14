<?php

declare(strict_types=1);

namespace App\Http\Controllers\Auth;

use App\Http\Controllers\Controller;
use App\Models\User;
use App\Services\FilebeamUrlGenerator;
use App\Support\Branding;
use App\Support\InstanceSettings;
use Illuminate\Http\Request;
use Inertia\Inertia;
use Inertia\Response;

class AccountController extends Controller
{
    public function __invoke(Request $request, FilebeamUrlGenerator $urls): Response
    {
        $user = $request->user();
        assert($user instanceof User);

        $username = $user->username;

        $branding = app(Branding::class)->resolve();

        return Inertia::render('Account', [
            'githubUrl' => $branding['github_url'],
            'copyrightHolder' => $branding['copyright_holder'],
            'user' => [
                'id' => $user->id,
                'name' => $user->name,
                'username' => $username,
                'email' => $user->email,
                'emailVerifiedAt' => $user->email_verified_at?->toIso8601String(),
                'profileUrl' => is_string($username) ? $urls->profile($username) : null,
                'inboxEnabled' => $user->inbox_enabled,
                'usernameRoutingEnabled' => app(InstanceSettings::class)->boolean('username_routing'),
                'notificationChannel' => $user->notification_channel,
            ],
        ]);
    }
}
