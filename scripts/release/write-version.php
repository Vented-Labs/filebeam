#!/usr/bin/env php
<?php

declare(strict_types=1);

if (! isset($argc, $argv) || $argc !== 7) {
    exit(64);
}

$path = $argv[1];
$version = $argv[2];
$tag = $argv[3];
$commit = $argv[4];
$builtAt = $argv[5];
$publicKey = $argv[6];
$content = "<?php\n\ndeclare(strict_types=1);\n\nreturn ".var_export([
    'version' => $version,
    'tag' => $tag,
    'commit' => $commit,
    'distribution' => 'package',
    'built_at' => $builtAt,
    'update_public_key' => $publicKey,
], true).";\n";
file_put_contents($path, $content) !== false || exit(1);
