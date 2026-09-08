<?php

declare(strict_types=1);

namespace App\Actions\Auth;

use App\Models\AdminAudit;
use App\Models\Plan;
use App\Models\User;
use App\Models\UserInvitation;
use Illuminate\Support\Facades\DB;
use Illuminate\Validation\ValidationException;
use Throwable;

class AcceptInvitation
{
    /** @param array{username: string, name: string|null, email: string, password: string} $attributes
     * @throws Throwable
     */
    public function handle(string $token, array $attributes): User
    {
        return DB::transaction(function () use ($token, $attributes): User {
            $invitation = UserInvitation::query()
                ->where('token_hash', hash('sha256', $token))
                ->lockForUpdate()
                ->first();

            if ($invitation === null || $invitation->accepted_at !== null || $invitation->expires_at->isPast()) {
                throw ValidationException::withMessages(['email' => 'This invitation is no longer valid.']);
            }

            if ($invitation->email !== $attributes['email']) {
                throw ValidationException::withMessages(['email' => 'This email address does not match the invitation.']);
            }

            $plan = Plan::query()
                ->default(config('filebeam.transfers.default_plan'))
                ->active()
                ->firstOrFail();

            $user = User::query()->create([
                'username' => $attributes['username'],
                'normalized_username' => $attributes['username'],
                'name' => $attributes['name'] ?: $attributes['username'],
                'email' => $invitation->email,
                'password' => $attributes['password'],
                'plan_id' => $plan->id,
                'inbox_enabled' => false,
            ]);
            $user->forceFill(['email_verified_at' => now()])->save();

            $invitation->forceFill(['accepted_at' => now()])->save();

            AdminAudit::query()->create([
                'actor_id' => $invitation->invited_by,
                'action' => 'invitation.accepted',
                'target_type' => User::class,
                'target_id' => $user->getKey(),
                'changes' => ['invitation_id' => $invitation->getKey()],
            ]);

            return $user;
        });
    }
}
