#!/usr/bin/env php
<?php

declare(strict_types=1);

if (! isset($argc, $argv) || $argc !== 3) {
    exit(64);
}

[$endpoint, $bucket] = [$argv[1], $argv[2]];
if (! preg_match('/^(?![0-9]+(?:\.[0-9]+){3}$)[a-z0-9](?:[a-z0-9-]{1,61}[a-z0-9])$/', $bucket)) {
    fwrite(STDERR, "R2_BUCKET must be a valid S3 bucket name.\n");
    exit(1);
}

$parts = parse_url($endpoint);
if ($parts === false
    || ($parts['scheme'] ?? null) !== 'https'
    || ! isset($parts['host'])
    || isset($parts['user']) || isset($parts['pass']) || isset($parts['port']) || isset($parts['query']) || isset($parts['fragment'])
    || ! preg_match('/^[a-f0-9]{32}\.r2\.cloudflarestorage\.com$/', strtolower($parts['host']))) {
    fwrite(STDERR, "R2_ENDPOINT_URL must be an HTTPS Cloudflare R2 account endpoint without credentials, a port, query, or fragment.\n");
    exit(1);
}

$path = $parts['path'] ?? '';
$bucketPath = '/'.$bucket;
if ($path !== '' && $path !== '/'
    && $path !== $bucketPath
    && $path !== $bucketPath.'/') {
    fwrite(STDERR, "R2_ENDPOINT_URL path must be empty, /, or exactly the configured R2_BUCKET.\n");
    exit(1);
}

echo 'https://'.strtolower($parts['host'])."\n";
