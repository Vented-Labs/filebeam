import { createInertiaApp } from '@inertiajs/vue3';

const appName =
    typeof document === 'undefined'
        ? 'Filebeam'
        : document.querySelector<HTMLMetaElement>('meta[name="application-name"]')?.content ||
          'Filebeam';

void createInertiaApp({
    title: (title) => (title ? `${title} - ${appName}` : appName),
    progress: {
        color: 'var(--fb-brand-bright)',
    },
    defaults: {
        visitOptions: (_href, options) => ({
            ...options,
            viewTransition: options.viewTransition ?? false,
        }),
    },
});
