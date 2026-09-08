<?php

declare(strict_types=1);

return [
    // Supply exact proxy IPs/CIDRs in the web-server environment before first-run setup.
    'proxies' => env('FILEBEAM_TRUSTED_PROXIES', []),
];
