<?php

declare(strict_types=1);

use Illuminate\Support\Facades\Http;

/** @return array<string, mixed> */
function desktopReleaseFixture(string $tag = 'v1.2.3'): array
{
    $names = [
        "filebeam-desktop-$tag-linux-x86_64.AppImage",
        "filebeam-desktop-$tag-linux-aarch64.AppImage",
        "filebeam-desktop-$tag-macos-x86_64.dmg",
        "filebeam-desktop-$tag-macos-aarch64.dmg",
        "filebeam-desktop-$tag-windows-x86_64-setup.exe",
        "filebeam-desktop-$tag-windows-x86_64.zip",
        "filebeam-desktop-$tag-linux-x86_64.tar.gz",
    ];

    return [
        'tag_name' => $tag,
        'draft' => false,
        'prerelease' => false,
        'assets' => array_map(fn (string $name): array => [
            'name' => $name,
            'browser_download_url' => "https://github.com/Vented-Labs/filebeam/releases/download/$tag/$name",
            'state' => 'uploaded',
            'size' => 12345678,
        ], $names),
    ];
}

test('public desktop releases select the newest stable native installers and cache the lookup', function (): void {
    Http::preventStrayRequests();
    Http::fake(['api.github.com/repos/Vented-Labs/filebeam/releases*' => Http::response([
        desktopReleaseFixture('v1.2.3'),
        desktopReleaseFixture('v1.10.0'),
        [...desktopReleaseFixture('v2.0.0'), 'prerelease' => true],
        [...desktopReleaseFixture('v3.0.0'), 'draft' => true],
        [...desktopReleaseFixture('v4.0.0'), 'assets' => []],
    ])]);

    $response = $this->getJson('/api/v1/desktop/releases')->assertOk()
        ->assertJsonPath('state', 'available')
        ->assertJsonPath('release.version', '1.10.0')
        ->assertJsonPath('release.notes_url', 'https://github.com/Vented-Labs/filebeam/releases/tag/v1.10.0')
        ->assertJsonCount(5, 'release.assets')
        ->assertJsonPath('release.assets.4.format', 'exe')
        ->assertJsonPath('release.assets.4.url', 'https://github.com/Vented-Labs/filebeam/releases/download/v1.10.0/filebeam-desktop-v1.10.0-windows-x86_64-setup.exe');
    expect(array_column($response->json('release.assets'), 'format'))->toBe(['AppImage', 'AppImage', 'dmg', 'dmg', 'exe']);
    $this->getJson('/api/v1/desktop/releases')->assertOk();
    Http::assertSentCount(1);
    $this->travel(301)->seconds();
    $this->getJson('/api/v1/desktop/releases')->assertOk();
    Http::assertSentCount(2);
});

test('desktop downloads omit missing, incomplete and untrusted assets', function (): void {
    $release = desktopReleaseFixture();
    $release['assets'][0]['browser_download_url'] = 'https://unrelated.test/app.AppImage';
    $release['assets'][1]['size'] = 0;
    $release['assets'][2]['state'] = 'new';
    unset($release['assets'][4]);
    Http::fake(['api.github.com/*' => Http::response([$release])]);

    $this->getJson('/api/v1/desktop/releases')->assertOk()
        ->assertJsonCount(1, 'release.assets')
        ->assertJsonPath('release.assets.0.os', 'macos')
        ->assertJsonPath('release.assets.0.architecture', 'aarch64');
});

test('desktop downloads report unavailable when only updater archives exist', function (): void {
    $release = desktopReleaseFixture();
    $release['assets'] = array_slice($release['assets'], 5);
    Http::fake(['api.github.com/*' => Http::response([$release])]);

    $this->getJson('/api/v1/desktop/releases')->assertOk()
        ->assertExactJson(['state' => 'unavailable', 'release' => null]);
});

test('upstream errors are briefly cached and recover on retry', function (): void {
    Http::fake(['api.github.com/*' => Http::sequence()->pushStatus(403)->push([desktopReleaseFixture()])]);

    $this->getJson('/api/v1/desktop/releases')->assertStatus(503)
        ->assertHeader('Retry-After', '30')
        ->assertExactJson(['state' => 'error', 'release' => null]);
    $this->getJson('/api/v1/desktop/releases')->assertStatus(503);
    Http::assertSentCount(1);
    $this->travel(31)->seconds();
    $this->getJson('/api/v1/desktop/releases')->assertOk()->assertJsonPath('state', 'available');
});

test('invalid upstream responses and connection failures return a retryable result', function (bool $connectionFailure): void {
    Http::fake(['api.github.com/*' => $connectionFailure ? Http::failedConnection() : Http::response('not json')]);

    $this->getJson('/api/v1/desktop/releases')->assertStatus(503)
        ->assertExactJson(['state' => 'error', 'release' => null]);
})->with([true, false]);
