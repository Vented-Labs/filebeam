<?php

declare(strict_types=1);

require dirname(__DIR__, 2).'/updater/Updater.php';

use Filebeam\Updater\Catalog;
use Filebeam\Updater\Package;
use Filebeam\Updater\Semver;
use Filebeam\Updater\State;

$required = ['sodium', 'zip'];
foreach ($required as $extension) {
    if (! extension_loaded($extension)) {
        fwrite(STDERR, "Required PHP extension is unavailable: {$extension}\n");
        exit(2);
    }
}

$checks = 0;
$failures = 0;
function check(bool $value, string $message): void
{
    global $checks, $failures;
    $checks++;
    if (! $value) {
        fwrite(STDERR, "FAIL: {$message}\n");
        $failures++;
    }
}

function rejects(callable $callback, string $message): void
{
    try {
        $callback();
        check(false, $message);
    } catch (Throwable) {
        check(true, $message);
    }
}

function removeDirectory(string $directory): void
{
    if (! is_dir($directory)) {
        return;
    }
    $iterator = new RecursiveIteratorIterator(new RecursiveDirectoryIterator($directory, FilesystemIterator::SKIP_DOTS), RecursiveIteratorIterator::CHILD_FIRST);
    foreach ($iterator as $file) {
        $file->isDir() ? rmdir($file->getPathname()) : unlink($file->getPathname());
    }
    rmdir($directory);
}

$temporary = sys_get_temp_dir().'/filebeam-updater-test-'.bin2hex(random_bytes(8));
mkdir($temporary, 0700, true);
try {
    check(Semver::parseTag('v1.2.3+build.7') === '1.2.3', 'normalizes a valid semantic version');
    check(Semver::parseTag('1.2') === null && Semver::parseTag('1.2.3-01') === null, 'rejects invalid semantic versions');
    check(Semver::compare('1.0.0-alpha.2', '1.0.0-alpha.10') < 0, 'orders numeric prerelease identifiers numerically');
    check(Semver::compare('1.0.0-rc.1', '1.0.0') < 0, 'orders stable releases after prereleases');
    check(Package::safePath('backend/app/Example.php'), 'accepts a package-relative path');
    check(! Package::safePath('../.env') && ! Package::safePath('/etc/passwd') && ! Package::safePath('.filebeam/status.json'), 'rejects unsafe package paths');
    check(Package::protected('backend/.env') && Package::protected('backend/storage/logs/x') && Package::protected('backend/database/a.sqlite'), 'preserves mutable paths');

    $pair = sodium_crypto_sign_keypair();
    $payload = json_encode([
        'schema' => 1,
        'generation' => 4,
        'published_at' => gmdate('c'),
        'expires_at' => gmdate('c', time() + 3600),
        'releases' => [],
    ], JSON_THROW_ON_ERROR);
    $envelope = json_encode([
        'signed' => base64_encode($payload),
        'signature' => base64_encode(sodium_crypto_sign_detached($payload, sodium_crypto_sign_secretkey($pair))),
    ], JSON_THROW_ON_ERROR);
    check(Catalog::verify($envelope, sodium_crypto_sign_publickey($pair))['generation'] === 4, 'production catalog parser accepts a signed catalog');
    $modifiedEnvelope = json_decode($envelope, true, 16, JSON_THROW_ON_ERROR);
    $modifiedEnvelope['signed'] = base64_encode($payload.' ');
    rejects(fn (): array => Catalog::verify(json_encode($modifiedEnvelope, JSON_THROW_ON_ERROR), sodium_crypto_sign_publickey($pair)), 'production catalog parser rejects modified signatures');
    $expiredPayload = json_encode([
        'schema' => 1,
        'generation' => 4,
        'expires_at' => '2026-01-01T00:00:00Z',
        'releases' => [],
    ], JSON_THROW_ON_ERROR);
    $expiredEnvelope = json_encode([
        'signed' => base64_encode($expiredPayload),
        'signature' => base64_encode(sodium_crypto_sign_detached($expiredPayload, sodium_crypto_sign_secretkey($pair))),
    ], JSON_THROW_ON_ERROR);
    rejects(fn (): array => Catalog::verify($expiredEnvelope, sodium_crypto_sign_publickey($pair), 1767225600), 'catalog parser rejects an expiration at the supplied clock time');
    $negativePayload = json_encode([
        'schema' => 1,
        'generation' => -1,
        'expires_at' => gmdate('c', time() + 3600),
        'releases' => [],
    ], JSON_THROW_ON_ERROR);
    $negativeEnvelope = json_encode([
        'signed' => base64_encode($negativePayload),
        'signature' => base64_encode(sodium_crypto_sign_detached($negativePayload, sodium_crypto_sign_secretkey($pair))),
    ], JSON_THROW_ON_ERROR);
    rejects(fn (): array => Catalog::verify($negativeEnvelope, sodium_crypto_sign_publickey($pair)), 'catalog parser rejects negative generations');
    $shortSignatureEnvelope = json_encode([
        'signed' => base64_encode($payload),
        'signature' => base64_encode('short'),
    ], JSON_THROW_ON_ERROR);
    rejects(fn (): array => Catalog::verify($shortSignatureEnvelope, sodium_crypto_sign_publickey($pair)), 'catalog parser rejects invalid signature lengths');
    $malformedPayload = '{';
    $malformedEnvelope = json_encode([
        'signed' => base64_encode($malformedPayload),
        'signature' => base64_encode(sodium_crypto_sign_detached($malformedPayload, sodium_crypto_sign_secretkey($pair))),
    ], JSON_THROW_ON_ERROR);
    rejects(fn (): array => Catalog::verify($malformedEnvelope, sodium_crypto_sign_publickey($pair)), 'catalog parser rejects signed malformed payloads');

    $state = new State($temporary.'/state');
    $state->write('status.json', ['state' => 'failed']);
    check(($state->read('status.json')['state'] ?? null) === 'failed', 'atomically persists updater state');
    file_put_contents($state->path('catalog.json'), '{');
    rejects(fn (): ?array => $state->read('catalog.json'), 'rejects partial updater state');
    rejects(fn (): string => $state->path('../escape.json'), 'rejects traversal state paths');

    $manifestRoot = $temporary.'/manifest';
    mkdir($manifestRoot, 0700, true);
    file_put_contents($manifestRoot.'/update.php', 'test');
    mkdir($manifestRoot.'/updater', 0700, true);
    file_put_contents($manifestRoot.'/updater/Updater.php', 'test');
    file_put_contents($manifestRoot.'/updater/ActivityLock.php', 'test');
    file_put_contents($manifestRoot.'/updater/PostgresBackup.php', 'test');
    mkdir($manifestRoot.'/docs', 0700, true);
    foreach (['LICENSE', 'README.md', 'SECURITY.md', 'docs/deployment.md'] as $path) {
        file_put_contents($manifestRoot.'/'.$path, 'test');
    }
    $manifestFiles = [
        'update.php' => hash_file('sha256', $manifestRoot.'/update.php'),
        'updater/Updater.php' => hash_file('sha256', $manifestRoot.'/updater/Updater.php'),
        'updater/ActivityLock.php' => hash_file('sha256', $manifestRoot.'/updater/ActivityLock.php'),
        'updater/PostgresBackup.php' => hash_file('sha256', $manifestRoot.'/updater/PostgresBackup.php'),
        'LICENSE' => hash_file('sha256', $manifestRoot.'/LICENSE'),
        'README.md' => hash_file('sha256', $manifestRoot.'/README.md'),
        'SECURITY.md' => hash_file('sha256', $manifestRoot.'/SECURITY.md'),
        'docs/deployment.md' => hash_file('sha256', $manifestRoot.'/docs/deployment.md'),
    ];
    file_put_contents($manifestRoot.'/package-files.json', json_encode(['schema' => 1, 'files' => $manifestFiles], JSON_THROW_ON_ERROR));
    Package::verify($manifestRoot, Package::manifest($manifestRoot));
    check(true, 'validates package manifests containing required legal and deployment files');
    $missingBackupHelper = Package::manifest($manifestRoot);
    unset($missingBackupHelper['files']['updater/PostgresBackup.php']);
    rejects(fn () => Package::verify($manifestRoot, $missingBackupHelper), 'rejects a package missing the PostgreSQL backup helper');
    file_put_contents($manifestRoot.'/package-files.json', json_encode(['schema' => 1, 'files' => ['../.env' => str_repeat('a', 64)]], JSON_THROW_ON_ERROR));
    rejects(fn (): array => Package::manifest($manifestRoot), 'rejects unsafe manifest paths');
    file_put_contents($manifestRoot.'/package-files.json', json_encode(['schema' => 1, 'files' => ['unexpected.md' => str_repeat('a', 64)]], JSON_THROW_ON_ERROR));
    rejects(fn (): array => Package::manifest($manifestRoot), 'rejects unapproved package-root files');

    $archive = $temporary.'/traversal.zip';
    $zip = new ZipArchive;
    $zip->open($archive, ZipArchive::CREATE);
    $zip->addFromString('filebeam/../escape.txt', 'bad');
    $zip->close();
    rejects(fn (): null => Package::extract($archive, $temporary.'/out', 1024), 'rejects traversal archives');
    $archive = $temporary.'/large.zip';
    $zip = new ZipArchive;
    $zip->open($archive, ZipArchive::CREATE);
    $zip->addFromString('filebeam/large.txt', str_repeat('x', 2048));
    $zip->close();
    rejects(fn (): null => Package::extract($archive, $temporary.'/out', 1024), 'enforces archive extraction limits');
} finally {
    removeDirectory($temporary);
}

fwrite($failures === 0 ? STDOUT : STDERR, sprintf("%d checks, %d failures\n", $checks, $failures));
exit($failures === 0 ? 0 : 1);
