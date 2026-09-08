#!/usr/bin/env php
<?php

declare(strict_types=1);

if (! isset($argc, $argv) || $argc !== 10) {
    exit(64);
}

$path = $argv[1];
$tag = $argv[2];
$version = $argv[3];
$commit = $argv[4];
$publishedAt = $argv[5];
$packagePath = $argv[6];
$sha256 = $argv[7];
$size = $argv[8];
$minimumPhp = $argv[9];
$release = [
    'schema' => 1,
    'tag' => $tag,
    'version' => $version,
    'commit' => $commit,
    'published_at' => $publishedAt,
    'built_at' => $publishedAt,
    'package' => ['path' => $packagePath, 'sha256' => $sha256, 'size' => (int) $size],
    'requirements' => ['php' => $minimumPhp, 'extensions' => ['curl', 'zip', 'sodium', 'mbstring', 'dom']],
    'minimum_updater' => 1,
    'minimum_version' => '0.0.0',
];
file_put_contents($path, json_encode($release, JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT)."\n") !== false || exit(1);
