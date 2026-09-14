<?php

declare(strict_types=1);

namespace App\Http\Controllers\Auth;

use App\Http\Controllers\Controller;
use App\Http\Requests\Auth\RegisterRequest;
use App\Models\Plan;
use App\Models\User;
use App\Support\Branding;
use App\Support\InstanceSettings;
use Illuminate\Auth\Events\Registered;
use Illuminate\Http\RedirectResponse;
use Illuminate\Support\Facades\Auth;
use Inertia\Inertia;
use Inertia\Response;

class RegisteredUserController extends Controller
{
    public function create(): Response
    {
        abort_unless(app(InstanceSettings::class)->boolean('registration'), 404);

        $branding = app(Branding::class)->resolve();

        return Inertia::render('auth/AuthScreen', [
            'mode' => 'register',
            'githubUrl' => $branding['github_url'],
            'copyrightHolder' => $branding['copyright_holder'],
            'registrationEnabled' => true,
        ]);
    }

    public function store(RegisterRequest $request): RedirectResponse
    {
        abort_unless(app(InstanceSettings::class)->boolean('registration'), 404);

        $validated = $request->validated();
        $plan = Plan::query()
            ->default(config('filebeam.transfers.default_plan'))
            ->active()
            ->firstOrFail();

        $user = User::query()->create([
            'username' => $validated['username'],
            'normalized_username' => $validated['username'],
            'name' => $validated['name'] ?: $validated['username'],
            'email' => $validated['email'],
            'password' => $validated['password'],
            'plan_id' => $plan->id,
            'inbox_enabled' => false,
        ]);

        event(new Registered($user));
        Auth::login($user);
        $request->session()->regenerate();

        return to_route('verification.notice');
    }
}
