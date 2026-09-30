#!/usr/bin/env php
<?php

declare(strict_types=1);
if (! isset($argc, $argv) || $argc !== 4) {
    fwrite(STDERR, "Usage: desktop-write-release.php vX.Y.Z DIRECTORY OUTPUT\n");
    exit(64);
}
[, $tag, $directory, $output] = $argv;
if (preg_match('/^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/', $tag, $m) !== 1) {
    throw new RuntimeException('Tag must be vX.Y.Z.');
}
$assets = [];
foreach ([['linux', 'x86_64', 'tar.gz', 'tar-gz'], ['linux', 'aarch64', 'tar.gz', 'tar-gz'], ['macos', 'x86_64', 'tar.gz', 'app-tar-gz'], ['macos', 'aarch64', 'tar.gz', 'app-tar-gz'], ['windows', 'x86_64', 'zip', 'zip-exe']] as [$os,$arch,$ext,$kind]) {
    $name = "filebeam-desktop-$tag-$os-$arch.$ext";
    $file = "$directory/$name";
    if (! is_file($file) || filesize($file) < 1) {
        throw new RuntimeException("Missing $name.");
    } $assets[] = ['architecture' => $arch, 'os' => $os, 'kind' => $kind, 'path' => "versions/$tag/$name", 'sha256' => hash_file('sha256', $file), 'size' => filesize($file)];
}
$json = json_encode(['schema' => 1, 'product' => 'desktop', 'tag' => $tag, 'version' => substr($tag, 1), 'published_at' => gmdate('Y-m-d\TH:i:s\Z', (int) (getenv('SOURCE_DATE_EPOCH') ?: time())), 'assets' => $assets], JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT)."\n";
if (file_put_contents($output, $json) === false) {
    throw new RuntimeException("Unable to write $output.");
}
