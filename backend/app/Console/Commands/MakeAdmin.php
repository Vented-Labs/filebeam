<?php

declare(strict_types=1);

namespace App\Console\Commands;

use App\Enums\UserRole;
use App\Models\AdminAudit;
use App\Models\User;
use App\Rules\ReservedUsername;
use Illuminate\Console\Attributes\Description;
use Illuminate\Console\Attributes\Signature;
use Illuminate\Console\Command;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Validator;
use Illuminate\Validation\Rule;
use Illuminate\Validation\Rules\Password;
use Throwable;

#[Signature('filebeam:make-admin')]
#[Description('Create an administrator or promote an existing verified, unsuspended account')]
class MakeAdmin extends Command
{
    /**
     * @throws Throwable
     */
    public function handle(): int
    {
        if (! $this->input->isInteractive()) {
            $this->error('This command requires an interactive terminal.');

            return self::FAILURE;
        }

        $email = $this->promptEmail();

        $existingUser = User::query()->where('email', $email)->first();

        if ($existingUser !== null) {
            return $this->promoteExistingUser($existingUser);
        }

        return $this->createAdministrator($email);
    }

    private function promptEmail(): string
    {
        while (true) {
            $email = strtolower(trim((string) $this->ask('Email')));
            $validator = Validator::make(['email' => $email], [
                'email' => ['required', 'string', 'email:rfc', 'max:255'],
            ]);

            if (! $validator->fails()) {
                return $email;
            }

            $this->error($validator->errors()->first('email'));
        }
    }

    /**
     * @throws Throwable
     */
    private function promoteExistingUser(User $user): int
    {
        if (! $user->hasVerifiedEmail()) {
            $this->error('The account email is not verified and cannot be promoted.');

            return self::FAILURE;
        }

        if ($user->suspended_at !== null) {
            $this->error('A suspended account cannot be promoted.');

            return self::FAILURE;
        }

        if (! $this->confirm('Promote this verified account to administrator?')) {
            $this->info('Cancelled.');

            return self::SUCCESS;
        }

        $reason = $this->promptReason();

        return DB::transaction(function () use ($user, $reason): int {
            $lockedUser = User::query()->whereKey($user->getKey())->lockForUpdate()->firstOrFail();

            if (! $lockedUser->hasVerifiedEmail() || $lockedUser->suspended_at !== null) {
                $this->error('The account must remain verified and unsuspended to be promoted.');

                return self::FAILURE;
            }

            if ($lockedUser->role !== UserRole::Admin) {
                $previousRole = $lockedUser->role;
                $lockedUser->forceFill(['role' => UserRole::Admin])->save();
                $this->audit('user.admin_granted_via_cli', $lockedUser, $reason, [
                    'role' => ['from' => $previousRole->value, 'to' => UserRole::Admin->value],
                ]);
            }

            $this->info('Administrator access granted.');
            $this->info('Sign in at /admin. Two-factor authentication is optional in your profile.');

            return self::SUCCESS;
        });
    }

    private function promptReason(): string
    {
        while (true) {
            $reason = trim((string) $this->ask('Audit reason'));

            if ($reason !== '' && mb_strlen($reason) <= 2000) {
                return $reason;
            }

            $this->error('The audit reason must contain between 1 and 2,000 characters.');
        }
    }

    /** @param array<string, mixed> $changes */
    private function audit(string $action, User $user, string $reason, array $changes): void
    {
        AdminAudit::query()->create([
            'actor_id' => null,
            'action' => $action,
            'target_type' => User::class,
            'target_id' => (string) $user->getKey(),
            'reason' => $reason,
            'changes' => $changes,
        ]);
    }

    /**
     * @throws Throwable
     */
    private function createAdministrator(string $email): int
    {
        $details = $this->promptNewUserDetails($email);

        if (! $this->confirm('I independently verified ownership of this email address.')) {
            $this->info('Cancelled.');

            return self::SUCCESS;
        }

        $reason = $this->promptReason();

        if (! $this->confirm('Create this verified administrator account?')) {
            $this->info('Cancelled.');

            return self::SUCCESS;
        }

        return DB::transaction(function () use ($details, $reason): int {
            if (User::query()->where('email', $details['email'])->lockForUpdate()->exists()) {
                $this->error('An account with this email was created while this command was running.');

                return self::FAILURE;
            }

            $user = new User($details);
            $user->forceFill([
                'role' => UserRole::Admin,
                'email_verified_at' => now(),
            ])->save();

            $this->audit('user.admin_created_via_cli', $user, $reason, [
                'email' => $user->email,
                'username' => $user->username,
                'role' => ['from' => null, 'to' => UserRole::Admin->value],
            ]);

            $this->info('Verified administrator account created.');
            $this->info('Sign in at /admin. Two-factor authentication is optional in your profile.');

            return self::SUCCESS;
        });
    }

    /** @return array{email: string, name: string, username: string, normalized_username: string, password: string} */
    private function promptNewUserDetails(string $email): array
    {
        while (true) {
            $details = [
                'email' => $email,
                'name' => trim((string) $this->ask('Name')),
                'username' => strtolower(trim((string) $this->ask('Username'))),
                'password' => (string) $this->secret('Password', false),
                'password_confirmation' => (string) $this->secret('Confirm password', false),
            ];
            $validator = Validator::make($details, [
                'username' => ['required', 'string', 'regex:/\A[a-z0-9_]{3,24}\z/', new ReservedUsername, Rule::unique('users', 'normalized_username')],
                'name' => ['required', 'string', 'max:255'],
                'email' => ['required', 'string', 'email:rfc', 'max:255', Rule::unique('users', 'email')],
                'password' => ['required', 'confirmed', Password::defaults()],
            ]);

            if (! $validator->fails()) {
                unset($details['password_confirmation']);
                $details['normalized_username'] = $details['username'];

                return $details;
            }

            $this->error($validator->errors()->first());
        }
    }
}
