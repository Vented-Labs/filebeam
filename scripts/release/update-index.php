#!/usr/bin/env php
<?php

declare(strict_types=1);

use Filebeam\Updater\Semver;

require dirname(__DIR__, 2).'/updater/Updater.php';

if (! isset($argc, $argv) || $argc !== 7) {
    exit(64);
}

$currentPath = $argv[1];
$releasePath = $argv[2];
$securityRelease = $argv[3];
$withdrawn = $argv[4];
$notesUrl = $argv[5];
$refreshTag = $argv[6];
$current = is_file($currentPath) ? json_decode((string) file_get_contents($currentPath), true, 512, JSON_THROW_ON_ERROR) : null;
$release = $releasePath === '-' ? null : json_decode((string) file_get_contents($releasePath), true, 512, JSON_THROW_ON_ERROR);
$tag = $release['tag'] ?? $refreshTag;
if ($tag === '__refresh__' && $release === null) {
    $tag = null;
}
if ($tag !== null && ! is_string($tag)) {
    throw new RuntimeException('A release tag is required.');
}

$releases = $current['releases'] ?? [];
$found = false;
foreach ($releases as &$entry) {
    if ($tag === null) {
        continue;
    }
    if (($entry['tag'] ?? null) !== $tag) {
        continue;
    }
    $found = true;
    if ($release !== null) {
        $entry = array_merge($entry, $release);
    }
    if ($securityRelease !== '__preserve__') {
        $entry['security_release'] = $securityRelease === 'true';
    }
    if ($withdrawn !== '__preserve__') {
        $entry['withdrawn'] = $withdrawn === 'true';
    }
    if ($notesUrl !== '__preserve__') {
        $entry['notes_url'] = $notesUrl === '-' ? null : $notesUrl;
    }
}
unset($entry);
if (! $found && $tag !== null) {
    if ($release === null) {
        throw new RuntimeException("Cannot reclassify unknown release {$tag}.");
    }
    $release['security_release'] = $securityRelease === '__preserve__' ? false : $securityRelease === 'true';
    $release['withdrawn'] = $withdrawn === '__preserve__' ? false : $withdrawn === 'true';
    $release['notes_url'] = $notesUrl === '__preserve__' || $notesUrl === '-' ? null : $notesUrl;
    $releases[] = $release;
}
usort($releases, static fn (array $left, array $right): int => Semver::compare((string) $right['version'], (string) $left['version']));
$now = new DateTimeImmutable('now', new DateTimeZone('UTC'));
$index = [
    'schema' => 1,
    'generation' => (int) ($current['generation'] ?? 0) + 1,
    'published_at' => $now->format('Y-m-d\\TH:i:s\\Z'),
    'expires_at' => $now->modify('+7 days')->format('Y-m-d\\TH:i:s\\Z'),
    'releases' => $releases,
];
echo json_encode($index, JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT)."\n";
