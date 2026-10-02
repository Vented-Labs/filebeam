@props(['dark' => false])

@php
    $brandName = (string) config('filebeam.branding.name', 'Filebeam');
    $configuredLogoUrl = config('filebeam.branding.logo_url');
    $hasConfiguredLogo = is_string($configuredLogoUrl) && $configuredLogoUrl !== '';
    $hasCustomIdentity = $hasConfiguredLogo || $brandName !== 'Filebeam';
    $themeAssets = app(\App\Support\Theming\Assets::class);
    $logoUrl = $hasConfiguredLogo
        ? $configuredLogoUrl
        : $themeAssets->url($hasCustomIdentity ? 'mark.svg' : 'logo.svg', mode: $dark ? 'dark' : 'light');
@endphp

<span class="fb-admin-brand">
    <img
        class="{{ $hasCustomIdentity ? 'fb-admin-brand__mark' : 'fb-admin-brand__lockup' }}"
        src="{{ $logoUrl }}"
        alt="{{ $hasCustomIdentity ? '' : $brandName }}"
    >

    @if (! $hasCustomIdentity)
        <img
            class="fb-admin-brand__collapsed-mark"
            src="{{ $themeAssets->url('mark.svg') }}"
            alt=""
        >
    @else
        <strong class="fb-admin-brand__name">{{ $brandName }}</strong>
    @endif
</span>
