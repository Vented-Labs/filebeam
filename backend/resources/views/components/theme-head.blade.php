@props(['database' => true, 'chrome' => '--fb-browser-chrome', 'personal' => false])
@php
    $palette = app(\App\Support\Theming\Theme::class)->palette($database);
    $darkChrome = $palette->tokens()[$chrome];
    $lightChrome = $palette->tokens('light')[$chrome];
    $appearance = $personal ? app(\App\Support\Theming\Appearance::class)->payload(request(), $database) : null;
@endphp
<meta name="theme-color" content="{{ $darkChrome }}" data-fb-dark="{{ $darkChrome }}" data-fb-light="{{ $lightChrome }}">
<style id="filebeam-theme" data-primary="{{ $palette->primary }}">{!! $palette->css() !!}</style>
@if ($appearance)
    <link id="filebeam-presets" rel="stylesheet" href="{{ $appearance['styles_url'] }}">
    <script id="filebeam-appearance" type="application/json" nonce="{{ \Illuminate\Support\Facades\Vite::cspNonce() }}">{!! json_encode($appearance, JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT | JSON_THROW_ON_ERROR) !!}</script>
@endif
<script nonce="{{ \Illuminate\Support\Facades\Vite::cspNonce() }}">{!! file_get_contents(resource_path('themes/appearance.js')) !!}</script>
