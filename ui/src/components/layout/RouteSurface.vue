<script lang="ts">
import { Head, router, usePage } from '@inertiajs/vue3';
import { cloneVNode, defineComponent, h, provide, type VNode } from 'vue';
import { ConfigProvider } from 'reka-ui';
import FilebeamHome from '../FilebeamHome.vue';
import AppShell from './AppShell.vue';
import PageTransition from './PageTransition.vue';
import CliProvider from '../cli/CliProvider.vue';
import type { FilebeamConfig } from '../../types';

export default defineComponent({
    setup(_, { slots }) {
        const page = usePage<{
            filebeam: FilebeamConfig;
            auth: { user: { name: string; username?: string | null } | null };
        }>();
        let background: VNode | undefined;
        let backgroundUrl = '/';
        let wasAuthentication = false;
        provide('authReturnUrl', () => backgroundUrl);
        function goHome(): void {
            if (backgroundUrl === '/') {
                window.dispatchEvent(new Event('filebeam:home'));
                return;
            }
            router.visit('/');
        }
        const scrollBody =
            typeof CSS !== 'undefined' && CSS.supports('scrollbar-gutter: stable')
                ? { padding: 0, margin: 0 }
                : true;

        return () => {
            const child = slots.default?.()[0];
            const authentication = page.component === 'auth/AuthScreen';

            // Retain the mounted page beneath auth, without putting drafts or keys in history.
            if (!authentication) {
                background =
                    wasAuthentication &&
                    page.url === backgroundUrl &&
                    background?.type === child?.type
                        ? cloneVNode(background!, {
                              ...child?.props,
                              key: background?.key ?? undefined,
                          })
                        : child;
                backgroundUrl = page.url;
            }
            background ??= h('div', [
                h(Head, { title: 'Secure sharing' }),
                h(FilebeamHome, { config: page.props.filebeam, user: page.props.auth.user }),
            ]);
            wasAuthentication = authentication;
            const renderedBackground = background;

            return h(ConfigProvider, { scrollBody }, () =>
                h(CliProvider, { config: page.props.filebeam.cli }, () =>
                    h(
                        AppShell,
                        {
                            githubUrl: page.props.filebeam.github_url,
                            copyrightHolder: page.props.filebeam.copyright_holder,
                            copyrightUrl: page.props.filebeam.copyright_url,
                            user: page.props.auth.user,
                            registrationEnabled: page.props.filebeam.registration_enabled,
                            communityLinks: page.props.filebeam.community_links,
                            homeAction: goHome,
                        },
                        () => [
                            h(
                                PageTransition,
                                { key: 'background', pageKey: backgroundUrl },
                                () => renderedBackground,
                            ),
                            authentication ? h('div', { key: 'authentication' }, [child]) : null,
                        ],
                    ),
                ),
            );
        };
    },
});
</script>
