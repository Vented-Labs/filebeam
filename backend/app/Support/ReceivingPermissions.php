<?php

declare(strict_types=1);

namespace App\Support;

use App\Enums\ReceivingPolicy;
use App\Models\ContactBlock;
use App\Models\ContactPreference;
use App\Models\Friendship;
use App\Models\User;

final class ReceivingPermissions
{
    /** @return array{canSend: bool, autoDownload: bool} */
    public function resolve(User $recipient, ?User $sender, bool $senderAuthenticated = false): array
    {
        $denied = ['canSend' => false, 'autoDownload' => false];
        if (! app(InstanceSettings::class)->boolean('username_routing') || ! $recipient->inbox_enabled || $recipient->suspended_at !== null
            || ($senderAuthenticated && $sender === null) || $sender?->suspended_at !== null) {
            return $denied;
        }
        $friendship = null;
        $preference = null;
        if ($sender !== null) {
            if (ContactBlock::query()->between($recipient->id, $sender->id)->exists()) {
                return $denied;
            }
            $friendship = Friendship::query()->between($recipient->id, $sender->id)->whereNotNull('accepted_at')->first();
            $preference = $friendship === null ? null : ContactPreference::query()->where('friendship_id', $friendship->id)->where('user_id', $recipient->id)->first();
        }
        $allowed = $preference->can_send ?? match ($recipient->receiving_policy) {
            ReceivingPolicy::Anyone => true,
            ReceivingPolicy::Authenticated => $sender !== null,
            ReceivingPolicy::Friends => $friendship !== null,
            ReceivingPolicy::Nobody => false,
        };

        return [
            'canSend' => $allowed,
            'autoDownload' => $allowed && $friendship !== null && ($preference->auto_download ?? $recipient->auto_download_friends),
        ];
    }
}
