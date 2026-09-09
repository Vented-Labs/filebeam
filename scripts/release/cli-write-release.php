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
foreach (['x86_64', 'aarch64'] as $architecture) {
    $name = "{$tag}-linux-{$architecture}.tar.gz";
    $path = $directory.'/'.$name;
    if (! is_file($path)) {
        throw new RuntimeException("Missing {$name}.");
    }
    $assets[] = [
        'architecture' => $architecture,
        'path' => "versions/v{$matches[1]}.{$matches[2]}.{$matches[3]}/{$name}",
        'sha256' => hash_file('sha256', $path),
        'size' => filesize($path),
    ];
}

$json = json_encode([
    'tag' => $tag,
    'version' => "{$matches[1]}.{$matches[2]}.{$matches[3]}",
    'published_at' => gmdate('Y-m-d\TH:i:s\Z'),
    'assets' => $assets,
], JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT)."\n";
if (file_put_contents($output, $json) === false) {
    throw new RuntimeException("Unable to write {$output}.");
}
