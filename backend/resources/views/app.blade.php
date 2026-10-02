<!DOCTYPE html>
<html lang="{{ str_replace('_', '-', app()->getLocale()) }}" data-fb-preset="{{ app(\App\Support\Theming\Appearance::class)->preset(request(), ! request()->is('install', 'install/*')) }}" @class(['dark' => ($appearance ?? 'system') == 'dark'])>
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <meta name="application-name" content="{{ config('filebeam.branding.name') }}">

        @php
            $socialPreview = app(\App\Support\SocialPreview::class)->for(request());
        @endphp
        @if ($socialPreview)
            <meta property="og:type" content="website">
            <meta property="og:site_name" content="{{ config('filebeam.branding.name') }}">
            <meta property="og:title" content="{{ $socialPreview['title'] }}">
            <meta property="og:description" content="{{ $socialPreview['description'] }}">
            <meta property="og:url" content="{{ $socialPreview['url'] }}">
            @if ($socialPreview['image_url'])
                <meta property="og:image" content="{{ $socialPreview['image_url'] }}">
                <meta property="og:image:width" content="1200">
                <meta property="og:image:height" content="630">
                <meta property="og:image:type" content="image/png">
                <meta property="og:image:alt" content="{{ $socialPreview['title'] }}">
            @endif
            <meta name="description" content="{{ $socialPreview['description'] }}">
            @if ($socialPreview['image_url'])
                <meta name="twitter:card" content="summary_large_image">
                <meta name="twitter:title" content="{{ $socialPreview['title'] }}">
                <meta name="twitter:description" content="{{ $socialPreview['description'] }}">
                <meta name="twitter:image" content="{{ $socialPreview['image_url'] }}">
                <meta name="twitter:image:alt" content="{{ $socialPreview['title'] }}">
            @endif
        @endif

        @php
            $themeAssets = app(\App\Support\Theming\Assets::class);
            $themePalette = app(\App\Support\Theming\Appearance::class)->palette(request(), ! request()->is('install', 'install/*'));
        @endphp
        @if ($faviconUrl = config('filebeam.branding.favicon_url'))
            <link rel="icon" href="{{ $faviconUrl }}">
        @else
            <link data-fb-favicon="favicon.ico" rel="icon" href="{{ $themeAssets->url('favicon.ico', $themePalette) }}" sizes="16x16 32x32 48x48 64x64">
            <link data-fb-favicon="favicon-32.png" rel="icon" type="image/png" sizes="32x32" href="{{ $themeAssets->url('favicon-32.png', $themePalette) }}">
            <link data-fb-favicon="favicon-16.png" rel="icon" type="image/png" sizes="16x16" href="{{ $themeAssets->url('favicon-16.png', $themePalette) }}">
            <link data-fb-favicon="favicon.svg" rel="icon" type="image/svg+xml" sizes="any" href="{{ $themeAssets->url('favicon.svg', $themePalette) }}">
            <link data-fb-favicon="apple-touch-icon.png" rel="apple-touch-icon" sizes="180x180" href="{{ $themeAssets->url('apple-touch-icon.png', $themePalette) }}">
        @endif

        @vite(['resources/css/app.css', 'resources/js/app.ts', "resources/js/pages/{$page['component']}.vue"])
        <x-theme-head :database="! request()->is('install', 'install/*')" :personal="true" />
        <x-inertia::head>
            <title>{{ config('filebeam.branding.name') }}</title>
        </x-inertia::head>
    </head>
    <body class="font-sans antialiased">
        <x-inertia::app />
    </body>
</html>
