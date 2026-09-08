#!/usr/bin/env php
<?php

declare(strict_types=1);

if (! isset($argc, $argv) || $argc !== 3) {
    exit(64);
}

$contents = shell_exec('unzip -p '.escapeshellarg($argv[1]).' filebeam/backend/config/version.php');
if (! is_string($contents) || ! preg_match("/'update_public_key'\\s*=>\\s*'([^']*)'/", $contents, $matches)) {
    throw new RuntimeException('Archive does not contain generated version metadata.');
}
if (! hash_equals($argv[2], $matches[1])) {
    throw new RuntimeException('Archive update public key does not match RELEASE_PUBLIC_KEY.');
}
