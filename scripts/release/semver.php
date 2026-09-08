#!/usr/bin/env php
<?php

declare(strict_types=1);

use Filebeam\Updater\Semver;

require dirname(__DIR__, 2).'/updater/Updater.php';

if (! isset($argc, $argv) || $argc !== 2) {
    exit(64);
}

$tag = Semver::parseTag($argv[1], true);
if ($tag === null) {
    fwrite(STDERR, "Tag must be strict SemVer, optionally prefixed with v.\n");
    exit(64);
}

echo $tag."\n";
