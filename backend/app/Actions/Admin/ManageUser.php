<?php

declare(strict_types=1);

namespace App\Actions\Admin;

use App\Enums\UserRole;
use App\Models\AdminAudit;
use App\Models\Plan;
use App\Models\User;
use App\Models\UserInvitation;
use App\Notifications\UserInvitationNotification;
use App\Rules\ReservedUsername;
use Illuminate\Auth\Access\AuthorizationException;
use Illuminate\Auth\Events\Registered;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Gate;
use Illuminate\Support\Facades\Validator;
use Illuminate\Support\Str;
use Illuminate\Validation\Rule;
use Illuminate\Validation\Rules\Password;
use Illuminate\Validation\ValidationException;
use Throwable;

class ManageUser
{
    /** @param array<string, mixed> $attributes
     * @throws Throwable
     */
    public function create(User $actor, array $attributes): User
    {
        Gate::forUser($actor)->authorize('create', User::class);
        $attributes = $this->validatedUserData($attributes);

        return DB::transaction(function () use ($actor, $attributes): User {
            $plan = Plan::query()
                ->default(config('filebeam.transfers.default_plan'))
                ->active()
                ->firstOrFail();

            $user = User::query()->create([
                'username' => $attributes['username'],
                'normalized_username' => $attributes['username'],
                'name' => $attributes['name'] ?: $attributes['username'],
                'email' => $attributes['email'],
                'password' => $attributes['password'],
                'plan_id' => $plan->id,
                'inbox_enabled' => false,
            ]);

            $this->audit($actor, 'user.created', $user, '', [
                'role' => UserRole::User->value,
                'plan_id' => $plan->getKey(),
            ]);
            DB::afterCommit(fn (): mixed => event(new Registered($user)));

            return $user;
        });
    }

    /** @param array<string, mixed> $attributes
     * @return array{username: string, name: string|null, email: string, password: string}
     */
    private function validatedUserData(array $attributes): array
    {
        $attributes = $this->normalizedAttributes($attributes);

        /** @var array{username: string, name: string|null, email: string, password: string} $validated */
        $validated = Validator::make($attributes, [
            'username' => ['required', 'string', 'regex:/\A[a-z0-9_]{3,24}\z/', new ReservedUsername, Rule::unique('users', 'normalized_username')],
            'name' => ['nullable', 'string', 'max:255'],
            'email' => ['required', 'string', 'email:rfc', 'max:255', Rule::unique('users', 'email')],
            'password' => ['required', 'confirmed', Password::defaults()],
        ])->validate();

        return $validated;
    }

    /** @param array<string, mixed> $attributes
     * @return array<string, mixed>
     */
    private function normalizedAttributes(array $attributes): array
    {
        $username = $attributes['username'] ?? null;
        $email = $attributes['email'] ?? null;
        $name = $attributes['name'] ?? null;

        $attributes['username'] = is_string($username) ? strtolower(trim($username)) : $username;
        $attributes['email'] = is_string($email) ? strtolower(trim($email)) : $email;
        $attributes['name'] = is_string($name) ? trim($name) ?: null : $name;

        return $attributes;
    }

    /** @param array<string, mixed> $changes */
    private function audit(User $actor, string $action, Model $target, string $reason, array $changes): void
    {
        AdminAudit::query()->create([
            'actor_id' => $actor->getKey(),
            'action' => $action,
            'target_type' => $target::class,
            'target_id' => $target->getKey(),
            'reason' => $reason,
            'changes' => $changes,
        ]);
    }

    /** @param array<string, mixed> $attributes
     * @throws Throwable
     */
    public function invite(User $actor, array $attributes): UserInvitation
    {
        Gate::forUser($actor)->authorize('create', User::class);
        $email = $this->validatedInvitationEmail($attributes);

        return DB::transaction(function () use ($actor, $email): UserInvitation {
            $invitation = UserInvitation::query()->where('email', $email)->lockForUpdate()->first();
            $isResend = $invitation !== null;
            $token = Str::random(80);

            if ($invitation === null) {
                $invitation = new UserInvitation(['email' => $email]);
            }

            $invitation->forceFill([
                'invited_by' => $actor->getKey(),
                'token_hash' => hash('sha256', $token),
                'expires_at' => now()->addHours(72),
                'accepted_at' => null,
            ])->save();

            $this->audit($actor, $isResend ? 'invitation.resent' : 'invitation.sent', $invitation, '', []);
            DB::afterCommit(function () use ($invitation, $token): void {
                $invitation->notify(new UserInvitationNotification($token));
            });

            return $invitation;
        });
    }

    /** @param array<string, mixed> $attributes */
    private function validatedInvitationEmail(array $attributes): string
    {
        $email = $attributes['email'] ?? null;
        $email = is_string($email) ? strtolower(trim($email)) : $email;

        /** @var array{email: string} $validated */
        $validated = Validator::make(['email' => $email], [
            'email' => ['required', 'string', 'email:rfc', 'max:255', Rule::unique('users', 'email')],
        ])->validate();

        return $validated['email'];
    }

    /**
     * @throws AuthorizationException
     */
    public function changeRole(User $actor, User $user, UserRole $role, string $reason): User
    {
        Gate::forUser($actor)->authorize('update', $user);
        $reason = $this->validatedReason($reason);

        return $this->mutateStaffAccount($actor, $user, $reason, function (User $lockedUser) use ($role): array {
            $from = $lockedUser->role;
            $lockedUser->forceFill(['role' => $role])->save();

            return [
                'action' => 'user.role_changed',
                'changes' => ['role' => ['from' => $from->value, 'to' => $role->value]],
            ];
        }, fn (User $lockedUser): bool => $lockedUser->role === UserRole::Admin && $role !== UserRole::Admin);
    }

    /** @throws ValidationException */
    private function validatedReason(string $reason): string
    {
        $reason = trim($reason);

        if ($reason === '' || Str::length($reason) > 2000) {
            throw ValidationException::withMessages(['reason' => 'A reason must contain between 1 and 2,000 characters.']);
        }

        return $reason;
    }

    /**
     * @param  callable(User): array{action: string, changes: array<string, mixed>}  $mutation
     * @param  callable(User): bool  $removesActiveAdmin
     *
     * @throws Throwable
     */
    private function mutateStaffAccount(User $actor, User $user, string $reason, callable $mutation, callable $removesActiveAdmin): User
    {
        return DB::transaction(function () use ($actor, $user, $reason, $mutation, $removesActiveAdmin): User {
            // Lock all staff rows in a stable order so concurrent staff changes cannot remove every active admin.
            $staff = User::query()->activeStaff()
                ->orderBy('id')
                ->lockForUpdate()
                ->get();

            $lockedUser = User::query()->whereKey($user->getKey())->lockForUpdate()->firstOrFail();
            $lockedActor = User::query()->whereKey($actor->getKey())->lockForUpdate()->firstOrFail();

            Gate::forUser($lockedActor)->authorize('update', $lockedUser);

            if ($removesActiveAdmin($lockedUser) && $this->activeAdminCount($staff) <= 1) {
                throw ValidationException::withMessages(['role' => 'At least one verified, unsuspended administrator must remain active.']);
            }

            $result = $mutation($lockedUser);
            $this->audit($actor, $result['action'], $lockedUser, $reason, $result['changes']);

            return $lockedUser;
        });
    }

    /** @param iterable<User> $staff */
    private function activeAdminCount(iterable $staff): int
    {
        $count = 0;

        foreach ($staff as $user) {
            if ($user->role === UserRole::Admin && $user->email_verified_at !== null && $user->suspended_at === null) {
                $count++;
            }
        }

        return $count;
    }

    /**
     * @throws AuthorizationException|Throwable
     */
    public function changePlan(User $actor, User $user, ?Plan $plan, string $reason): User
    {
        Gate::forUser($actor)->authorize('update', $user);
        $reason = $this->validatedReason($reason);

        return DB::transaction(function () use ($actor, $user, $plan, $reason): User {
            $lockedUsers = User::query()
                ->whereKey([$actor->getKey(), $user->getKey()])
                ->orderBy('id')
                ->lockForUpdate()
                ->get();
            $lockedActor = $lockedUsers->findOrFail($actor->getKey());
            $lockedUser = $lockedUsers->findOrFail($user->getKey());

            Gate::forUser($lockedActor)->authorize('update', $lockedUser);

            $lockedPlan = $plan === null
                ? null
                : Plan::query()->whereKey($plan->getKey())->lockForUpdate()->firstOrFail();

            if ($lockedPlan !== null && ! $lockedPlan->is_active) {
                throw ValidationException::withMessages(['plan_id' => 'An inactive plan cannot be assigned.']);
            }

            $from = $lockedUser->plan_id;
            $lockedUser->forceFill(['plan_id' => $lockedPlan?->getKey()])->save();

            $this->audit($actor, 'user.plan_changed', $lockedUser, $reason, [
                'plan_id' => ['from' => $from, 'to' => $lockedPlan?->getKey()],
            ]);

            return $lockedUser;
        });
    }

    /**
     * @throws AuthorizationException
     * @throws Throwable
     */
    public function setSuspension(User $actor, User $user, bool $suspended, string $reason): User
    {
        Gate::forUser($actor)->authorize('update', $user);
        $reason = $this->validatedReason($reason);

        return $this->mutateStaffAccount($actor, $user, $reason, function (User $lockedUser) use ($suspended): array {
            $from = $lockedUser->suspended_at;
            $lockedUser->forceFill(['suspended_at' => $suspended ? now() : null])->save();

            return [
                'action' => $suspended ? 'user.suspended' : 'user.reinstated',
                'changes' => ['suspended_at' => ['from' => $from?->toISOString(), 'to' => $lockedUser->suspended_at?->toISOString()]],
            ];
        }, fn (User $lockedUser): bool => $lockedUser->role === UserRole::Admin && $lockedUser->suspended_at === null && $suspended);
    }
}
