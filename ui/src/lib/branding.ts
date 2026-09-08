import { usePage } from '@inertiajs/vue3';
import { computed } from 'vue';

export type Branding = {
    name: string;
    logo_url: string | null;
    default_logo_url: string;
    default_mark_url: string;
    favicon_url: string | null;
    version: string;
    copyright_holder: string;
    copyright_year: number;
    github_url: string;
};

const defaultBranding: Branding = {
    name: 'Filebeam',
    logo_url: null,
    default_logo_url: '/brand/filebeam-logo-header.svg',
    default_mark_url: '/brand/filebeam-mark.svg',
    favicon_url: null,
    version: '0.1.0',
    copyright_holder: 'Vented',
    copyright_year: 2026,
    github_url: 'https://github.com/Vented-Labs/filebeam',
};

export function useBranding() {
    const page = usePage<{ branding?: Partial<Branding> }>();

    return computed<Branding>(() => ({ ...defaultBranding, ...page.props.branding }));
}
