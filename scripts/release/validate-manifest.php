#!/usr/bin/env php
<?php

declare(strict_types=1);

use Filebeam\Updater\Package;

if (! isset($argc, $argv) || $argc !== 2 || ! is_dir($argv[1])) {
    fwrite(STDERR, "Usage: validate-manifest.php PACKAGE_ROOT\n");

    exit(64);
}

require dirname(__DIR__, 2).'/updater/Updater.php';

try {
    Package::verify($argv[1], Package::manifest($argv[1]));
} catch (Throwable $exception) {
    fwrite(STDERR, 'Manifest validation failed: '.$exception->getMessage()."\n");

    exit(1);
}
