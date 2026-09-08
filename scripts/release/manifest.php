#!/usr/bin/env php
<?php

declare(strict_types=1);

$root = $argv[1] ?? '';
if (! is_dir($root)) {
    exit(64);
}

$files = [];
$iterator = new RecursiveIteratorIterator(new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS));
foreach ($iterator as $file) {
    if (! $file->isFile()) {
        continue;
    }
    $path = substr($file->getPathname(), strlen($root) + 1);
    if ($path === 'package-files.json') {
        continue;
    }
    $files[str_replace(DIRECTORY_SEPARATOR, '/', $path)] = hash_file('sha256', $file->getPathname());
}
ksort($files, SORT_STRING);
echo json_encode(['schema' => 1, 'files' => $files], JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT)."\n";
