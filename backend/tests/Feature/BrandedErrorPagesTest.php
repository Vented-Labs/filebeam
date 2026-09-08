<?php

declare(strict_types=1);

use Illuminate\Support\Facades\Route;

beforeEach(function () {
    config()->set('app.debug', false);
});

test('renders branded pages for common browser errors', function (int $status, string $title) {
    Route::middleware('web')->get('/test-error', fn () => abort($status));

    $this->get('/test-error')
        ->assertStatus($status)
        ->assertHeader('content-type', 'text/html; charset=UTF-8')
        ->assertSee($title)
        ->assertSee('Error '.$status)
        ->assertSee('Filebeam');
})->with([
    'forbidden' => [403, 'You do not have access'],
    'not found' => [404, 'Page not found'],
    'expired session' => [419, 'Your session has expired'],
    'rate limited' => [429, 'Too many requests'],
    'server error' => [500, 'Something went wrong'],
    'unavailable' => [503, 'Temporarily unavailable'],
]);

test('renders a branded fallback for other HTTP errors', function () {
    Route::middleware('web')->get('/test-error', fn () => abort(418));

    $this->get('/test-error')
        ->assertStatus(418)
        ->assertSee('Request could not be completed')
        ->assertSee('Error 418');
});

test('uses configured branding without requiring the application shell', function () {
    config()->set('filebeam.branding.name', 'Acme Share');
    config()->set('filebeam.branding.logo_url', '/images/acme-logo.svg');

    $this->get('/missing-page')
        ->assertNotFound()
        ->assertSee('<title>Page not found - Acme Share</title>', false)
        ->assertSee('src="/images/acme-logo.svg"', false)
        ->assertSee('Acme Share')
        ->assertDontSee('@vite');
});

test('returns the error component for failed Inertia navigation', function () {
    $this->withHeaders([
        'Accept' => 'text/html, application/xhtml+xml',
        'X-Inertia' => 'true',
        'X-Requested-With' => 'XMLHttpRequest',
    ])->get('/missing-inertia-page')
        ->assertNotFound()
        ->assertHeader('X-Inertia', 'true')
        ->assertJsonPath('component', 'Error')
        ->assertJsonPath('props.status', 404)
        ->assertJsonPath('props.branding.name', 'Filebeam');
});

test('preserves retry timing for an Inertia error response', function () {
    Route::get('/test-error', fn () => abort(429, '', ['Retry-After' => '30']));

    $this->withHeaders([
        'Accept' => 'text/html, application/xhtml+xml',
        'X-Inertia' => 'true',
        'X-Requested-With' => 'XMLHttpRequest',
    ])->get('/test-error')
        ->assertTooManyRequests()
        ->assertHeader('Retry-After', '30')
        ->assertJsonPath('component', 'Error')
        ->assertJsonPath('props.status', 429);
});

test('keeps API errors machine readable', function () {
    $this->getJson('/api/v1/missing-endpoint')
        ->assertNotFound()
        ->assertHeader('content-type', 'application/json')
        ->assertJsonPath('message', 'The route api/v1/missing-endpoint could not be found.')
        ->assertDontSee('<!DOCTYPE html>', false);
});

test('renders the branded maintenance page', function () {
    $this->artisan('down')->assertSuccessful();

    try {
        $this->get('/')
            ->assertServiceUnavailable()
            ->assertSee('Temporarily unavailable')
            ->assertSee('Error 503');
    } finally {
        $this->artisan('up')->assertSuccessful();
    }
});
