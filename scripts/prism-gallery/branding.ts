import { computed } from 'vue';
import { useAppearance } from '../../ui/src/composables/useAppearance';

const branding = {
    name: 'Filebeam',
    logo_url: null,
    default_logo_url: '/brand/filebeam-logo-header.svg',
    default_light_logo_url: '/brand/filebeam-logo-header-on-light.svg',
    default_mark_url: '/brand/filebeam-mark.svg',
    favicon_url: null,
    version: 'gallery',
    copyright_holder: 'Vented',
    copyright_year: 2026,
    github_url: 'https://github.com/Vented-Labs/filebeam',
};

export function useBranding() {
    const { palette } = useAppearance();
    return computed(() => ({ ...branding, ...palette.value?.branding }));
}
