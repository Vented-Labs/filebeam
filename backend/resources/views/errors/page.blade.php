@php
    $brandName = (string) config('filebeam.branding.name', 'Filebeam');
    $configuredLogoUrl = config('filebeam.branding.logo_url');
    $hasConfiguredLogo = is_string($configuredLogoUrl) && $configuredLogoUrl !== '';
    $hasCustomIdentity = $hasConfiguredLogo || $brandName !== 'Filebeam';
    $logoUrl = $hasConfiguredLogo
        ? $configuredLogoUrl
        : asset($hasCustomIdentity ? 'brand/filebeam-mark.svg' : 'brand/filebeam-logo-header.svg');
    $faviconUrl = config('filebeam.branding.favicon_url') ?: asset('favicon.svg');
@endphp
<!DOCTYPE html>
<html lang="{{ str_replace('_', '-', app()->getLocale()) }}">
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <meta name="theme-color" content="#0b0914">
        <meta name="robots" content="noindex">
        <link rel="icon" href="{{ $faviconUrl }}">
        <title>{{ $title }} - {{ $brandName }}</title>
        <style>
            :root {
                color-scheme: dark;
                font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
                color: #f7f5ff;
                background: #0b0914;
            }

            * { box-sizing: border-box; }

            body {
                min-width: 320px;
                min-height: 100svh;
                margin: 0;
                background:
                    radial-gradient(ellipse at 50% 15%, rgb(139 53 255 / 12%), transparent 58%),
                    #0b0914;
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
                border-bottom: 1px solid #352747;
            }

            .brand {
                display: inline-flex;
                align-items: center;
                gap: .58rem;
                color: inherit;
                text-decoration: none;
            }

            .brand img { display: block; object-fit: contain; }
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
                border: 1px solid #352747;
                border-radius: 999px;
                background: #26163f;
                overflow: hidden;
            }

            .signal::before, .signal::after {
                content: "";
                position: absolute;
                width: 6rem;
                height: 1px;
                background: linear-gradient(90deg, transparent, #c084fc, transparent);
                transform: rotate(-32deg);
            }

            .signal::after { transform: rotate(32deg); opacity: .45; }

            .status {
                position: relative;
                z-index: 1;
                padding: .34rem .55rem;
                border: 1px solid #746184;
                border-radius: .5rem;
                color: #f7f5ff;
                background: #131020;
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
                color: #aaa0c0;
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
                border: 1px solid #352747;
                border-radius: .75rem;
                color: #f7f5ff;
                background: #131020;
                font-size: .9rem;
                font-weight: 600;
                line-height: 1.2;
                text-decoration: none;
                transition: background-color 150ms ease, border-color 150ms ease;
            }

            .action:hover { border-color: #746184; background: #1b152b; }
            .action:focus-visible { outline: 3px solid #c084fc; outline-offset: 3px; }
            .action svg { width: 1rem; height: 1rem; }

            .footer {
                min-height: 4.5rem;
                display: flex;
                align-items: center;
                justify-content: center;
                border-top: 1px solid #352747;
                color: #aaa0c0;
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
                        class="{{ $hasCustomIdentity ? 'brand-glyph' : 'brand-lockup' }}"
                        src="{{ $logoUrl }}"
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
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <path d="M5 12h14M13 6l6 6-6 6" />
                        </svg>
                    </a>
                </section>
            </main>

            <footer class="footer">Error {{ $status }}</footer>
        </div>
    </body>
</html>
