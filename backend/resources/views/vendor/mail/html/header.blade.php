@props(['url'])
@php
    $brandName = (string) config('filebeam.branding.name');
    $configuredLogo = config('filebeam.branding.logo_url');
    $configuredPath = is_string($configuredLogo) ? parse_url($configuredLogo, PHP_URL_PATH) : null;
    $configuredExtension = is_string($configuredPath) ? strtolower(pathinfo($configuredPath, PATHINFO_EXTENSION)) : null;
    $logoUrl = in_array($configuredExtension, ['gif', 'jpg', 'jpeg', 'png'], true)
        ? url($configuredLogo)
        : (empty($configuredLogo) ? asset('brand/filebeam-mark-email.png') : null);
@endphp
<tr>
<td class="header" align="center" bgcolor="#0b0914">
<a href="{{ $url }}" class="brand-link">
@if ($logoUrl !== null)
<img src="{{ $logoUrl }}" class="logo" width="32" height="32" alt="" />
@endif
<span class="brand-name">{{ $brandName }}</span>
</a>
</td>
</tr>
