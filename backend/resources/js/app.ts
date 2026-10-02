import { createInertiaApp, router } from '@inertiajs/vue3';
import type { AppearancePayload } from '../../../ui/src/lib/appearance-types';

if (typeof document !== 'undefined')
    router.on('navigate', ({ detail }) => {
        const theme = detail.page.props.theme as
            | {
                  primary: string;
                  css: string;
                  chrome: { dark: string; light: string };
              }
            | undefined;
        const style = document.getElementById('filebeam-theme');
        if (!theme || !style) return;
        style.textContent = theme.css;
        style.dataset.primary = theme.primary;
        const chrome = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]');
        if (chrome) {
            chrome.dataset.fbDark = theme.chrome.dark;
            chrome.dataset.fbLight = theme.chrome.light;
        }
        const appearance = detail.page.props.appearance as AppearancePayload | undefined;
        if (appearance) window.filebeamAppearance?.hydrate(appearance);
        else window.filebeamAppearance?.refresh();
    });

const appName =
    typeof document === 'undefined'
        ? 'Filebeam'
        : document.querySelector<HTMLMetaElement>('meta[name="application-name"]')?.content ||
          'Filebeam';

void createInertiaApp({
    title: (title) => (title ? `${title} - ${appName}` : appName),
    progress: {
        color: 'var(--fb-progress-fill)',
    },
    defaults: {
        visitOptions: (_href, options) => ({
            ...options,
            viewTransition: options.viewTransition ?? false,
        }),
    },
});
