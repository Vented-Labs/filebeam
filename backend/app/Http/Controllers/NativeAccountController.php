<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Models\AccountKeyBundle;
use App\Models\User;
use App\Notifications\InboxTransferCompleted;
use App\Services\FilebeamUrlGenerator;
use App\Support\InstanceSettings;
use Illuminate\Auth\Events\PasswordReset;
use Illuminate\Auth\Events\Verified;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Facades\Password;
use Illuminate\Support\Str;
use Illuminate\Support\Facades\URL;
use Illuminate\Validation\Rules\Password as PasswordRule;
use Illuminate\Validation\ValidationException;

class NativeAccountController extends Controller
{
    public function session(Request $request, FilebeamUrlGenerator $urls): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);

        return response()->json(['data' => [
            'id' => $user->id, 'name' => $user->name, 'username' => $user->username, 'email' => $user->email,
            'emailVerifiedAt' => $user->email_verified_at?->toIso8601String(),
            'profileUrl' => is_string($user->username) ? $urls->profile($user->username) : null,
            'inboxEnabled' => $user->inbox_enabled,
            'usernameRoutingEnabled' => app(InstanceSettings::class)->boolean('username_routing'),
            'notificationChannel' => $user->notification_channel ?? 'mail',
        ]], 200, ['Cache-Control' => 'no-store, private']);
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
        } catch (\Symfony\Component\HttpKernel\Exception\NotFoundHttpException) {
            throw ValidationException::withMessages(['link' => 'Invalid verification link.']);
        }
        $user = $request->user();
        assert($user instanceof User);
        if ($route->getName() !== 'verification.verify'
            || ! hash_equals((string) $user->getKey(), (string) $route->parameter('id'))
            || ! hash_equals(sha1($user->getEmailForVerification()), (string) $route->parameter('hash'))) {
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
}
