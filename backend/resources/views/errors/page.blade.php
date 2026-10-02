@php
    $brandName = (string) config('filebeam.branding.name', 'Filebeam');
    $configuredLogoUrl = config('filebeam.branding.logo_url');
    $hasConfiguredLogo = is_string($configuredLogoUrl) && $configuredLogoUrl !== '';
    $hasCustomIdentity = $hasConfiguredLogo || $brandName !== 'Filebeam';
    $themeAssets = app(\App\Support\Theming\Assets::class);
    $themePalette = app(\App\Support\Theming\Appearance::class)->palette(request(), false);
    $logoUrl = $hasConfiguredLogo
        ? $configuredLogoUrl
        : $themeAssets->url($hasCustomIdentity ? 'mark.svg' : 'logo.svg', $themePalette);
    $lightLogoUrl = $hasConfiguredLogo ? $configuredLogoUrl : $themeAssets->url($hasCustomIdentity ? 'mark.svg' : 'logo.svg', $themePalette, 'light');
    $faviconUrl = config('filebeam.branding.favicon_url') ?: $themeAssets->url('favicon.svg', $themePalette);
@endphp
<!DOCTYPE html>
<html lang="{{ str_replace('_', '-', app()->getLocale()) }}" data-fb-preset="{{ app(\App\Support\Theming\Appearance::class)->preset(request(), false) }}">
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <x-theme-head :database="false" chrome="--fb-bg" :personal="true" />
        <meta name="robots" content="noindex">
        <link rel="icon" href="{{ $faviconUrl }}">
        <title>{{ $title }} - {{ $brandName }}</title>
        <style>
            :root {
                color-scheme: var(--fb-color-scheme);
                font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
                color: var(--fb-error-text);
                background: var(--fb-bg);
            }

            * { box-sizing: border-box; }

            body {
                min-width: 320px;
                min-height: 100svh;
                margin: 0;
                background: var(--fb-bg);
            }

            .shell {
                min-height: 100svh;
                display: grid;
                grid-template-rows: auto 1fr auto;
            }

            .header, .footer {
                width: 100%;
                padding-inline: clamp(1rem, 4vw, 3rem);
            }

            .header {
                min-height: 5.35rem;
                display: flex;
                align-items: center;
                border-bottom: 1px solid var(--fb-line-strong);
            }

            .brand {
                display: inline-flex;
                align-items: center;
                gap: .58rem;
                color: inherit;
                text-decoration: none;
            }

            .brand img { display: block; object-fit: contain; }
            .brand .brand-logo--light { display: none; }
            :root[data-fb-theme="light"] .brand-logo--dark { display: none; }
            :root[data-fb-theme="light"] .brand-logo--light { display: block; }
            @media (prefers-color-scheme: light) {
                :root:not([data-fb-theme]) .brand-logo--dark { display: none; }
                :root:not([data-fb-theme]) .brand-logo--light { display: block; }
            }
            .brand-lockup { width: auto; height: 2rem; }
            .brand-glyph { width: 2.25rem; height: 2.25rem; }

            .brand-name {
                font-size: 1.52rem;
                font-weight: 700;
                letter-spacing: -.055em;
            }

            main {
                display: grid;
                place-items: center;
                width: 100%;
                padding: clamp(3rem, 10vh, 7rem) 1rem;
            }

            .error {
                width: min(100%, 38rem);
                text-align: center;
            }

            .signal {
                position: relative;
                width: 7rem;
                height: 7rem;
                display: grid;
                place-items: center;
                margin: 0 auto 2rem;
                border: 1px solid var(--fb-share-border);
                border-radius: 999px;
                background: var(--fb-error-surface);
                overflow: hidden;
            }

            .signal::before, .signal::after {
                content: "";
                position: absolute;
                width: 6rem;
                height: 1px;
                background: linear-gradient(90deg, transparent, var(--fb-error-signal), transparent);
                transform: rotate(-32deg);
            }

            .signal::after { transform: rotate(32deg); opacity: .45; }

            .status {
                position: relative;
                z-index: 1;
                padding: .34rem .55rem;
                border: 1px solid var(--fb-danger-border);
                border-radius: .5rem;
                color: var(--fb-danger);
                background: var(--fb-danger-surface);
                font-family: "JetBrains Mono", ui-monospace, monospace;
                font-size: .875rem;
                font-weight: 600;
                letter-spacing: .08em;
            }

            h1 {
                margin: 0;
                font-size: clamp(2rem, 7vw, 3.5rem);
                line-height: 1.05;
                letter-spacing: -.045em;
            }

            .description {
                max-width: 34rem;
                margin: 1rem auto 0;
                color: var(--fb-error-muted);
                font-size: 1rem;
                line-height: 1.65;
            }

            .action {
                min-height: 2.75rem;
                display: inline-flex;
                align-items: center;
                justify-content: center;
                gap: .55rem;
                margin-top: 2rem;
                padding: .625rem 1rem;
                border: 1px solid var(--fb-control-border);
                border-radius: .75rem;
                color: var(--fb-error-text);
                background: var(--fb-error-surface);
                font-size: .9rem;
                font-weight: 600;
                line-height: 1.2;
                text-decoration: none;
                transition: background-color 150ms ease, border-color 150ms ease;
            }

            .action:hover { border-color: var(--fb-error-hover-border); background: var(--fb-error-hover-surface); }
            .action:focus-visible { outline: 2px solid var(--fb-focus); outline-offset: 3px; }
            .action svg { width: 1rem; height: 1rem; }

            .footer {
                min-height: 4.5rem;
                display: flex;
                align-items: center;
                justify-content: center;
                border-top: 1px solid var(--fb-line-strong);
                color: var(--fb-error-muted);
                font-size: .8125rem;
            }

            @media (prefers-reduced-motion: reduce) {
                .action { transition: none; }
            }
        </style>
    </head>
    <body>
        <div class="shell">
            <header class="header">
                <a class="brand" href="{{ url('/') }}" aria-label="{{ $brandName }} home">
                    <img
                        class="{{ $hasCustomIdentity ? 'brand-glyph' : 'brand-lockup' }} brand-logo--dark"
                        src="{{ $logoUrl }}"
                        data-fb-brand="{{ $hasConfiguredLogo ? '' : ($hasCustomIdentity ? 'default_mark_url' : 'default_logo_url') }}"
                        alt="{{ $hasCustomIdentity ? '' : $brandName }}"
                    >
                    <img
                        class="{{ $hasCustomIdentity ? 'brand-glyph' : 'brand-lockup' }} brand-logo--light"
                        src="{{ $lightLogoUrl }}"
                        data-fb-brand="{{ $hasConfiguredLogo ? '' : ($hasCustomIdentity ? 'default_mark_url' : 'default_light_logo_url') }}"
                        alt="{{ $hasCustomIdentity ? '' : $brandName }}"
                    >
                    @if ($hasCustomIdentity)
                        <strong class="brand-name">{{ $brandName }}</strong>
                    @endif
                </a>
            </header>

            <main>
                <section class="error" aria-labelledby="error-title">
                    <div class="signal" aria-hidden="true">
                        <span class="status">{{ $status }}</span>
                    </div>
                    <h1 id="error-title">{{ $title }}</h1>
                    <p class="description">{{ $description }}</p>
                    <a class="action" href="{{ url('/') }}">
                        {{ $actionLabel ?? 'Return home' }}
                        <x-filebeam-icon name="arrow-right" :size="24" />
                    </a>
                </section>
            </main>

            <footer class="footer">Error {{ $status }}</footer>
        </div>
    </body>
</html>
