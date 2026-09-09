#!/usr/bin/env php
<?php

declare(strict_types=1);

$envelope = json_decode(stream_get_contents(STDIN), true, 512, JSON_THROW_ON_ERROR);
$public = base64_decode(getenv('RELEASE_PUBLIC_KEY') ?: '', true);
if (! is_array($envelope) || ! is_string($envelope['signed'] ?? null) || ! is_string($envelope['signature'] ?? null) || $public === false || strlen($public) !== SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES) {
    throw new RuntimeException('CLI index signature verification failed.');
}
$payload = base64_decode($envelope['signed'], true);
$signature = base64_decode($envelope['signature'], true);
if ($payload === false || $signature === false || $signature === '' || strlen($signature) !== SODIUM_CRYPTO_SIGN_BYTES || ! sodium_crypto_sign_verify_detached($signature, $payload, $public)) {
    throw new RuntimeException('CLI index signature verification failed.');
}
$index = json_decode($payload, true, 512, JSON_THROW_ON_ERROR);
if (! is_array($index) || ($index['schema'] ?? null) !== 1 || ! is_int($index['generation'] ?? null) || ! is_string($index['expires_at'] ?? null) || strtotime($index['expires_at']) === false || strtotime($index['expires_at']) <= time() || ! is_array($index['releases'] ?? null)) {
    throw new RuntimeException('Invalid CLI index.');
}
echo $payload;
