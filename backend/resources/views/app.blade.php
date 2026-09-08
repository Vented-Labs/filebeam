<!DOCTYPE html>
<html lang="{{ str_replace('_', '-', app()->getLocale()) }}"  @class(['dark' => ($appearance ?? 'system') == 'dark'])>
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <meta name="application-name" content="{{ config('filebeam.branding.name') }}">

        @if ($faviconUrl = config('filebeam.branding.favicon_url'))
            <link rel="icon" href="{{ $faviconUrl }}">
        @else
            <link rel="icon" href="{{ asset('favicon.ico') }}" sizes="16x16 32x32 48x48 64x64">
            <link rel="icon" type="image/png" sizes="32x32" href="{{ asset('favicon-32x32.png') }}">
            <link rel="icon" type="image/png" sizes="16x16" href="{{ asset('favicon-16x16.png') }}">
            <link rel="icon" type="image/svg+xml" sizes="any" href="{{ asset('favicon.svg') }}">
            <link rel="apple-touch-icon" sizes="180x180" href="{{ asset('apple-touch-icon.png') }}">
            <meta name="theme-color" content="#0B0914">
        @endif

        @vite(['resources/css/app.css', 'resources/js/app.ts', "resources/js/pages/{$page['component']}.vue"])
        <x-inertia::head>
            <title>{{ config('filebeam.branding.name') }}</title>
        </x-inertia::head>
    </head>
    <body class="font-sans antialiased">
        <x-inertia::app />
    </body>
</html>
