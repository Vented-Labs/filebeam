@props(['database' => true, 'chrome' => '--fb-browser-chrome'])
@php
    $palette = app(\App\Support\Theming\Theme::class)->palette($database);
    $darkChrome = $palette->tokens()[$chrome];
    $lightChrome = $palette->tokens('light')[$chrome];
@endphp
<meta name="theme-color" content="{{ $darkChrome }}" data-fb-dark="{{ $darkChrome }}" data-fb-light="{{ $lightChrome }}">
<style id="filebeam-theme" data-primary="{{ $palette->primary }}">{!! $palette->css() !!}</style>
<script nonce="{{ \Illuminate\Support\Facades\Vite::cspNonce() }}">{!! file_get_contents(resource_path('themes/appearance.js')) !!}</script>
