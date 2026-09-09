import { computed } from 'vue';

const branding = {
    name: 'Filebeam',
    logo_url: null,
    default_logo_url: '/brand/filebeam-logo-header.svg',
    default_mark_url: '/brand/filebeam-mark.svg',
    favicon_url: null,
    version: 'gallery',
    copyright_holder: 'Vented',
    copyright_year: 2026,
    github_url: 'https://github.com/Vented-Labs/filebeam',
};

export function useBranding() {
    return computed(() => branding);
}
