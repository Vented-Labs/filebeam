<?php

declare(strict_types=1);

namespace App\Http\Controllers\Auth;

use App\Actions\Auth\AcceptInvitation;
use App\Http\Controllers\Controller;
use App\Http\Requests\Auth\AcceptInvitationRequest;
use App\Models\UserInvitation;
use Illuminate\Http\RedirectResponse;
use Illuminate\Support\Facades\Auth;
use Inertia\Inertia;
use Inertia\Response;
use Throwable;

class AcceptInvitationController extends Controller
{
    public function create(string $token): Response
    {
        $invitation = $this->invitation($token);

        abort_if($invitation === null || $invitation->accepted_at !== null || $invitation->expires_at->isPast(), 404);

        return Inertia::render('auth/AcceptInvitation', [
            'token' => $token,
            'email' => $invitation->email,
        ]);
    }

    private function invitation(string $token): ?UserInvitation
    {
        return UserInvitation::query()->where('token_hash', hash('sha256', $token))->first();
    }

    /**
     * @throws Throwable
     */
    public function store(AcceptInvitationRequest $request, string $token, AcceptInvitation $acceptInvitation): RedirectResponse
    {
        /** @var array{username: string, name: string|null, email: string, password: string} $attributes */
        $attributes = $request->validated();
        $user = $acceptInvitation->handle($token, $attributes);

        Auth::login($user);
        $request->session()->regenerate();

        return to_route('account');
    }
}
