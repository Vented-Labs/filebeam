#!/usr/bin/env php
<?php

declare(strict_types=1);

$payload = stream_get_contents(STDIN);
$secret = base64_decode(getenv('RELEASE_SIGNING_KEY') ?: '', true);
$expectedPublic = base64_decode(getenv('RELEASE_PUBLIC_KEY') ?: '', true);
if ($secret === false || strlen($secret) !== SODIUM_CRYPTO_SIGN_SECRETKEYBYTES) {
    fwrite(STDERR, "RELEASE_SIGNING_KEY must be a base64-encoded Ed25519 secret key.\n");
    exit(1);
}
$public = sodium_crypto_sign_publickey_from_secretkey($secret);
if ($expectedPublic === false || ! hash_equals($public, $expectedPublic)) {
    fwrite(STDERR, "RELEASE_PUBLIC_KEY does not match RELEASE_SIGNING_KEY.\n");
    exit(1);
}
echo json_encode([
    'signed' => base64_encode($payload),
    'signature' => base64_encode(sodium_crypto_sign_detached($payload, $secret)),
], JSON_UNESCAPED_SLASHES)."\n";
