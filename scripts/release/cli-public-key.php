#!/usr/bin/env php
<?php

declare(strict_types=1);

if (! isset($argc, $argv) || $argc !== 2 || ! in_array($argv[1], ['derive', 'public'], true)) {
    fwrite(STDERR, "Usage: cli-public-key.php derive|public\n");
    exit(64);
}

/** @return non-empty-string */
function decodeCliReleaseKey(string $name, int $length): string
{
    $key = base64_decode(getenv($name) ?: '', true);
    if ($key === false || $key === '' || strlen($key) !== $length) {
        fwrite(STDERR, "$name must be a base64-encoded $length-byte Ed25519 key.\n");
        exit(1);
    }

    return $key;
}

if ($argv[1] === 'public') {
    $public = decodeCliReleaseKey('BEAM_RELEASE_PUBLIC_KEY', SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES);
} else {
    $secret = decodeCliReleaseKey('RELEASE_SIGNING_KEY', SODIUM_CRYPTO_SIGN_SECRETKEYBYTES);
    $public = sodium_crypto_sign_publickey_from_secretkey($secret);
    sodium_memzero($secret);

    // The signing key is authoritative; an optional configured public key must agree.
    $expected = getenv('RELEASE_PUBLIC_KEY');
    if ($expected !== false && $expected !== '') {
        if (! hash_equals($public, decodeCliReleaseKey('RELEASE_PUBLIC_KEY', SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES))) {
            fwrite(STDERR, "RELEASE_PUBLIC_KEY does not match RELEASE_SIGNING_KEY.\n");
            exit(1);
        }
    }
}

echo base64_encode($public)."\n";
