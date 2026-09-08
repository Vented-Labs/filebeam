<?php

declare(strict_types=1);

return [
    // Overrides must be absolute HTTP(S) URLs to 1200x630 PNG images.
    'image_url' => env('FILEBEAM_OG_IMAGE_URL') ?: null,
    // Missing or invalid manifest entries intentionally render cards without image metadata.
    'manifest_path' => public_path('build/og/manifest.json'),
];
