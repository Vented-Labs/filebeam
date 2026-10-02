<?php

declare(strict_types=1);

// Capture ownership before config caching; defaults must not count as operator configuration.
$connectionKeys = ['MAIL_MAILER', 'MAIL_URL', 'MAIL_SCHEME', 'MAIL_HOST', 'MAIL_PORT', 'MAIL_USERNAME', 'MAIL_PASSWORD', 'MAIL_PASSWORD_FILE', 'MAIL_EHLO_DOMAIN'];

return [
    'managed' => array_any($connectionKeys, static fn (string $key): bool => env($key) !== null),
    'from_address' => env('MAIL_FROM_ADDRESS'),
    'from_name' => env('MAIL_FROM_NAME'),
];
