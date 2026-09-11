<?php

declare(strict_types=1);

use App\Support\InstanceSettingValue;

$version = require __DIR__.'/version.php';

$branding = [
    'name' => env('FILEBEAM_NAME', 'Filebeam'),
    'logo_url' => env('FILEBEAM_LOGO_URL') ?: null,
    'favicon_url' => env('FILEBEAM_FAVICON_URL') ?: (env('FILEBEAM_LOGO_URL') ?: null),
    'version' => $version['version'],
    // PHP fallbacks for the admin-editable branding resolved by App\Support\Branding.
    'copyright_holder' => 'Vented',
    'copyright_year' => 2026,
    'copyright_url' => null,
    'github_url' => 'https://github.com/Vented-Labs/filebeam',
    'community_links' => [],
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

$transportDriversValue = env('FILEBEAM_ENABLED_TRANSFER_DRIVERS');

if ($transportDriversValue !== null && ! is_string($transportDriversValue)) {
    throw new InvalidArgumentException('FILEBEAM_ENABLED_TRANSFER_DRIVERS must be a JSON array.');
}

$transportDrivers = $transportDriversValue === null ? null : json_decode($transportDriversValue, true, 512, JSON_THROW_ON_ERROR);

if ($transportDrivers !== null && (! is_array($transportDrivers) || ! array_is_list($transportDrivers) || $transportDrivers === [] || array_filter($transportDrivers, 'is_string') !== $transportDrivers)) {
    throw new InvalidArgumentException('FILEBEAM_ENABLED_TRANSFER_DRIVERS must be a non-empty JSON array of driver names.');
}

$transportDefaultDriver = env('FILEBEAM_DEFAULT_TRANSFER_DRIVER') ?: null;

if ($transportDefaultDriver !== null && ! is_string($transportDefaultDriver)) {
    throw new InvalidArgumentException('FILEBEAM_DEFAULT_TRANSFER_DRIVER must be a driver name.');
}

$knownTransferDrivers = ['http', 'webrtc'];

if (($transportDrivers === null) !== ($transportDefaultDriver === null)) {
    throw new InvalidArgumentException('FILEBEAM_ENABLED_TRANSFER_DRIVERS and FILEBEAM_DEFAULT_TRANSFER_DRIVER must be configured together.');
}

if ($transportDrivers !== null && (array_diff($transportDrivers, $knownTransferDrivers) !== [] || ! in_array($transportDefaultDriver, $transportDrivers, true))) {
    throw new InvalidArgumentException('The configured default transfer driver must be known and enabled.');
}

$turnUrls = array_values(array_filter(array_map('trim', explode(',', (string) env('FILEBEAM_WEBRTC_TURN_URLS', '')))));
$iceServersValue = env('FILEBEAM_WEBRTC_ICE_SERVERS');

if ($iceServersValue !== null && ! is_string($iceServersValue)) {
    throw new InvalidArgumentException('FILEBEAM_WEBRTC_ICE_SERVERS must be a JSON array.');
}

$iceServers = $iceServersValue === null
    ? ($turnUrls === [] ? [['urls' => ['stun:stun.vented.com:3478']]] : [])
    : json_decode($iceServersValue, true, 512, JSON_THROW_ON_ERROR);

if (! is_array($iceServers) || ! array_is_list($iceServers)) {
    throw new InvalidArgumentException('FILEBEAM_WEBRTC_ICE_SERVERS must be a JSON array.');
}

foreach ($iceServers as $iceServer) {
    if (! is_array($iceServer) || array_diff(array_keys($iceServer), ['urls']) !== []) {
        throw new InvalidArgumentException('FILEBEAM_WEBRTC_ICE_SERVERS entries may contain only STUN URLs.');
    }
    $urls = $iceServer['urls'] ?? null;
    $urls = is_string($urls) ? [$urls] : $urls;

    if (! is_array($urls) || $urls === [] || ! array_is_list($urls) || array_filter($urls, static fn (mixed $url): bool => is_string($url) && (str_starts_with($url, 'stun:') || str_starts_with($url, 'stuns:'))) !== $urls) {
        throw new InvalidArgumentException('FILEBEAM_WEBRTC_ICE_SERVERS must contain non-empty stun: or stuns: URL lists.');
    }
}

$webrtcSessionLimit = (int) env('FILEBEAM_WEBRTC_SESSION_LIMIT', 8);
$webrtcMaxSdpBytes = (int) env('FILEBEAM_WEBRTC_MAX_SDP_BYTES', 65_536);

if ($webrtcSessionLimit < 1 || $webrtcSessionLimit > 128 || $webrtcMaxSdpBytes < 1_024 || $webrtcMaxSdpBytes > 1_048_576) {
    throw new InvalidArgumentException('WebRTC session and SDP limits are outside supported bounds.');
}

$settingEnvironmentNames = [
    'registration' => ['FILEBEAM_REGISTRATION_ENABLED', 'boolean'],
    'anonymous_uploads' => ['FILEBEAM_ANONYMOUS_UPLOADS_ENABLED', 'boolean'],
    'username_routing' => ['FILEBEAM_USERNAME_ROUTING_ENABLED', 'boolean'],
    'copyright_holder' => ['FILEBEAM_COPYRIGHT_HOLDER', 'text'],
    'copyright_year' => ['FILEBEAM_COPYRIGHT_YEAR', 'year'],
    'copyright_url' => ['FILEBEAM_COPYRIGHT_URL', 'url'],
    'github_url' => ['FILEBEAM_GITHUB_URL', 'url'],
    'community_links' => ['FILEBEAM_COMMUNITY_LINKS', 'community_links'],
];
$settingEnvironment = ['enabled_drivers' => $transportDrivers, 'default_driver' => $transportDefaultDriver];

foreach ($settingEnvironmentNames as $key => [$name, $type]) {
    $value = env($name);
    $value = $value === '' ? null : $value;

    if ($value !== null && $type === 'community_links') {
        if (! is_string($value)) {
            throw new InvalidArgumentException("{$name} must be a JSON array.");
        }
        $value = json_decode($value, true, 512, JSON_THROW_ON_ERROR);
    }

    try {
        $settingEnvironment[$key] = $value === null ? null : InstanceSettingValue::normalize($type, $value);
    } catch (InvalidArgumentException $exception) {
        throw new InvalidArgumentException("{$name}: ".$exception->getMessage());
    }
}

$cliInstallerUrl = env('FILEBEAM_CLI_INSTALLER_URL') ?: 'https://releases.filebeam.io/cli/install.sh';
$cliWindowsInstallerUrl = env('FILEBEAM_CLI_WINDOWS_INSTALLER_URL') ?: 'https://releases.filebeam.io/cli/install.ps1';
foreach (['FILEBEAM_CLI_INSTALLER_URL' => $cliInstallerUrl, 'FILEBEAM_CLI_WINDOWS_INSTALLER_URL' => $cliWindowsInstallerUrl] as $name => $installerUrl) {
    if (! is_string($installerUrl)
        || ! filter_var($installerUrl, FILTER_VALIDATE_URL)
        || parse_url($installerUrl, PHP_URL_SCHEME) !== 'https'
        || parse_url($installerUrl, PHP_URL_USER) !== null
        || parse_url($installerUrl, PHP_URL_PASS) !== null
        || parse_url($installerUrl, PHP_URL_FRAGMENT) !== null
        || preg_match('/[\x00-\x20\x7f]/', $installerUrl)
        || preg_match('/(?:^|\.)example$/i', (string) parse_url($installerUrl, PHP_URL_HOST))) {
        throw new InvalidArgumentException("{$name} must be a published HTTPS URL without credentials or a fragment.");
    }
}

return [
    'username_domain' => $usernameDomain,
    'branding' => $branding,

    'cli' => [
        // Configure only after the signed CLI release and installer are published.
        'installer_url' => $cliInstallerUrl,
        'windows_installer_url' => $cliWindowsInstallerUrl,
        'installer_interpreter' => 'sh',
        'executable' => 'beam',
    ],

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
        // Normalized environment values; a non-null key locks its Admin field and wins over the database row.
        'environment' => $settingEnvironment,
        // Every admin-editable setting: its type drives validation, the fallback is a config path.
        'definitions' => [
            'registration' => [
                'label' => 'Registration',
                'description' => 'Allow new accounts to be registered.',
                'type' => 'boolean',
                'fallback' => 'filebeam.features.registration',
            ],
            'anonymous_uploads' => [
                'label' => 'Anonymous uploads',
                'description' => 'Allow visitors without an account to create uploads.',
                'type' => 'boolean',
                'fallback' => 'filebeam.features.anonymous_uploads',
            ],
            'username_routing' => [
                'label' => 'Username Routing',
                'description' => 'Allow public username receiving pages and inbox activation.',
                'type' => 'boolean',
                'fallback' => 'filebeam.features.username_routing',
            ],
            'enabled_drivers' => [
                'label' => 'Enabled drivers',
                'description' => 'Transfer drivers offered on public pages.',
                'type' => 'drivers',
                'fallback' => 'filebeam.transport_policy.defaults.enabled_drivers',
            ],
            'default_driver' => [
                'label' => 'Default driver',
                'description' => 'Transfer driver selected by default.',
                'type' => 'driver',
                'fallback' => 'filebeam.transport_policy.defaults.default_driver',
            ],
            'copyright_holder' => [
                'label' => 'Copyright holder',
                'description' => 'Name shown in the copyright line.',
                'type' => 'text',
                'fallback' => 'filebeam.branding.copyright_holder',
            ],
            'copyright_year' => [
                'label' => 'Copyright year',
                'description' => 'Year shown in the copyright line.',
                'type' => 'year',
                'fallback' => 'filebeam.branding.copyright_year',
            ],
            'copyright_url' => [
                'label' => 'Copyright link',
                'description' => 'Optional link on the copyright holder.',
                'type' => 'url',
                'fallback' => 'filebeam.branding.copyright_url',
            ],
            'github_url' => [
                'label' => 'GitHub link',
                'description' => 'Project link shown in the header navigation.',
                'type' => 'url',
                'fallback' => 'filebeam.branding.github_url',
            ],
            'community_links' => [
                'label' => 'Community links',
                'description' => 'Platform links rendered as icons in the footer.',
                'type' => 'community_links',
                'fallback' => 'filebeam.branding.community_links',
            ],
        ],
    ],

    'transport_policy' => [
        'defaults' => ['enabled_drivers' => ['http'], 'default_driver' => 'http'],
    ],

    'webrtc' => [
        'ice_servers' => $iceServers,
        'turn_urls' => $turnUrls,
        'turn_secret' => env('FILEBEAM_WEBRTC_TURN_SECRET') ?: null,
        'turn_ttl_seconds' => min(86_400, max(60, (int) env('FILEBEAM_WEBRTC_TURN_TTL_SECONDS', 3_600))),
        'session_idle_seconds' => min(3_600, max(10, (int) env('FILEBEAM_WEBRTC_SESSION_IDLE_SECONDS', 120))),
        'session_limit' => $webrtcSessionLimit,
        'max_sdp_bytes' => $webrtcMaxSdpBytes,
        'live_max_hours' => min(168, max(1, (int) env('FILEBEAM_WEBRTC_LIVE_MAX_HOURS', 24))),
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
