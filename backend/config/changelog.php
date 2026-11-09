<?php

declare(strict_types=1);

$repository = (string) env('FILEBEAM_CHANGELOG_REPOSITORY', 'Vented-Labs/filebeam');

// The API origin is fixed; operators may choose another public repository, never another host.
if (preg_match('/^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})\/[A-Za-z0-9._-]{1,100}$/', $repository) !== 1 || str_ends_with($repository, '.git')) {
    throw new InvalidArgumentException('FILEBEAM_CHANGELOG_REPOSITORY must be an owner/repository pair.');
}

$imageOrigins = array_values(array_filter(array_map('trim', explode(',', (string) env('FILEBEAM_CHANGELOG_IMAGE_ORIGINS', 'https://github.com,https://user-images.githubusercontent.com,https://private-user-images.githubusercontent.com')))));

foreach ($imageOrigins as $origin) {
    if (preg_match('#^https://[a-z0-9.-]+(?::[0-9]{1,5})?$#i', $origin) !== 1) {
        throw new InvalidArgumentException('FILEBEAM_CHANGELOG_IMAGE_ORIGINS must list HTTPS origins without paths.');
    }
}

return [
    // Removing the page also removes its navigation and routes.
    'enabled' => (bool) env('FILEBEAM_CHANGELOG_ENABLED', true),
    // Outbound synchronization can be disabled while keeping an already populated mirror.
    'sync' => (bool) env('FILEBEAM_CHANGELOG_SYNC', true),
    'repository' => $repository,
    // Optional server-side read token; never sent to browsers.
    'token' => env('FILEBEAM_CHANGELOG_TOKEN') ?: null,
    'api_version' => '2026-03-10',
    'per_page' => 5,
    'stale_after_hours' => 24,
    'budget' => [
        'pages' => max(1, (int) env('FILEBEAM_CHANGELOG_MAX_PAGES', 20)),
        'page_bytes' => 4_000_000,
        'seconds' => 60,
        'body_bytes' => 200_000,
    ],
    // Markdown images render inline only from these HTTPS origins; others become links.
    'image_origins' => $imageOrigins,
];
