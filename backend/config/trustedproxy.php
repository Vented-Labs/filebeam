<?php

declare(strict_types=1);

return [
    // Do not accept forwarded headers before the installation environment exists.
    'proxies' => is_file(app()->environmentFilePath()) ? env('FILEBEAM_TRUSTED_PROXIES', []) : [],
];
