<?php

declare(strict_types=1);

$arguments = $_SERVER['argv'] ?? [];

if (count($arguments) !== 2) {
    fwrite(STDERR, "Usage: validate-og.php OG_DIRECTORY\n");
    exit(64);
}

$directory = $arguments[1];
$manifestPath = $directory.'/manifest.json';

if (! is_file($manifestPath)) {
    fwrite(STDERR, "Open Graph manifest is missing: {$manifestPath}\n");
    exit(1);
}

try {
    $manifest = json_decode((string) file_get_contents($manifestPath), true, 512, JSON_THROW_ON_ERROR);
} catch (JsonException $exception) {
    fwrite(STDERR, "Open Graph manifest is invalid: {$exception->getMessage()}\n");
    exit(1);
}

if (! is_array($manifest)) {
    fwrite(STDERR, "Open Graph manifest must contain an object.\n");
    exit(1);
}

foreach (['home', 'receive', 'transfer'] as $card) {
    $filename = $manifest[$card] ?? null;

    if (! is_string($filename) || ! preg_match('/^'.$card.'-[a-f0-9]{16}\.png$/', $filename)) {
        fwrite(STDERR, "Open Graph manifest entry for {$card} must be a hashed PNG filename.\n");
        exit(1);
    }

    $path = $directory.'/'.$filename;
    $dimensions = is_file($path) ? getimagesize($path) : false;

    if ($dimensions === false || $dimensions[0] !== 1200 || $dimensions[1] !== 630 || $dimensions[2] !== IMAGETYPE_PNG) {
        fwrite(STDERR, "Open Graph image for {$card} is missing or is not a 1200x630 PNG: {$path}\n");
        exit(1);
    }

    if ($filename !== $card.'-'.substr((string) hash_file('sha256', $path), 0, 16).'.png') {
        fwrite(STDERR, "Open Graph image hash does not match its filename: {$path}\n");
        exit(1);
    }
}
