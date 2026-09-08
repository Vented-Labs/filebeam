#!/usr/bin/env php
<?php

declare(strict_types=1);

use Filebeam\Updater\Catalog;

require dirname(__DIR__, 2).'/updater/Updater.php';

$public = base64_decode(getenv('RELEASE_PUBLIC_KEY') ?: '', true);
if ($public === false || $public === '' || strlen($public) !== SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES) {
    throw new RuntimeException('Index signature verification failed.');
}
$index = Catalog::verify(stream_get_contents(STDIN), $public);
echo json_encode($index, JSON_UNESCAPED_SLASHES | JSON_PRETTY_PRINT)."\n";
