<?php

declare(strict_types=1);

namespace App\Enums;

use Filament\Support\Contracts\HasLabel;

enum SocialPlatform: string implements HasLabel
{
    case Discord = 'discord';
    case X = 'x';
    case Bluesky = 'bluesky';
    case Mastodon = 'mastodon';
    case Threads = 'threads';
    case GitHub = 'github';
    case YouTube = 'youtube';
    case Instagram = 'instagram';
    case Facebook = 'facebook';
    case LinkedIn = 'linkedin';
    case Reddit = 'reddit';
    case Telegram = 'telegram';
    case TikTok = 'tiktok';
    case Twitch = 'twitch';
    case Website = 'website';

    public function getLabel(): string
    {
        return match ($this) {
            self::Discord => 'Discord',
            self::X => 'X',
            self::Bluesky => 'Bluesky',
            self::Mastodon => 'Mastodon',
            self::Threads => 'Threads',
            self::GitHub => 'GitHub',
            self::YouTube => 'YouTube',
            self::Instagram => 'Instagram',
            self::Facebook => 'Facebook',
            self::LinkedIn => 'LinkedIn',
            self::Reddit => 'Reddit',
            self::Telegram => 'Telegram',
            self::TikTok => 'TikTok',
            self::Twitch => 'Twitch',
            self::Website => 'Website',
        };
    }

    /** @return array<string, string> */
    public static function options(): array
    {
        $options = [];

        foreach (self::cases() as $platform) {
            $options[$platform->value] = $platform->getLabel();
        }

        return $options;
    }
}
