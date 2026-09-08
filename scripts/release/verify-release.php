#!/usr/bin/env php
<?php

declare(strict_types=1);

if (! isset($argc, $argv) || $argc !== 4) {
    exit(64);
}

$release = json_decode((string) file_get_contents($argv[1]), true, flags: JSON_THROW_ON_ERROR);

if (! is_array($release) || ($release['tag'] ?? null) !== $argv[2] || ($release['commit'] ?? null) !== $argv[3]) {
    throw new RuntimeException('Release metadata does not match the verified tag and commit.');
}
