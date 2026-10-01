<?php

declare(strict_types=1);

namespace Filebeam\Updater;

// Keep the published updater unchanged while routing its release transport to
// an isolated fixture. Signature, archive, database, and web checks still run.
function curl_init(?string $url = null): \CurlHandle|false
{
    $fixture = getenv('FILEBEAM_UPDATER_FIXTURE_URL');
    if (! is_string($fixture) || ! preg_match('~^http://127\.0\.0\.1:[0-9]+/$~', $fixture)) {
        throw new \RuntimeException('Invalid isolated release fixture URL.');
    }
    if ($url !== null && str_starts_with($url, 'https://releases.filebeam.io/')) {
        $url = $fixture.substr($url, strlen('https://releases.filebeam.io/'));
    }

    return \curl_init($url);
}
