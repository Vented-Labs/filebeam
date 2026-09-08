<?php

declare(strict_types=1);

$version = require __DIR__.'/version.php';

$branding = [
    'name' => env('FILEBEAM_NAME', 'Filebeam'),
    'logo_url' => env('FILEBEAM_LOGO_URL') ?: null,
    'favicon_url' => env('FILEBEAM_FAVICON_URL') ?: (env('FILEBEAM_LOGO_URL') ?: null),
    'version' => $version['version'],
    'copyright_holder' => env('FILEBEAM_COPYRIGHT_HOLDER', 'Vented'),
    'copyright_year' => (int) env('FILEBEAM_COPYRIGHT_YEAR', 2026),
    'github_url' => env('FILEBEAM_GITHUB_URL', 'https://github.com/Vented-Labs/filebeam'),
];
$chunkMaxSizeValue = env('CHUNK_MAX_SIZE', '25000000');

if (! is_string($chunkMaxSizeValue) || ! ctype_digit($chunkMaxSizeValue)) {
    throw new InvalidArgumentException('CHUNK_MAX_SIZE must be a whole number of bytes.');
}

$chunkMaxSize = (int) $chunkMaxSizeValue;
$stagingRequestTargetMs = max(1_000, (int) env('FILEBEAM_STAGING_REQUEST_TARGET_MS', 20_000));
$stagingRequestBudgetMs = max((int) ceil($stagingRequestTargetMs * 1.25), (int) env('FILEBEAM_STAGING_REQUEST_BUDGET_MS', 120_000));

if ($chunkMaxSize < 17 || $chunkMaxSize > 25_000_000) {
    throw new InvalidArgumentException('CHUNK_MAX_SIZE must be between 17 and 25000000 bytes.');
}

$filesystemsValue = env('FILEBEAM_FILESYSTEMS');

if ($filesystemsValue !== null && (! is_string($filesystemsValue) || trim($filesystemsValue) === '')) {
    throw new InvalidArgumentException('FILEBEAM_FILESYSTEMS must not be empty when configured.');
}

$environmentFilesystems = $filesystemsValue === null
    ? null
    : array_values(array_unique(array_map('trim', explode(',', $filesystemsValue))));

if ($environmentFilesystems !== null && in_array('', $environmentFilesystems, true)) {
    throw new InvalidArgumentException('FILEBEAM_FILESYSTEMS must not contain empty disk names.');
}
$usernameDomain = env('FILEBEAM_USERNAME_DOMAIN') ?: null;

if ($usernameDomain !== null && (! is_string($usernameDomain) || filter_var($usernameDomain, FILTER_VALIDATE_DOMAIN, FILTER_FLAG_HOSTNAME) === false)) {
    throw new InvalidArgumentException('FILEBEAM_USERNAME_DOMAIN must be an exact hostname.');
}

return [
    'username_domain' => $usernameDomain,
    'branding' => $branding,
    'github_url' => $branding['github_url'],
    'copyright_holder' => $branding['copyright_holder'],

    'updates' => [
        'state_path' => env('FILEBEAM_CONTAINER', false)
            ? env('FILEBEAM_DATA_DIR', '/data').'/app/updates'
            : base_path('../.filebeam'),
        'auto_enabled' => ! env('FILEBEAM_CONTAINER', false) && env('FILEBEAM_AUTO_UPDATES_ENABLED', false),
    ],

    'features' => [
        // These remain the PHP fallbacks. Database and environment overrides are resolved by InstanceSettings.
        'registration' => true,
        'anonymous_uploads' => true,
        'username_routing' => true,
    ],

    'instance_settings' => [
        'environment' => [
            'registration' => env('FILEBEAM_REGISTRATION_ENABLED'),
            'anonymous_uploads' => env('FILEBEAM_ANONYMOUS_UPLOADS_ENABLED'),
            'username_routing' => env('FILEBEAM_USERNAME_ROUTING_ENABLED'),
        ],
        'definitions' => [
            'registration' => [
                'label' => 'Registration',
                'description' => 'Allow new accounts to be registered.',
                'fallback' => 'filebeam.features.registration',
                'environment' => 'registration',
            ],
            'anonymous_uploads' => [
                'label' => 'Anonymous uploads',
                'description' => 'Allow visitors without an account to create uploads.',
                'fallback' => 'filebeam.features.anonymous_uploads',
                'environment' => 'anonymous_uploads',
            ],
            'username_routing' => [
                'label' => 'Username Routing',
                'description' => 'Allow public username receiving pages and inbox activation.',
                'fallback' => 'filebeam.features.username_routing',
                'environment' => 'username_routing',
            ],
        ],
    ],

    'transfers' => [
        'default_plan' => env('FILEBEAM_DEFAULT_PLAN', 'default'),
        'chunk_max_size' => $chunkMaxSize,
        'chunk_bytes' => $chunkMaxSize - 16,
        'upload_attempt_lease_seconds' => max(60, (int) env('FILEBEAM_UPLOAD_ATTEMPT_LEASE_SECONDS', 900)),
        'upload_attempt_cleanup_grace_seconds' => max(60, (int) env('FILEBEAM_UPLOAD_ATTEMPT_CLEANUP_GRACE_SECONDS', 300)),
        'upload_concurrency' => min(8, max(1, (int) env('FILEBEAM_UPLOAD_CONCURRENCY', 4))),
        'download_concurrency' => min(8, max(1, (int) env('FILEBEAM_DOWNLOAD_CONCURRENCY', 4))),
        'incomplete_expiry_hours' => (int) env('FILEBEAM_INCOMPLETE_EXPIRY_HOURS', 2),
        'pending_max_lifetime_hours' => (int) env('FILEBEAM_PENDING_MAX_LIFETIME_HOURS', 24),
        'session_limit' => (int) env('FILEBEAM_DOWNLOAD_SESSION_LIMIT', 32),
        'session_idle_minutes' => (int) env('FILEBEAM_DOWNLOAD_SESSION_IDLE_MINUTES', 20),
        'session_terminal_minutes' => (int) env('FILEBEAM_DOWNLOAD_SESSION_TERMINAL_MINUTES', 10),
    ],

    'staging' => [
        // This is always a private local path; do not point it at an object-store mount.
        'root' => env('FILEBEAM_STAGING_ROOT', storage_path('app/transfer-staging')),
        'ttl_seconds' => max(60, (int) env('FILEBEAM_STAGING_TTL_SECONDS', 3600)),
        'global_bytes' => max(0, (int) env('FILEBEAM_STAGING_GLOBAL_BYTES', 2 * 1024 * 1024 * 1024)),
        'transfer_bytes' => max(0, (int) env('FILEBEAM_STAGING_TRANSFER_BYTES', 512 * 1024 * 1024)),
        'global_max_rows' => max(1, (int) env('FILEBEAM_STAGING_GLOBAL_MAX_ROWS', 4096)),
        'transfer_max_rows' => max(1, (int) env('FILEBEAM_STAGING_TRANSFER_MAX_ROWS', 512)),
        'part_min_bytes' => 65_536,
        'part_max_bytes' => min($chunkMaxSize, max(65_536, (int) env('FILEBEAM_STAGING_PART_MAX_BYTES', 4 * 1024 * 1024))),
        'part_max_count' => max(1, (int) env('FILEBEAM_STAGING_PART_MAX_COUNT', 512)),
        'request_target_ms' => $stagingRequestTargetMs,
        'request_budget_ms' => $stagingRequestBudgetMs,
    ],

    'filesystems' => [
        'environment' => $environmentFilesystems,
        'local_root' => env('FILEBEAM_FILESTORE_ROOT', storage_path('app/transfers')),
    ],

    'rate_limits' => [
        'creations_per_hour' => (int) env('FILEBEAM_CREATIONS_PER_HOUR', 30),
        'writes_per_minute' => (int) env('FILEBEAM_WRITES_PER_MINUTE', 600),
        'reads_per_minute' => (int) env('FILEBEAM_READS_PER_MINUTE', 1200),
        'monitor_per_minute' => (int) env('FILEBEAM_MONITOR_PER_MINUTE', 120),
        'session_registration_per_minute' => (int) env('FILEBEAM_SESSION_REGISTRATION_PER_MINUTE', 30),
        'session_reporting_per_minute' => (int) env('FILEBEAM_SESSION_REPORTING_PER_MINUTE', 240),
    ],

    'default_plan' => [
        'maximum_transfer_bytes' => (int) env('FILEBEAM_DEFAULT_TRANSFER_BYTES', 2 * 1024 * 1024 * 1024),
        'maximum_file_count' => (int) env('FILEBEAM_DEFAULT_FILE_COUNT', 20),
        'maximum_note_bytes' => (int) env('FILEBEAM_DEFAULT_NOTE_BYTES', 1024 * 1024),
        'file_retention_hours' => (int) env('FILEBEAM_DEFAULT_FILE_RETENTION_HOURS', 24),
        'note_retention_hours' => (int) env('FILEBEAM_DEFAULT_NOTE_RETENTION_HOURS', 30 * 24),
    ],
];
