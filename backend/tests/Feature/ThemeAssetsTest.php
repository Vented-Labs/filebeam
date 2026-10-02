<?php

declare(strict_types=1);

use App\Models\InstanceSetting;
use App\Models\User;
use App\Notifications\InboxTransferCompleted;
use App\Support\FeatureAvailability;
use App\Support\Theming\Assets;
use App\Support\Theming\BrandArtwork;
use App\Support\Theming\Palette;
use Illuminate\Support\Facades\File;

beforeEach(function () {
    config()->set('app.key', 'base64:'.base64_encode(str_repeat('t', 32)));
    config()->set('theme.storage_directory', storage_path('framework/testing/theme-assets-'.bin2hex(random_bytes(8))));
});

afterEach(function () {
    File::deleteDirectory(config('theme.storage_directory'));
});

test('signed theme images are generated cached and served without sessions', function (string $asset, string $type) {
    if (! extension_loaded('gd')) {
        $this->markTestSkipped('GD rendering is covered by the GD-enabled job.');
    }
    $assets = app(Assets::class);
    $url = $assets->url($asset, new Palette('#008877'));
    $response = $this->get($url)->assertOk()->assertHeader('content-type', $type);
    expect($response->headers->get('cache-control'))->toContain('public')->toContain('immutable');
    expect($response->headers->getCookies())->toBe([]);
    $this->get($url)->assertContent($response->getContent());
    $this->withHeader('If-None-Match', $response->headers->get('etag'))->get($url)->assertNotModified();
    if ($type === 'image/svg+xml') {
        expect($response->getContent())->toBe(BrandArtwork::render($asset, new Palette('#008877')))->not->toContain('#8B35FF');
    } elseif ($type === 'image/png') {
        $image = imagecreatefromstring($response->getContent());
        expect($image)->toBeInstanceOf(GdImage::class);
        expect(imagesx($image))->toBeGreaterThan(0);
    } else {
        expect(substr($response->getContent(), 0, 6))->toBe(pack('vvv', 0, 1, 4));
    }
})->with([
    ['logo.svg', 'image/svg+xml'],
    ['favicon-32.png', 'image/png'],
    ['email.png', 'image/png'],
    ['favicon.ico', 'image/vnd.microsoft.icon'],
]);

test('asset signatures prevent arbitrary palettes paths and rendering requests', function () {
    $url = app(Assets::class)->url('logo.svg', new Palette('#008877'));
    $this->get(str_replace('008877', 'ff0000', $url))->assertForbidden();
    $this->get(str_replace('logo.svg', 'unknown.svg', $url))->assertForbidden();
    $this->get('/_theme/'.str_repeat('1', 24).'/008877/dark/logo.svg')->assertForbidden();
});

test('GD availability controls both page colors and built-in image URLs', function () {
    InstanceSetting::query()->create(['key' => 'primary_color', 'value' => '#008877']);
    app()->instance(FeatureAvailability::class, new FeatureAvailability([]));
    $this->get('/')->assertOk()
        ->assertSee('data-primary="#8b35ff"', false)
        ->assertSee('href="'.asset('favicon.svg').'"', false);
    app()->instance(FeatureAvailability::class, new FeatureAvailability(['gd']));
    $this->get('/')->assertOk()->assertSee('data-primary="#008877"', false)->assertSee('/_theme/');
});

test('email colors and logos use the latest instance palette across renders', function () {
    app()->instance(FeatureAvailability::class, new FeatureAvailability(['gd']));
    $user = User::factory()->create();
    $render = fn (): string => (string) (new InboxTransferCompleted)->toMail($user)->render();
    expect($render())->toContain('background-color: #7c3aed');
    InstanceSetting::query()->create(['key' => 'primary_color', 'value' => '#008877']);
    $green = $render();
    expect($green)->not->toContain('background-color: #7c3aed')->toContain('/008877/dark/email.png');
    InstanceSetting::query()->findOrFail('primary_color')->update(['value' => '#ff6600']);
    expect($render())->not->toBe($green)->toContain('/ff6600/dark/email.png');
});

test('a missing optional renderer cannot cache fallback bytes under a custom image URL', function () {
    app()->instance(FeatureAvailability::class, new FeatureAvailability([]));
    $url = app(Assets::class)->url('email.png', new Palette('#008877'));
    $response = $this->getJson($url)->assertServiceUnavailable();
    expect($response->headers->get('cache-control'))->not->toContain('immutable');
});
