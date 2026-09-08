<?php

declare(strict_types=1);

namespace App\Providers\Filament;

use App\Filament\Pages\AdminLogin;
use App\Filament\Pages\Dashboard;
use App\Filament\Pages\StaffProfile;
use Filament\Auth\MultiFactor\App\AppAuthentication;
use Filament\Enums\ThemeMode;
use Filament\Http\Middleware\Authenticate;
use Filament\Http\Middleware\AuthenticateSession;
use Filament\Http\Middleware\DisableBladeIconComponents;
use Filament\Http\Middleware\DispatchServingFilamentEvent;
use Filament\Panel;
use Filament\PanelProvider;
use Filament\View\PanelsRenderHook;
use Illuminate\Cookie\Middleware\AddQueuedCookiesToResponse;
use Illuminate\Cookie\Middleware\EncryptCookies;
use Illuminate\Foundation\Http\Middleware\PreventRequestForgery;
use Illuminate\Routing\Middleware\SubstituteBindings;
use Illuminate\Session\Middleware\StartSession;
use Illuminate\Support\Facades\Blade;
use Illuminate\View\Middleware\ShareErrorsFromSession;

class AdminPanelProvider extends PanelProvider
{
    public function panel(Panel $panel): Panel
    {
        return $panel
            ->default()
            ->id('admin')
            ->path('admin')
            ->spa()
            ->brandName(fn (): string => config('filebeam.branding.name').' Admin')
            ->brandLogo(fn () => view('components.brand.admin-logo'))
            ->darkModeBrandLogo(fn () => view('components.brand.admin-logo', ['dark' => true]))
            ->brandLogoHeight('2rem')
            ->favicon(fn (): string => config('filebeam.branding.favicon_url') ?: asset('favicon.svg'))
            ->viteTheme('resources/css/filament/admin/theme.css')
            ->darkMode()
            ->themeSwitcher()
            ->defaultThemeMode(ThemeMode::Dark)
            ->authGuard('admin')
            ->navigationGroups(['Work', 'Administration'])
            ->sidebarCollapsibleOnDesktop()
            ->unsavedChangesAlerts()
            ->login(AdminLogin::class)
            ->profile(StaffProfile::class)
            ->multiFactorAuthentication([
                AppAuthentication::make()->recoverable()->brandName(config('filebeam.branding.name').' Admin'),
            ])
            ->colors([
                'primary' => '#7c3aed',
            ])
            ->discoverResources(in: app_path('Filament/Resources'), for: 'App\Filament\Resources')
            ->discoverPages(in: app_path('Filament/Pages'), for: 'App\Filament\Pages')
            ->pages([
                Dashboard::class,
            ])
            ->discoverWidgets(in: app_path('Filament/Widgets'), for: 'App\Filament\Widgets')
            ->renderHook(
                PanelsRenderHook::CONTENT_START,
                fn (): string => Blade::render('<livewire:scheduler-heartbeat-alert />'),
            )
            ->middleware([
                EncryptCookies::class,
                AddQueuedCookiesToResponse::class,
                StartSession::class,
                AuthenticateSession::class,
                ShareErrorsFromSession::class,
                PreventRequestForgery::class,
                SubstituteBindings::class,
                DisableBladeIconComponents::class,
                DispatchServingFilamentEvent::class,
            ])
            ->authMiddleware([
                Authenticate::class,
            ], isPersistent: true);
    }
}
