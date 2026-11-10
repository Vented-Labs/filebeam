<?php

declare(strict_types=1);

/** @return array<string, mixed> */
function androidAssetLinksConfig(string $value): array
{
    $previous = getenv('FILEBEAM_ANDROID_APPS');
    putenv('FILEBEAM_ANDROID_APPS='.$value);
    $_ENV['FILEBEAM_ANDROID_APPS'] = $value;
    $_SERVER['FILEBEAM_ANDROID_APPS'] = $value;

    try {
        return require config_path('filebeam.php');
    } finally {
        putenv($previous === false ? 'FILEBEAM_ANDROID_APPS' : 'FILEBEAM_ANDROID_APPS='.$previous);
        if ($previous === false) {
            unset($_ENV['FILEBEAM_ANDROID_APPS'], $_SERVER['FILEBEAM_ANDROID_APPS']);
        } else {
            $_ENV['FILEBEAM_ANDROID_APPS'] = $previous;
            $_SERVER['FILEBEAM_ANDROID_APPS'] = $previous;
        }
    }
}

const ANDROID_TEST_FINGERPRINT = '01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF';

test('returns 404 rather than an empty statement list while unconfigured', function (): void {
    config()->set('filebeam.android.apps', []);

    $this->get('/.well-known/assetlinks.json')->assertNotFound();
});

test('serves a handle_all_urls statement for each configured app', function (): void {
    config()->set('filebeam.android.apps', [['package' => 'io.filebeam.android', 'fingerprints' => [ANDROID_TEST_FINGERPRINT]]]);

    $this->get('/.well-known/assetlinks.json')
        ->assertOk()
        ->assertHeader('Content-Type', 'application/json')
        ->assertExactJson([[
            'relation' => ['delegate_permission/common.handle_all_urls'],
            'target' => [
                'namespace' => 'android_app',
                'package_name' => 'io.filebeam.android',
                'sha256_cert_fingerprints' => [ANDROID_TEST_FINGERPRINT],
            ],
        ]]);
});

test('parses package to fingerprint maps and rejects malformed values', function (): void {
    $config = androidAssetLinksConfig(json_encode(['io.filebeam.android' => ANDROID_TEST_FINGERPRINT, 'io.filebeam.android.debug' => [ANDROID_TEST_FINGERPRINT]], JSON_THROW_ON_ERROR));

    expect($config['android']['apps'])->toBe([
        ['package' => 'io.filebeam.android', 'fingerprints' => [ANDROID_TEST_FINGERPRINT]],
        ['package' => 'io.filebeam.android.debug', 'fingerprints' => [ANDROID_TEST_FINGERPRINT]],
    ]);
    expect(androidAssetLinksConfig('')['android']['apps'])->toBe([]);
    expect(fn () => androidAssetLinksConfig('{"io.filebeam.android":"not-a-fingerprint"}'))->toThrow(InvalidArgumentException::class);
    expect(fn () => androidAssetLinksConfig('["io.filebeam.android"]'))->toThrow(InvalidArgumentException::class);
});
