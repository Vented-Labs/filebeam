<?php

declare(strict_types=1);

return [
    'container' => env('FILEBEAM_CONTAINER', false),
    'container_variant' => env('FILEBEAM_VARIANT', 'light'),
    'data_directory' => env('FILEBEAM_DATA_DIR', '/data'),
    // Keep this directory on persistent storage, including when replacing containers.
    'state_directory' => env('FILEBEAM_CONTAINER', false) ? env('FILEBEAM_DATA_DIR', '/data').'/app/installation' : storage_path('app/installation'),
    'environment_path' => env('FILEBEAM_CONTAINER', false) ? env('FILEBEAM_DATA_DIR', '/data').'/config/.env' : base_path('.env'),
    'sqlite_directory' => env('FILEBEAM_CONTAINER', false) ? env('FILEBEAM_DATA_DIR', '/data').'/database' : database_path(),
];
