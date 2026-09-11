<?php

declare(strict_types=1);

namespace App\Support;

use App\Enums\SocialPlatform;

/**
 * Presents the branding instance settings for public pages and email.
 */
class Branding
{
    public const KEYS = ['copyright_holder', 'copyright_year', 'copyright_url', 'github_url', 'community_links'];

    /**
     * @param  array<string, mixed>|null  $values  already resolved settings, to avoid a second query
     * @return array{copyright_holder: string, copyright_year: int, copyright_url: string|null, github_url: string, community_links: list<array{platform: string, label: string, url: string}>}
     */
    public function resolve(?array $values = null): array
    {
        $values ??= app(InstanceSettings::class)->values(self::KEYS);
        /** @var list<array{platform: string, url: string}> $links */
        $links = is_array($values['community_links'] ?? null) ? $values['community_links'] : [];

        return [
            'copyright_holder' => is_string($values['copyright_holder'] ?? null) ? $values['copyright_holder'] : '',
            'copyright_year' => is_int($values['copyright_year'] ?? null) ? $values['copyright_year'] : (int) date('Y'),
            'copyright_url' => is_string($values['copyright_url'] ?? null) ? $values['copyright_url'] : null,
            'github_url' => is_string($values['github_url'] ?? null) ? $values['github_url'] : '',
            'community_links' => array_map(static fn (array $link): array => [
                'platform' => $link['platform'],
                'label' => SocialPlatform::from($link['platform'])->getLabel(),
                'url' => $link['url'],
            ], $links),
        ];
    }
}
