#!/usr/bin/env php
<?php

declare(strict_types=1);

if (! isset($argc, $argv) || $argc !== 3 || ($argv[1] ?? null) !== 'public') {
    fwrite(STDERR, "Usage: key.php public BASE64_ED25519_PUBLIC_KEY\n");
    exit(64);
}

$key = base64_decode($argv[2], true);
if ($key === false || strlen($key) !== SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES) {
    fwrite(STDERR, "RELEASE_PUBLIC_KEY must be base64-encoded 32-byte Ed25519 public key.\n");
    exit(1);
}
