#!/usr/bin/env php
<?php

declare(strict_types=1);

if (! isset($argc, $argv) || $argc !== 4) {
    fwrite(STDERR, "Usage: cli-write-release.php TAG DIRECTORY OUTPUT\n");
    exit(64);
}

[, $tag, $directory, $output] = $argv;
if (preg_match('/^beam-v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/', $tag, $matches) !== 1) {
    throw new RuntimeException('Tag must be beam-vX.Y.Z.');
}

// Keep in sync with filebeam_daemon::IPC_VERSION and filebeam_daemon::STORAGE_SCHEMA.
const DAEMON_IPC_VERSION = 1;
const DAEMON_STORAGE_SCHEMA = 1;

$version = "{$matches[1]}.{$matches[2]}.{$matches[3]}";
$assets = [];
$daemons = [];
$targets = [
    ['linux', 'x86_64', 'tar.gz'],
    ['linux', 'aarch64', 'tar.gz'],
    ['macos', 'x86_64', 'tar.gz'],
    ['macos', 'aarch64', 'tar.gz'],
    ['windows', 'x86_64', 'zip'],
];
foreach ($targets as [$os, $architecture, $extension]) {
    $name = "{$tag}-{$os}-{$architecture}.{$extension}";
    $path = $directory.'/'.$name;
    if (! is_file($path)) {
        throw new RuntimeException("Missing {$name}.");
    }
    $assets[] = [
        'architecture' => $architecture,
        'os' => $os,
        'kind' => $os === 'windows' ? 'zip-exe' : 'tar-gz',
        'path' => "versions/v{$version}/{$name}",
        'sha256' => hash_file('sha256', $path),
        'size' => filesize($path),
    ];
    if ($os === 'windows') {
        continue;
    }
    $name = "{$tag}-{$os}-{$architecture}-filebeamd.tar.gz";
    $path = $directory.'/'.$name;
    if (! is_file($path)) {
        throw new RuntimeException("Missing {$name}.");
    }
    $daemons[] = [
        'version' => $version,
        'min_ipc' => DAEMON_IPC_VERSION,
        'max_ipc' => DAEMON_IPC_VERSION,
        'storage_schema' => DAEMON_STORAGE_SCHEMA,
        'asset' => [
            'architecture' => $architecture,
            'os' => $os,
            'kind' => 'tar-gz',
            'path' => "versions/v{$version}/{$name}",
            'sha256' => hash_file('sha256', $path),
            'size' => filesize($path),
        ],
    ];
}

$sourceDateEpoch = getenv('SOURCE_DATE_EPOCH');
if ($sourceDateEpoch !== false && ! ctype_digit($sourceDateEpoch)) {
    throw new RuntimeException('SOURCE_DATE_EPOCH must be an integer timestamp.');
}
$json = json_encode([
    'tag' => $tag,
    'version' => $version,
    'published_at' => gmdate('Y-m-d\TH:i:s\Z', $sourceDateEpoch === false ? time() : (int) $sourceDateEpoch),
    'assets' => $assets,
    'daemons' => $daemons,
], JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT)."\n";
if (file_put_contents($output, $json) === false) {
    throw new RuntimeException("Unable to write {$output}.");
}
