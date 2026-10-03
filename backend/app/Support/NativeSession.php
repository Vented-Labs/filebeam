<?php

declare(strict_types=1);

namespace App\Support;

use App\Models\User;
use App\Services\FilebeamUrlGenerator;

final class NativeSession
{
    /** @return array<string, bool|int|string|null> */
    public static function forUser(User $user): array
    {
        return [
            'id' => $user->id,
            'name' => $user->name,
            'username' => $user->username,
            'email' => $user->email,
            'emailVerifiedAt' => $user->email_verified_at?->toIso8601String(),
            'profileUrl' => is_string($user->username) ? app(FilebeamUrlGenerator::class)->profile($user->username) : null,
            'inboxEnabled' => $user->inbox_enabled,
            'usernameRoutingEnabled' => app(InstanceSettings::class)->boolean('username_routing'),
            'notificationChannel' => $user->notification_channel ?? 'mail',
            'receivingPolicy' => $user->receiving_policy->value,
            'autoDownloadFriends' => $user->auto_download_friends,
            'receivingRevision' => $user->receiving_revision,
        ];
    }
}
