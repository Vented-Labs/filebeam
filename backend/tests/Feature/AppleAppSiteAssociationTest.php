<?php

declare(strict_types=1);

use Illuminate\Http\Request;
use Illuminate\Support\Facades\Route;

/** @return array<string, mixed> */
function appleAssociationConfig(string $value): array
{
    $previous = getenv('FILEBEAM_IOS_APP_IDS');
    putenv('FILEBEAM_IOS_APP_IDS='.$value);
    $_ENV['FILEBEAM_IOS_APP_IDS'] = $value;
    $_SERVER['FILEBEAM_IOS_APP_IDS'] = $value;

    try {
        return require config_path('filebeam.php');
    } finally {
        putenv($previous === false ? 'FILEBEAM_IOS_APP_IDS' : 'FILEBEAM_IOS_APP_IDS='.$previous);
        if ($previous === false) {
            unset($_ENV['FILEBEAM_IOS_APP_IDS'], $_SERVER['FILEBEAM_IOS_APP_IDS']);
        } else {
            $_ENV['FILEBEAM_IOS_APP_IDS'] = $previous;
            $_SERVER['FILEBEAM_IOS_APP_IDS'] = $previous;
        }
    }
}

test('returns 404 rather than an empty association document while unconfigured', function (): void {
    config()->set('filebeam.apple.app_ids', []);

    $this->get('/.well-known/apple-app-site-association')->assertNotFound();
});

test('serves configured app identifiers and only explicit universal-link paths', function (): void {
    config()->set('filebeam.apple.app_ids', ['A1B2C3D4E5.io.filebeam.ios', 'A1B2C3D4E5.io.filebeam.ios.debug']);

    $response = $this->get('/.well-known/apple-app-site-association');

    $response->assertOk()
        ->assertHeader('Content-Type', 'application/json')
        ->assertHeaderMissing('Location')
        ->assertExactJson([
            'applinks' => [
                'details' => [
                    [
                        'appIDs' => ['A1B2C3D4E5.io.filebeam.ios'],
                        'components' => [
                            ['/' => '/??????????????????????????'],
                            ['/' => '/u/*'],
                            ['/' => '/invitations/*'],
                            ['/' => '/verify-email/*'],
                            ['/' => '/reset-password/*'],
                        ],
                    ],
                    [
                        'appIDs' => ['A1B2C3D4E5.io.filebeam.ios.debug'],
                        'components' => [
                            ['/' => '/??????????????????????????'],
                            ['/' => '/u/*'],
                            ['/' => '/invitations/*'],
                            ['/' => '/verify-email/*'],
                            ['/' => '/reset-password/*'],
                        ],
                    ],
                ],
            ],
        ]);

    expect($response->headers->get('Cache-Control'))->toContain('public')
        ->toContain('max-age=3600')
        ->toContain('s-maxage=3600')
        ->toContain('must-revalidate')
        ->and($response->json())->not->toHaveKey('webcredentials')
        ->and($response->getContent())->not->toContain('TEAM.');
});

test('requires a JSON or comma list of valid Apple app identifiers', function (string $value): void {
    expect(fn (): array => appleAssociationConfig($value))->toThrow(InvalidArgumentException::class);
})->with([
    'wrong Team ID length' => 'A1B2C3D4E.io.filebeam.ios',
    'lowercase Team ID' => 'a1B2C3D4E5.io.filebeam.ios',
    'bundle without dotted names' => 'A1B2C3D4E5.filebeam',
    'empty comma entry' => 'A1B2C3D4E5.io.filebeam.ios,',
    'JSON object' => '{"app":"A1B2C3D4E5.io.filebeam.ios"}',
]);

test('rejects malformed JSON app identifier lists', function (): void {
    expect(fn (): array => appleAssociationConfig('["A1B2C3D4E5.io.filebeam.ios"'))->toThrow(JsonException::class);
});

test('accepts JSON app identifiers without changing their case', function (): void {
    $configuration = appleAssociationConfig('["A1B2C3D4E5.io.Filebeam.iOS","A1B2C3D4E5.io.filebeam.ios.debug"]');

    expect($configuration['apple']['app_ids'])->toBe([
        'A1B2C3D4E5.io.Filebeam.iOS',
        'A1B2C3D4E5.io.filebeam.ios.debug',
    ]);
});

test('the explicit association route wins before the generic transfer route', function (): void {
    config()->set('filebeam.apple.app_ids', ['A1B2C3D4E5.io.filebeam.ios']);

    expect(Route::getRoutes()->match(Request::create('/.well-known/apple-app-site-association'))->getName())->toBe('apple-app-site-association');
    $this->get('/not-a-transfer')->assertNotFound()->assertDontSee('A1B2C3D4E5.io.filebeam.ios');
});
