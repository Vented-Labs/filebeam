@props(['dark' => false])

@php
    $brandName = (string) config('filebeam.branding.name', 'Filebeam');
    $configuredLogoUrl = config('filebeam.branding.logo_url');
    $hasConfiguredLogo = is_string($configuredLogoUrl) && $configuredLogoUrl !== '';
    $hasCustomIdentity = $hasConfiguredLogo || $brandName !== 'Filebeam';
    $logoUrl = $hasConfiguredLogo
        ? $configuredLogoUrl
        : asset($hasCustomIdentity
            ? 'brand/filebeam-mark.svg'
            : ($dark ? 'brand/filebeam-logo-header.svg' : 'brand/filebeam-logo-header-on-light.svg'));
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
            src="{{ asset('brand/filebeam-mark.svg') }}"
            alt=""
        >
    @else
        <strong class="fb-admin-brand__name">{{ $brandName }}</strong>
    @endif
</span>
