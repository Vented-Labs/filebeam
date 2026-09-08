<?php

declare(strict_types=1);

use App\Models\AccountKeyBundle;
use App\Models\Plan;
use App\Models\Transfer;
use App\Models\User;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Route;

beforeEach(function (): void {
    $this->withoutVite();
    Plan::factory()->create(['slug' => 'default']);
    config()->set('social.manifest_path', socialManifest(['home' => 'home-abc.png', 'receive' => 'receive-def.png', 'transfer' => 'transfer-ghi.png']));
});

afterEach(function (): void {
    foreach (socialFixtureDirectories() as $directory) {
        foreach (glob($directory.'/*') ?: [] as $path) {
            @unlink($path);
        }

        @rmdir($directory);
    }
});

/** @param array<string, string> $cards */
function socialManifest(array $cards): string
{
    return socialFixture(json_encode($cards, JSON_THROW_ON_ERROR), array_values($cards));
}

/** @param list<string> $images */
function socialFixture(string $manifest, array $images = []): string
{
    $directory = tempnam(sys_get_temp_dir(), 'filebeam-og-');

    if ($directory === false || ! unlink($directory) || ! mkdir($directory)) {
        throw new RuntimeException('Unable to create social preview fixture directory.');
    }

    socialFixtureDirectories($directory);
    $path = $directory.'/manifest.json';
    file_put_contents($path, $manifest);

    foreach ($images as $image) {
        file_put_contents($directory.'/'.$image, 'PNG');
    }

    return $path;
}

/** @return list<string> */
function socialFixtureDirectories(?string $directory = null): array
{
    static $directories = [];

    if ($directory !== null) {
        $directories[] = $directory;
    }

    return $directories;
}

function socialRecipient(): User
{
    $user = User::factory()->create(['username' => 'receiver', 'normalized_username' => 'receiver', 'inbox_enabled' => true]);
    AccountKeyBundle::factory()->for($user)->create();

    return $user;
}

test('supported public pages render their matching generic social cards', function (): void {
    socialRecipient();

    $home = $this->get('/')
        ->assertOk()
        ->assertSee('<meta property="og:title" content="Filebeam | Secure file sharing">', false)
        ->assertSee('<meta property="og:image" content="'.asset('build/og/home-abc.png').'">', false);
    $this->get('/u/receiver')
        ->assertOk()
        ->assertSee('<meta property="og:title" content="Send files securely with Filebeam">', false)
        ->assertSee('<meta property="og:image" content="'.asset('build/og/receive-def.png').'">', false);
    $this->get('/0'.str_repeat('A', 25))
        ->assertOk()
        ->assertSee('<meta property="og:title" content="Receive files securely with Filebeam">', false)
        ->assertSee('<meta property="og:image" content="'.asset('build/og/transfer-ghi.png').'">', false);

    expect(substr_count($home->getContent(), '<meta property="og:type"'))->toBe(1)
        ->and(substr_count($home->getContent(), '<meta property="og:image"'))->toBe(1)
        ->and(substr_count($home->getContent(), '<meta name="twitter:card"'))->toBe(1);
});

test('username domain receiving pages render the receive card with their own clean URL', function (): void {
    socialRecipient();
    config()->set('filebeam.username_domain', 'receive.example.test');
    Route::middleware('web')->group(base_path('routes/web.php'));

    $this->get('https://receive.example.test/receiver?source=social')
        ->assertOk()
        ->assertSee('<meta property="og:url" content="https://receive.example.test/receiver">', false)
        ->assertDontSee('<meta property="og:url" content="https://receive.example.test/receiver?source=social">', false)
        ->assertSee('<meta property="og:image" content="'.asset('build/og/receive-def.png').'">', false);
});

test('private and error pages have no social metadata', function (): void {
    $user = User::factory()->create();

    $this->get('/login')->assertOk()
        ->assertDontSee('<meta property="og:type"', false)
        ->assertDontSee('<meta name="description"', false);
    $this->actingAs($user)->get('/account/inbox')->assertOk()
        ->assertDontSee('<meta property="og:type"', false)
        ->assertDontSee('<meta name="description"', false);
    $this->actingAs($user)->get('/account/keys')->assertOk()
        ->assertDontSee('<meta property="og:type"', false)
        ->assertDontSee('<meta name="description"', false);
    $this->get('/not-a-public-page')->assertNotFound()
        ->assertDontSee('<meta property="og:type"', false)
        ->assertDontSee('<meta name="description"', false);
});

test('an operator image override is used without reading the manifest', function (): void {
    config()->set('social.image_url', 'https://cdn.example.test/filebeam.png');
    config()->set('social.manifest_path', sys_get_temp_dir().'/missing-filebeam-og-manifest.json');

    $this->get('/')
        ->assertOk()
        ->assertSee('<meta property="og:image" content="https://cdn.example.test/filebeam.png">', false)
        ->assertSee('<meta name="twitter:card" content="summary_large_image">', false);
});

test('a missing manifest omits image and Twitter metadata', function (): void {
    config()->set('social.manifest_path', sys_get_temp_dir().'/missing-filebeam-og-manifest.json');

    $this->get('/')
        ->assertOk()
        ->assertDontSee('<meta property="og:image"', false)
        ->assertDontSee('<meta name="twitter:card"', false);
});

test('malformed, traversal, and missing-image manifest entries omit image metadata', function (string $manifest, array $images): void {
    config()->set('social.manifest_path', socialFixture($manifest, $images));

    $this->get('/')
        ->assertOk()
        ->assertDontSee('<meta property="og:image"', false)
        ->assertDontSee('<meta name="twitter:card"', false);
})->with([
    'malformed JSON' => ['{', []],
    'traversal filename' => [json_encode(['home' => '../outside.png'], JSON_THROW_ON_ERROR), []],
    'missing referenced image' => [json_encode(['home' => 'home-missing.png'], JSON_THROW_ON_ERROR), []],
]);

test('invalid image overrides are ignored in favor of the manifest image', function (string $override): void {
    config()->set('social.image_url', $override);

    $this->get('/')
        ->assertOk()
        ->assertSee('<meta property="og:image" content="'.asset('build/og/home-abc.png').'">', false)
        ->assertDontSee('content="'.$override.'"', false);
})->with([
    'relative URL' => ['/build/og/override.png'],
    'unsupported scheme' => ['ftp://cdn.example.test/override.png'],
    'malformed URL' => ['https://'],
]);

test('complete social metadata is escaped and transfer pages do not query transfer data or change state', function (): void {
    config()->set('filebeam.branding.name', '<Unsafe & Brand>');
    $transferCount = Transfer::query()->count();
    $transferId = '0'.str_repeat('A', 25);
    $queries = [];
    DB::listen(function ($query) use (&$queries): void {
        $queries[] = $query->sql;
    });

    $response = $this->get('/'.$transferId)
        ->assertOk()
        ->assertSee('<meta property="og:site_name" content="&lt;Unsafe &amp; Brand&gt;">', false)
        ->assertSee('<meta property="og:title" content="Receive files securely with &lt;Unsafe &amp; Brand&gt;">', false)
        ->assertSee('<meta property="og:description" content="Receive an end-to-end encrypted file transfer with &lt;Unsafe &amp; Brand&gt;.">', false)
        ->assertSee('<meta property="og:url" content="'.url('/'.$transferId).'">', false)
        ->assertSee('<meta property="og:image:width" content="1200">', false)
        ->assertSee('<meta property="og:image:height" content="630">', false)
        ->assertSee('<meta property="og:image:type" content="image/png">', false)
        ->assertSee('<meta property="og:image:alt" content="Receive files securely with &lt;Unsafe &amp; Brand&gt;">', false)
        ->assertSee('<meta name="description" content="Receive an end-to-end encrypted file transfer with &lt;Unsafe &amp; Brand&gt;.">', false)
        ->assertSee('<meta name="twitter:title" content="Receive files securely with &lt;Unsafe &amp; Brand&gt;">', false)
        ->assertSee('<meta name="twitter:description" content="Receive an end-to-end encrypted file transfer with &lt;Unsafe &amp; Brand&gt;.">', false)
        ->assertSee('<meta name="twitter:image:alt" content="Receive files securely with &lt;Unsafe &amp; Brand&gt;">', false);

    foreach ([
        '<meta property="og:type"',
        '<meta property="og:site_name"',
        '<meta property="og:title"',
        '<meta property="og:description"',
        '<meta property="og:url"',
        '<meta property="og:image"',
        '<meta property="og:image:width"',
        '<meta property="og:image:height"',
        '<meta property="og:image:type"',
        '<meta property="og:image:alt"',
        '<meta name="description"',
        '<meta name="twitter:card"',
        '<meta name="twitter:title"',
        '<meta name="twitter:description"',
        '<meta name="twitter:image"',
        '<meta name="twitter:image:alt"',
    ] as $tag) {
        expect(substr_count($response->getContent(), $tag))->toBe(1);
    }

    $queriedTransfers = collect($queries)->contains(fn (string $sql): bool => str_contains(strtolower($sql), 'transfers'));

    expect($queriedTransfers)->toBeFalse()
        ->and(Transfer::query()->count())->toBe($transferCount);
});
