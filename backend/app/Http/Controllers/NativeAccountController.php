<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Models\AccountKeyBundle;
use App\Models\User;
use App\Notifications\InboxTransferCompleted;
use App\Support\InstanceSettings;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;

class NativeAccountController extends Controller
{
    public function session(Request $request): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);

        return response()->json(['data' => [
            'id' => $user->id, 'name' => $user->name, 'username' => $user->username, 'email' => $user->email,
            'inboxEnabled' => $user->inbox_enabled,
            'usernameRoutingEnabled' => app(InstanceSettings::class)->boolean('username_routing'),
        ]], 200, ['Cache-Control' => 'no-store, private']);
    }

    public function inbox(Request $request): JsonResponse
    {
        $user = $request->user();
        assert($user instanceof User);
        $user->unreadNotifications()->where('type', InboxTransferCompleted::class)->update(['read_at' => now()]);
        $data = $user->receivedTransfers()->availableAndUnexpired()->latest('completed_at')->get()->map(fn ($transfer): array => [
            'id' => $transfer->id, 'ciphertext_bytes' => $transfer->ciphertext_bytes, 'item_count' => $transfer->item_count,
            'completed_at' => $transfer->completed_at?->toIso8601String(), 'expires_at' => $transfer->expires_at->toIso8601String(),
        ])->all();

        return response()->json(['data' => $data], 200, ['Cache-Control' => 'no-store, private']);
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
