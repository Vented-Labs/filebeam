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

$assets = [];
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
        'path' => "versions/v{$matches[1]}.{$matches[2]}.{$matches[3]}/{$name}",
        'sha256' => hash_file('sha256', $path),
        'size' => filesize($path),
    ];
}

$sourceDateEpoch = getenv('SOURCE_DATE_EPOCH');
if ($sourceDateEpoch !== false && ! ctype_digit($sourceDateEpoch)) {
    throw new RuntimeException('SOURCE_DATE_EPOCH must be an integer timestamp.');
}
$json = json_encode([
    'tag' => $tag,
    'version' => "{$matches[1]}.{$matches[2]}.{$matches[3]}",
    'published_at' => gmdate('Y-m-d\TH:i:s\Z', $sourceDateEpoch === false ? time() : (int) $sourceDateEpoch),
    'assets' => $assets,
], JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT)."\n";
if (file_put_contents($output, $json) === false) {
    throw new RuntimeException("Unable to write {$output}.");
}
