<script setup lang="ts">
import {
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuPortal,
    DropdownMenuRoot,
    DropdownMenuTrigger,
} from 'reka-ui';
import BrandLogo from '../brand/BrandLogo.vue';
import AppLink from '../primitives/AppLink.vue';
import AuthLink from '../auth/AuthLink.vue';
import AppInstallButton from './AppInstallButton.vue';
import CliFooterLauncher from '../cli/CliFooterLauncher.vue';
import Icon, { type IconName } from '../primitives/Icon.vue';
import { computed, nextTick, ref } from 'vue';
import { useBranding } from '../../lib/branding';
import type { CommunityLink, SocialPlatform } from '../../types';

type User = { name: string; username?: string | null; unread_inbox_notifications?: number };
const props = withDefaults(
    defineProps<{
        githubUrl?: string;
        copyrightHolder?: string;
        copyrightUrl?: string | null;
        user?: User | null;
        homeAction?: () => void;
        registrationEnabled?: boolean;
        communityLinks?: CommunityLink[];
    }>(),
    { user: null, registrationEnabled: true, communityLinks: () => [] },
);
const socialIcons: Record<SocialPlatform, IconName> = {
    discord: 'social-discord',
    x: 'social-x',
    bluesky: 'social-bluesky',
    mastodon: 'social-mastodon',
    threads: 'social-threads',
    github: 'social-github',
    youtube: 'social-youtube',
    instagram: 'social-instagram',
    facebook: 'social-facebook',
    linkedin: 'social-linkedin',
    reddit: 'social-reddit',
    telegram: 'social-telegram',
    tiktok: 'social-tiktok',
    twitch: 'social-twitch',
    website: 'globe',
};
function socialIcon(link: CommunityLink): IconName {
    return socialIcons[link.platform] ?? 'globe';
}
const branding = useBranding();
const githubUrl = computed(() => props.githubUrl ?? branding.value.github_url);
const copyrightHolder = computed(() => props.copyrightHolder ?? branding.value.copyright_holder);
const copyrightUrl = computed(() => props.copyrightUrl ?? branding.value.copyright_url ?? null);

const information = [
    {
        title: 'About',
        description: 'Share end-to-end encrypted files and notes with temporary links.',
    },
    {
        title: 'Privacy',
        description:
            'Files are encrypted in your browser. The decryption key remains with the people you choose to share it with.',
    },
    {
        title: 'Help',
        description:
            'Choose files or write a note, then share the generated link with its intended recipient.',
    },
];

type Information = (typeof information)[number];

const activeInformation = ref<Information | null>(null);
const informationOpen = ref(false);
const informationReturnFocus = ref<HTMLElement | null>(null);
const mobileNavigationTrigger = ref<HTMLElement | null>(null);

function openInformation(item: Information, returnFocus: HTMLElement | null): void {
    activeInformation.value = item;
    informationReturnFocus.value = returnFocus;
    nextTick(() => {
        informationOpen.value = true;
    });
}

function openDesktopInformation(item: Information, event: MouseEvent): void {
    openInformation(item, event.currentTarget as HTMLElement);
}

function openMobileInformation(item: Information): void {
    openInformation(item, mobileNavigationTrigger.value);
}

function restoreInformationFocus(event: Event): void {
    event.preventDefault();
    nextTick(() => informationReturnFocus.value?.focus());
}

function goHome(event: MouseEvent): void {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
        return;
    event.preventDefault();
    props.homeAction?.();
}
</script>

<template>
    <div class="fb-shell">
        <header class="fb-header">
            <a
                v-if="homeAction"
                href="/"
                class="fb-header__brand"
                :aria-label="`${branding.name} home`"
                @click="goHome"
                ><BrandLogo
            /></a>
            <AppLink v-else href="/" class="fb-header__brand" :aria-label="`${branding.name} home`"
                ><BrandLogo
            /></AppLink>
            <nav class="fb-header__nav fb-desktop-nav" aria-label="Primary navigation">
                <button
                    v-for="item in information"
                    :key="item.title"
                    class="fb-nav-link"
                    @click="openDesktopInformation(item, $event)"
                >
                    {{ item.title }}
                </button>
                <a
                    class="fb-nav-link fb-nav-link--github"
                    :href="githubUrl"
                    target="_blank"
                    rel="noopener noreferrer"
                    ><span>GitHub</span><Icon name="arrow-up-right" :size="14"
                /></a>
            </nav>
            <AppInstallButton class="fb-header__app" />
            <div class="fb-header__actions">
                <AppLink
                    v-if="user?.unread_inbox_notifications"
                    href="/account/inbox"
                    class="fb-button fb-button--ghost"
                    :aria-label="`${user.unread_inbox_notifications} new inbox notifications`"
                    ><Icon name="folder" :size="18" />{{ user.unread_inbox_notifications }}</AppLink
                >
                <AppLink
                    v-if="user"
                    class="fb-button fb-button--ghost fb-account-link"
                    href="/account"
                    >{{ user.username || user.name }}</AppLink
                >
                <template v-else
                    ><AuthLink class="fb-button fb-button--ghost fb-sign-in-link" href="/login"
                        >Sign in</AuthLink
                    ><AuthLink
                        v-if="registrationEnabled"
                        class="fb-button fb-button--secondary"
                        href="/register"
                        >Register</AuthLink
                    ></template
                >
            </div>
            <DropdownMenuRoot :modal="false">
                <DropdownMenuTrigger as-child>
                    <button
                        ref="mobileNavigationTrigger"
                        class="fb-button fb-button--ghost fb-button--icon fb-mobile-nav"
                        aria-label="Open navigation"
                    >
                        <Icon name="menu" :size="20" />
                    </button>
                </DropdownMenuTrigger>
                <DropdownMenuPortal>
                    <DropdownMenuContent
                        class="fb-select-content fb-mobile-nav__content"
                        :side-offset="8"
                        align="end"
                    >
                        <DropdownMenuItem v-if="user" as-child>
                            <AppLink href="/account" class="fb-select-item fb-mobile-nav__item"
                                >Account</AppLink
                            >
                        </DropdownMenuItem>
                        <DropdownMenuItem v-if="user" as-child>
                            <AppLink
                                href="/account/inbox"
                                class="fb-select-item fb-mobile-nav__item"
                                >Inbox</AppLink
                            >
                        </DropdownMenuItem>
                        <DropdownMenuItem v-if="!user" as-child>
                            <AuthLink href="/login" class="fb-select-item fb-mobile-nav__item"
                                >Sign in</AuthLink
                            >
                        </DropdownMenuItem>
                        <DropdownMenuItem v-if="!user && registrationEnabled" as-child>
                            <AuthLink href="/register" class="fb-select-item fb-mobile-nav__item"
                                >Register</AuthLink
                            >
                        </DropdownMenuItem>
                        <DropdownMenuItem
                            v-for="item in information"
                            :key="item.title"
                            class="fb-select-item fb-mobile-nav__item"
                            @select="openMobileInformation(item)"
                        >
                            {{ item.title }}
                        </DropdownMenuItem>
                        <DropdownMenuItem as-child>
                            <a
                                class="fb-select-item fb-mobile-nav__item"
                                :href="githubUrl"
                                target="_blank"
                                rel="noopener noreferrer"
                            >
                                <span>GitHub</span>
                            </a>
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenuPortal>
            </DropdownMenuRoot>
        </header>
        <DialogRoot v-model:open="informationOpen">
            <DialogPortal>
                <DialogOverlay class="fb-dialog__overlay" />
                <DialogContent
                    v-if="activeInformation"
                    class="fb-dialog__content"
                    @close-auto-focus="restoreInformationFocus"
                >
                    <DialogTitle class="fb-dialog__title">{{
                        activeInformation.title
                    }}</DialogTitle>
                    <DialogDescription class="fb-dialog__description">{{
                        activeInformation.description
                    }}</DialogDescription>
                    <DialogClose class="fb-dialog__close" aria-label="Close dialog"
                        ><Icon name="x" :size="18"
                    /></DialogClose>
                </DialogContent>
            </DialogPortal>
        </DialogRoot>
        <main class="fb-shell__content"><slot /></main>
        <footer class="fb-footer">
            <div class="fb-footer__identity flex items-center gap-3">
                <BrandLogo /><span
                    class="fb-footer__version text-xs font-normal text-[var(--fb-text-muted)]"
                    >v{{ branding.version }}</span
                >
            </div>
            <CliFooterLauncher />
            <nav
                v-if="communityLinks.length"
                class="fb-footer__community"
                aria-label="Community links"
            >
                <a
                    v-for="link in communityLinks"
                    :key="link.platform"
                    class="fb-footer__community-link"
                    :href="link.url"
                    target="_blank"
                    rel="noopener noreferrer"
                    :aria-label="link.label"
                    :title="link.label"
                    ><Icon :name="socialIcon(link)" :size="16"
                /></a>
            </nav>
            <span class="fb-footer__copyright"
                >&copy; {{ branding.copyright_year }}
                <a
                    v-if="copyrightUrl"
                    class="fb-footer__copyright-link"
                    :href="copyrightUrl"
                    target="_blank"
                    rel="noopener noreferrer"
                    >{{ copyrightHolder }}</a
                ><template v-else>{{ copyrightHolder }}</template></span
            >
        </footer>
    </div>
</template>

<style scoped>
.fb-mobile-nav {
    display: none;
}
.fb-header__brand {
    min-width: 0;
    flex: 0 1 auto;
}
.fb-header__brand :deep(.fb-brand) {
    max-width: 100%;
}
.fb-header__brand :deep(.fb-brand__glyph) {
    flex: none;
}
.fb-header__brand :deep(.fb-brand__wordmark) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.fb-footer {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 556px) auto minmax(0, 1fr);
    column-gap: 20px;
}
.fb-footer__copyright {
    justify-self: end;
    text-align: right;
}
.fb-footer__identity {
    flex-wrap: nowrap;
    gap: 0.5rem;
}
.fb-footer__identity :deep(.fb-brand) {
    min-width: 0;
    flex: 0 1 auto;
}
.fb-footer__identity :deep(.fb-brand__lockup) {
    min-width: 0;
    max-width: 100%;
    height: auto;
}
.fb-footer__version {
    flex: none;
    white-space: nowrap;
    overflow-wrap: normal;
}
@media (min-width: 901px) and (max-width: 1100px) {
    .fb-header {
        gap: 0.75rem;
        padding-inline: 1.5rem;
    }
    .fb-header__nav {
        gap: 0;
    }
    .fb-nav-link {
        padding-inline: 0.4rem;
    }
    .fb-footer {
        grid-template-columns: minmax(0, 1fr) minmax(0, 460px) auto minmax(0, 1fr);
    }
}

@media (max-width: 900px) {
    .fb-header {
        flex-wrap: nowrap;
    }

    .fb-header .fb-desktop-nav {
        display: none;
    }
    .fb-header {
        gap: 0.5rem;
    }
    .fb-header__app {
        margin-left: auto;
    }

    .fb-header .fb-header__actions {
        margin-left: auto;
    }
    .fb-header__actions .fb-sign-in-link {
        display: none;
    }
    .fb-header__actions .fb-account-link {
        max-width: 7rem;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        display: block;
    }

    .fb-header .fb-mobile-nav {
        display: inline-flex;
        flex: none;
    }
    .fb-footer {
        grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    }
}
@media (max-width: 560px) {
    .fb-header .fb-header__actions {
        display: none;
    }
}

.fb-mobile-nav__content {
    min-width: 10rem;
}
@media (max-width: 380px) {
    .fb-header__brand :deep(.fb-brand__lockup) {
        max-width: 7rem;
        height: auto;
    }
}

.fb-mobile-nav__item {
    width: 100%;
    box-sizing: border-box;
    color: var(--fb-text-muted);
    text-decoration: none;
}

.fb-mobile-nav__item:hover,
.fb-mobile-nav__item[data-highlighted] {
    color: var(--fb-text);
}
</style>
