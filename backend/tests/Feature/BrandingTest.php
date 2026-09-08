<?php

declare(strict_types=1);

use App\Models\Plan;
use Filament\Enums\ThemeMode;
use Filament\Facades\Filament;
use Inertia\Testing\AssertableInertia as Assert;

test('shares default branding artwork separately from an unset logo override', function () {
    Plan::factory()->create(['slug' => 'default']);

    $this->get('/')
        ->assertOk()
        ->assertInertia(fn (Assert $page) => $page->component('Welcome')
            ->where('branding.logo_url', null)
            ->where('branding.default_logo_url', asset('brand/filebeam-logo-header.svg'))
            ->where('branding.default_mark_url', asset('brand/filebeam-mark.svg')));
});

test('shares public branding configuration without changing retention fields', function () {
    Plan::factory()->create(['slug' => 'default']);
    config()->set('filebeam.branding', [
        'name' => 'Acme Share',
        'logo_url' => '/images/acme-logo.svg',
        'favicon_url' => '/images/acme-favicon.svg',
        'default_logo_url' => asset('brand/filebeam-logo-header.svg'),
        'default_mark_url' => asset('brand/filebeam-mark.svg'),
        'version' => '2.4.0',
        'copyright_holder' => 'Acme, Inc.',
        'copyright_year' => 2030,
        'github_url' => 'https://github.com/acme/share',
    ]);

    $this->get('/')
        ->assertOk()
        ->assertInertia(fn (Assert $page) => $page->component('Welcome')
            ->where('branding.name', 'Acme Share')
            ->where('branding.logo_url', '/images/acme-logo.svg')
            ->where('branding.favicon_url', '/images/acme-favicon.svg')
            ->where('branding.default_logo_url', asset('brand/filebeam-logo-header.svg'))
            ->where('branding.default_mark_url', asset('brand/filebeam-mark.svg'))
            ->where('branding.version', '2.4.0')
            ->where('branding.copyright_holder', 'Acme, Inc.')
            ->where('branding.copyright_year', 2030)
            ->where('branding.github_url', 'https://github.com/acme/share')
            ->where('filebeam.file_retention_hours', 24)
            ->where('filebeam.note_retention_hours', 720));
});

test('uses the default favicon assets and theme color in the application shell', function () {
    config()->set('filebeam.branding.favicon_url', null);

    $this->get('/')
        ->assertOk()
        ->assertSee('href="'.asset('favicon.ico').'"', false)
        ->assertSee('href="'.asset('favicon-32x32.png').'"', false)
        ->assertSee('href="'.asset('favicon-16x16.png').'"', false)
        ->assertSee('href="'.asset('favicon.svg').'"', false)
        ->assertSee('href="'.asset('apple-touch-icon.png').'"', false)
        ->assertSee('<meta name="theme-color" content="#0B0914">', false);
});

test('uses only the configured favicon and brand name in the application shell', function () {
    config()->set('filebeam.branding.favicon_url', '/images/acme-favicon.svg');
    config()->set('filebeam.branding.name', 'Acme Share');

    $this->get('/')
        ->assertOk()
        ->assertSee('href="/images/acme-favicon.svg"', false)
        ->assertDontSee('favicon-32x32.png', false)
        ->assertDontSee('apple-touch-icon.png', false)
        ->assertSee('<title>Acme Share</title>', false);
});

test('defaults the admin panel to dark while retaining persisted light and dark choices', function () {
    config()->set('filebeam.branding.favicon_url', null);
    $this->withoutVite();

    $panel = Filament::getPanel('admin');

    expect($panel->hasDarkMode())->toBeTrue()
        ->and($panel->hasDarkModeForced())->toBeFalse()
        ->and($panel->hasThemeSwitcher())->toBeTrue()
        ->and($panel->getDefaultThemeMode())->toBe(ThemeMode::Dark)
        ->and($panel->getViteTheme())->toBe('resources/css/filament/admin/theme.css')
        ->and($panel->getFavicon())->toBe(asset('favicon.svg'));

    $this->get('/admin/login')
        ->assertOk()
        ->assertSee('class="fi"', false)
        ->assertSee("localStorage.getItem('theme')", false)
        ->assertSee("?? 'dark'", false)
        ->assertSee('brand/filebeam-logo-header-on-light.svg', false)
        ->assertSee('brand/filebeam-logo-header.svg', false)
        ->assertSee('Filebeam Admin', false);
});

test('uses the configured admin logo or a mark with the configured name', function () {
    config()->set('filebeam.branding.name', 'Acme Share');
    config()->set('filebeam.branding.logo_url', '/images/acme-logo.svg');
    $this->withoutVite();

    $this->get('/admin/login')
        ->assertOk()
        ->assertSee('src="/images/acme-logo.svg"', false)
        ->assertSee('Acme Share', false)
        ->assertSee('Acme Share Admin', false);

    config()->set('filebeam.branding.logo_url', null);

    $this->get('/admin/login')
        ->assertOk()
        ->assertSee('brand/filebeam-mark.svg', false)
        ->assertSee('Acme Share', false);
});
