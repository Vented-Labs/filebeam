#!/usr/bin/env php
<?php

declare(strict_types=1);

if (! isset($argc, $argv) || ($argc !== 2 && $argc !== 3)) {
    fwrite(STDERR, "Usage: cli-update-index.php CURRENT [RELEASE]\n");
    exit(64);
}

$current = is_file($argv[1]) ? json_decode((string) file_get_contents($argv[1]), true, 512, JSON_THROW_ON_ERROR) : ['generation' => 0, 'releases' => []];
$release = $argc === 3 ? json_decode((string) file_get_contents($argv[2]), true, 512, JSON_THROW_ON_ERROR) : null;
if ($release !== null && (! is_array($release) || ! is_string($release['tag'] ?? null) || ! is_string($release['version'] ?? null) || ! is_array($release['assets'] ?? null))) {
    throw new RuntimeException('Invalid CLI release metadata.');
}
$releases = is_array($current['releases'] ?? null) ? $current['releases'] : [];
$found = false;
foreach ($releases as $index => $entry) {
    if ($release !== null && ($entry['tag'] ?? null) === $release['tag']) {
        $releases[$index] = $release;
        $found = true;
    }
}
if ($release !== null && ! $found) {
    $releases[] = $release;
}
usort($releases, static fn (array $left, array $right): int => version_compare((string) $right['version'], (string) $left['version']));
echo json_encode([
    'schema' => 1,
    'generation' => (int) ($current['generation'] ?? 0) + 1,
    'published_at' => gmdate('Y-m-d\TH:i:s\Z'),
    'expires_at' => gmdate('Y-m-d\TH:i:s\Z', time() + 7 * 24 * 60 * 60),
    'releases' => $releases,
], JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT)."\n";
