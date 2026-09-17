<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Actions\Auth\AcceptInvitation;
use App\Enums\TransferStatus;
use App\Enums\UserRole;
use App\Http\Requests\Auth\AcceptInvitationRequest;
use App\Jobs\DeleteTransfer;
use App\Models\AccountKeyBundle;
use App\Models\Transfer;
use App\Models\TransferKeyEnvelope;
use App\Models\User;
use App\Models\UserInvitation;
use App\Notifications\InboxTransferCompleted;
use App\Services\FilebeamUrlGenerator;
use App\Support\Branding;
use App\Support\EffectivePlan;
use App\Support\InstanceSettings;
use App\Support\TransportPolicy;
use Illuminate\Auth\Events\PasswordReset;
use Illuminate\Auth\Events\Verified;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Facades\Password;
use Illuminate\Support\Facades\URL;
use Illuminate\Support\Str;
use Illuminate\Validation\Rules\Password as PasswordRule;
use Illuminate\Validation\ValidationException;
use Symfony\Component\HttpKernel\Exception\NotFoundHttpException;

class NativeAccountController extends Controller
{
    public function policy(Request $request, EffectivePlan $plans, InstanceSettings $settings, TransportPolicy $transport, Branding $branding): JsonResponse
    {
        $user = $this->optionalUser($request);
        $plan = $user === null ? $plans->default() : $user->plan;
        $policy = $transport->configuration($user);
        $fileDefault = $plan === null ? config('filebeam.default_plan.file_retention_hours') : $plan->default_file_retention_hours;
        $noteDefault = $plan === null ? config('filebeam.default_plan.note_retention_hours') : $plan->default_note_retention_hours;
        $branding = $branding->resolve();

        return response()->json(['data' => [
            'instance' => [
                'name' => config('app.name'),
                'url' => rtrim((string) config('app.url'), '/'),
            ],
            'branding' => ['githubUrl' => $branding['github_url'], 'communityLinks' => $branding['community_links']],
            'account' => [
                'registrationEnabled' => $settings->boolean('registration'),
                'usernameRoutingEnabled' => $settings->boolean('username_routing'),
            ],
            'transfer' => [
                'anonymousUploadsEnabled' => $settings->boolean('anonymous_uploads'),
                'defaultDriver' => $policy['default_driver'],
                'enabledDrivers' => $policy['enabled_drivers'],
                'chunkBytes' => config('filebeam.transfers.chunk_bytes'),
                'limits' => $policy['limits'],
                'retention' => [
                    'files' => ['defaultHours' => $fileDefault, 'options' => $this->retentionOptions($fileDefault, $plan === null ? $fileDefault : $plan->maximum_file_retention_hours)],
                    'notes' => ['defaultHours' => $noteDefault, 'options' => $this->retentionOptions($noteDefault, $plan === null ? $noteDefault : $plan->maximum_note_retention_hours)],
                ],
            ],
        ]], 200, ['Cache-Control' => 'no-store, private']);
    }

    public function invitation(string $token): JsonResponse
    {
        $invitation = $this->invitationForToken($token);
        abort_if($invitation === null || $invitation->accepted_at !== null || $invitation->expires_at->isPast(), 404);

        return response()->json(['data' => [
            'email' => $invitation->email,
            'expiresAt' => $invitation->expires_at->toIso8601String(),
        ]], 200, ['Cache-Control' => 'no-store, private']);
    }

    /**
     * @throws \Throwable
     */
    public function acceptInvitation(AcceptInvitationRequest $request, string $token, AcceptInvitation $acceptInvitation): JsonResponse
    {
        /** @var array{username: string, name: string|null, email: string, password: string} $attributes */
        $attributes = $request->validated();
        $user = $acceptInvitation->handle($token, $attributes);
        auth()->login($user);
        $request->session()->regenerate();
        $request->session()->put('password_hash_'.config('auth.defaults.guard'), $user->getAuthPassword());

        return response()->json(['data' => $this->sessionData($user)], 201, ['Cache-Control' => 'no-store, private']);
    }

    public function session(Request $request, FilebeamUrlGenerator $urls): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);

        return response()->json(['data' => $this->sessionData($user, $urls)], 200, ['Cache-Control' => 'no-store, private']);
    }

    public function inbox(Request $request): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);
        $data = $user->receivedTransfers()->availableAndUnexpired()->latest('completed_at')->get()->map(fn ($transfer): array => [
            'id' => $transfer->id, 'ciphertext_bytes' => $transfer->ciphertext_bytes, 'item_count' => $transfer->item_count,
            'completed_at' => $transfer->completed_at?->toIso8601String(), 'expires_at' => $transfer->expires_at->toIso8601String(),
        ])->all();

        return response()->json(['data' => $data], 200, ['Cache-Control' => 'no-store, private']);
    }

    public function markInboxNotificationsRead(Request $request): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);
        $user->unreadNotifications()->where('type', InboxTransferCompleted::class)->update(['read_at' => now()]);

        return response()->json(status: 204);
    }

    public function inboxUnreadCount(Request $request): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);

        return response()->json(['data' => [
            'unreadCount' => $user->unreadNotifications()->where('type', InboxTransferCompleted::class)->count(),
        ]], 200, ['Cache-Control' => 'no-store, private']);
    }

    public function notificationPreference(Request $request): JsonResponse
    {
        $data = $request->validate(['channel' => ['required', 'in:mail,database']]);
        $user = $request->user();
        assert($user instanceof User);
        $user->update(['notification_channel' => $data['channel']]);

        return response()->json(['data' => ['channel' => $data['channel']]], 200, ['Cache-Control' => 'no-store, private']);
    }

    public function resendVerification(Request $request): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);
        if (! $user->hasVerifiedEmail()) {
            $user->sendEmailVerificationNotification();
        }

        return response()->json(status: 204);
    }

    public function verifyEmail(Request $request): JsonResponse
    {
        $data = $request->validate(['link' => ['required', 'url', 'max:2048']]);
        $signed = Request::create($data['link'], 'GET');
        if ($signed->getSchemeAndHttpHost() !== rtrim((string) config('app.url'), '/') || ! URL::hasValidSignature($signed)) {
            throw ValidationException::withMessages(['link' => 'Invalid or expired verification link.']);
        }
        try {
            $route = app('router')->getRoutes()->match($signed);
        } catch (NotFoundHttpException) {
            throw ValidationException::withMessages(['link' => 'Invalid verification link.']);
        }
        $user = $request->user();
        assert($user instanceof User);
        $routeId = $route->parameter('id');
        $routeHash = $route->parameter('hash');
        if ($route->getName() !== 'verification.verify'
            || ! is_string($routeId)
            || ! is_string($routeHash)
            || ! hash_equals((string) $user->getKey(), $routeId)
            || ! hash_equals(sha1($user->getEmailForVerification()), $routeHash)) {
            throw ValidationException::withMessages(['link' => 'Invalid verification link.']);
        }
        if ($user->markEmailAsVerified()) {
            event(new Verified($user));
        }

        return response()->json(status: 204);
    }

    public function requestPasswordReset(Request $request): JsonResponse
    {
        $data = $request->validate(['email' => ['required', 'string', 'max:255']]);
        Password::sendResetLink(['email' => strtolower(trim($data['email']))]);

        return response()->json(status: 204);
    }

    public function resetPassword(Request $request): JsonResponse
    {
        $data = $request->validate([
            'token' => ['required', 'string'], 'email' => ['required', 'string', 'max:255'],
            'password' => ['required', 'confirmed', PasswordRule::defaults()],
        ]);
        $status = Password::reset([
            'email' => strtolower(trim($data['email'])), 'token' => $data['token'], 'password' => $data['password'],
        ], function (User $user, string $password): void {
            $user->forceFill(['password' => Hash::make($password), 'remember_token' => Str::random(60)])->save();
            event(new PasswordReset($user));
        });
        if ($status !== Password::PASSWORD_RESET) {
            throw ValidationException::withMessages(['email' => __($status)]);
        }

        return response()->json(status: 204);
    }

    public function keys(Request $request): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);
        $data = $user->accountKeyBundles()->orderBy('version')->get()->map(fn (AccountKeyBundle $bundle): array => $bundle->only('id', 'user_id', 'version', 'public_key', 'fingerprint', 'custody_mode', 'encrypted_private_key', 'is_active'))->all();

        return response()->json(['data' => $data], 200, ['Cache-Control' => 'no-store, private']);
    }

    public function recipient(string $username): JsonResponse
    {
        abort_unless(app(InstanceSettings::class)->boolean('username_routing'), 404);
        $recipient = User::query()->where('normalized_username', strtolower($username))->inboxEnabled()->whereHas('activeAccountKeyBundles')->with('activeAccountKeyBundles')->firstOrFail();
        $bundle = $recipient->activeAccountKeyBundles->sole();

        return response()->json(['data' => [
            'id' => $recipient->id, 'username' => $recipient->username, 'public_key' => $bundle->public_key,
            'account_key_bundle_id' => $bundle->id, 'version' => $bundle->version, 'fingerprint' => $bundle->fingerprint,
        ]], 200, ['Cache-Control' => 'no-store']);
    }

    /**
     * @throws \Throwable
     */
    public function destroyAccount(Request $request): JsonResponse
    {
        $data = $request->validate([
            'current_password' => ['required', 'string'],
            'confirmation' => ['required', 'in:DELETE'],
        ]);
        $user = $request->user();
        assert($user instanceof User);

        if (! Hash::check($data['current_password'], $user->password)) {
            throw ValidationException::withMessages(['current_password' => 'The current password is incorrect.']);
        }

        DB::transaction(function () use ($user): void {
            $staff = User::query()->activeStaff()->orderBy('id')->lockForUpdate()->get();
            $locked = User::query()->lockForUpdate()->findOrFail($user->id);
            if ($locked->role === UserRole::Admin && $locked->isAdmin() && $this->activeAdminCount($staff) <= 1) {
                throw ValidationException::withMessages(['confirmation' => 'At least one verified, unsuspended administrator must remain active.']);
            }

            $ownedTransfers = Transfer::query()->where('owner_id', $locked->id)->lockForUpdate()->get();
            $bundleIds = $locked->accountKeyBundles()->lockForUpdate()->pluck('id');
            TransferKeyEnvelope::query()->whereIn('account_key_bundle_id', $bundleIds)->delete();
            $locked->accountKeyBundles()->delete();
            $locked->notifications()->delete();
            DB::table('password_reset_tokens')->where('email', $locked->email)->delete();
            DB::table('sessions')->where('user_id', $locked->id)->delete();

            foreach ($ownedTransfers as $transfer) {
                if ($transfer->status !== TransferStatus::Deleting) {
                    $transfer->update(['status' => TransferStatus::Deleting]);
                }
                DB::afterCommit(fn (): mixed => DeleteTransfer::dispatch($transfer->id));
            }

            $locked->delete();
        });

        auth()->logout();
        $request->session()->invalidate();
        $request->session()->regenerateToken();

        return response()->json(['data' => [
            'status' => 'deletion_scheduled',
            'removed' => ['account' => true, 'accountKeys' => true, 'ownedTransfers' => 'scheduled_for_cleanup'],
            'preserved' => ['transfersOwnedByOthers' => true],
        ]], 202, ['Cache-Control' => 'no-store, private']);
    }

    /** @return array<string, bool|int|string|null> */
    private function sessionData(User $user, ?FilebeamUrlGenerator $urls = null): array
    {
        $urls ??= app(FilebeamUrlGenerator::class);

        return [
            'id' => $user->id, 'name' => $user->name, 'username' => $user->username, 'email' => $user->email,
            'emailVerifiedAt' => $user->email_verified_at?->toIso8601String(),
            'profileUrl' => is_string($user->username) ? $urls->profile($user->username) : null,
            'inboxEnabled' => $user->inbox_enabled,
            'usernameRoutingEnabled' => app(InstanceSettings::class)->boolean('username_routing'),
            'notificationChannel' => $user->notification_channel ?? 'mail',
        ];
    }

    private function invitationForToken(string $token): ?UserInvitation
    {
        return UserInvitation::query()->where('token_hash', hash('sha256', $token))->first();
    }

    /** @return list<int> */
    private function retentionOptions(int $defaultHours, int $maximumHours): array
    {
        $options = array_filter([1, 6, 12, 24, 72, 168, 720, 2160, 8760, $defaultHours, $maximumHours], fn (int $hours): bool => $hours <= $maximumHours);
        sort($options);

        return array_values(array_unique($options));
    }

    /** @param iterable<User> $staff */
    private function activeAdminCount(iterable $staff): int
    {
        $count = 0;
        foreach ($staff as $staffUser) {
            if ($staffUser->role === UserRole::Admin) {
                $count++;
            }
        }

        return $count;
    }

    protected function optionalUser(Request $request): ?User
    {
        $user = $request->user();

        return $user instanceof User ? $user : null;
    }
}
