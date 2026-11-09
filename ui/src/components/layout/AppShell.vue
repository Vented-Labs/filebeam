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
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from 'reka-ui';
import BrandLogo from '../brand/BrandLogo.vue';
import AppearancePopover from './AppearancePopover.vue';
import AppLink from '../primitives/AppLink.vue';
import AuthLink from '../auth/AuthLink.vue';
import CliFooterLauncher from '../cli/CliFooterLauncher.vue';
import Icon, { type IconName } from '../primitives/Icon.vue';
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
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
        currentPath?: string;
        changelogEnabled?: boolean;
        versionUrl?: string | null;
    }>(),
    {
        user: null,
        registrationEnabled: true,
        communityLinks: () => [],
        currentPath: '/',
        changelogEnabled: true,
        versionUrl: null,
    },
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

const path = computed(() => props.currentPath.split(/[?#]/, 1)[0] || '/');
const destinations = computed(() =>
    [
        { href: '/features', label: 'Features', active: path.value === '/features' },
        {
            href: '/changelog',
            label: 'Changelog',
            active: path.value === '/changelog' || path.value.startsWith('/changelog/'),
            hidden: !props.changelogEnabled,
        },
    ].filter((item) => !item.hidden),
);
// One persistent dot slides between destinations as the shell stays mounted across visits.
const primaryNavigation = ref<HTMLElement | null>(null);
const indicator = reactive({ x: 0, visible: false, ready: false });
let navigationObserver: ResizeObserver | undefined;
function placeIndicator(): void {
    const active = primaryNavigation.value?.querySelector<HTMLElement>('[aria-current="page"]');
    if (!active || active.offsetParent === null) {
        indicator.visible = false;
        return;
    }
    indicator.x = active.offsetLeft + active.offsetWidth / 2 - 2;
    indicator.visible = true;
}
watch(path, () => void nextTick(placeIndicator));
onMounted(() => {
    placeIndicator();
    // Enable motion only after the first placement so the dot never slides in on load.
    requestAnimationFrame(() => (indicator.ready = true));
    if (typeof ResizeObserver !== 'undefined' && primaryNavigation.value) {
        navigationObserver = new ResizeObserver(placeIndicator);
        navigationObserver.observe(primaryNavigation.value);
    }
});
onBeforeUnmount(() => navigationObserver?.disconnect());

const versionHref = computed(() =>
    props.changelogEnabled ? (props.versionUrl ?? '/changelog') : null,
);

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

function openFooterInformation(item: Information, event: MouseEvent): void {
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
            <nav
                ref="primaryNavigation"
                class="fb-header__nav fb-primary-nav"
                aria-label="Primary navigation"
            >
                <span
                    class="fb-nav-indicator"
                    :class="{ 'fb-nav-indicator--ready': indicator.ready }"
                    :style="{
                        transform: `translateX(${indicator.x}px)`,
                        opacity: indicator.visible ? 1 : 0,
                    }"
                    aria-hidden="true"
                />
                <AppLink
                    v-for="item in destinations"
                    :key="item.href"
                    :href="item.href"
                    class="fb-nav-link"
                    :aria-current="item.active ? 'page' : undefined"
                    prefetch
                    >{{ item.label }}</AppLink
                >
                <a
                    class="fb-nav-link fb-nav-link--github"
                    :href="githubUrl"
                    target="_blank"
                    rel="noopener noreferrer"
                    ><span>GitHub</span><Icon name="arrow-up-right" :size="13"
                /></a>
            </nav>
            <div class="fb-header__actions">
                <div class="fb-header__account">
                    <template v-if="user">
                        <AppLink
                            href="/account/history"
                            class="fb-nav-link"
                            :aria-current="path === '/account/history' ? 'page' : undefined"
                            >History</AppLink
                        >
                        <AppLink
                            href="/account/contacts"
                            class="fb-nav-link"
                            :aria-current="path === '/account/contacts' ? 'page' : undefined"
                            >Contacts</AppLink
                        >
                        <AppLink
                            v-if="user.unread_inbox_notifications"
                            href="/account/inbox"
                            class="fb-nav-link fb-nav-link--inbox"
                            :aria-label="`${user.unread_inbox_notifications} new inbox notifications`"
                            ><Icon name="folder" :size="16" />{{
                                user.unread_inbox_notifications
                            }}</AppLink
                        >
                        <AppLink
                            class="fb-nav-link fb-account-link fb-header__slot"
                            href="/account"
                            :aria-current="path === '/account' ? 'page' : undefined"
                            >{{ user.username || user.name }}</AppLink
                        >
                    </template>
                    <template v-else>
                        <AuthLink
                            class="fb-nav-link fb-sign-in-link"
                            :class="{ 'fb-header__slot': !registrationEnabled }"
                            href="/login"
                            >Sign in</AuthLink
                        >
                        <AuthLink
                            v-if="registrationEnabled"
                            class="fb-button fb-button--secondary fb-header__button fb-register-link fb-header__slot"
                            href="/register"
                            >Register</AuthLink
                        >
                    </template>
                </div>
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
                        <DropdownMenuItem v-for="item in destinations" :key="item.href" as-child>
                            <AppLink
                                :href="item.href"
                                class="fb-select-item fb-mobile-nav__item"
                                :aria-current="item.active ? 'page' : undefined"
                                >{{ item.label }}</AppLink
                            >
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
                        <DropdownMenuSeparator class="fb-mobile-nav__separator" />
                        <template v-if="user">
                            <DropdownMenuItem as-child>
                                <AppLink
                                    href="/account/history"
                                    class="fb-select-item fb-mobile-nav__item"
                                    :aria-current="path === '/account/history' ? 'page' : undefined"
                                    >History</AppLink
                                >
                            </DropdownMenuItem>
                            <DropdownMenuItem as-child>
                                <AppLink
                                    href="/account"
                                    class="fb-select-item fb-mobile-nav__item"
                                    :aria-current="path === '/account' ? 'page' : undefined"
                                    >Account</AppLink
                                >
                            </DropdownMenuItem>
                            <DropdownMenuItem as-child>
                                <AppLink
                                    href="/account/contacts"
                                    class="fb-select-item fb-mobile-nav__item"
                                    :aria-current="
                                        path === '/account/contacts' ? 'page' : undefined
                                    "
                                    >Contacts</AppLink
                                >
                            </DropdownMenuItem>
                            <DropdownMenuItem as-child>
                                <AppLink
                                    href="/account/inbox"
                                    class="fb-select-item fb-mobile-nav__item"
                                    :aria-current="path === '/account/inbox' ? 'page' : undefined"
                                    >Inbox<span
                                        v-if="user.unread_inbox_notifications"
                                        class="fb-mobile-nav__count"
                                        >{{ user.unread_inbox_notifications }}</span
                                    ></AppLink
                                >
                            </DropdownMenuItem>
                            <DropdownMenuItem as-child>
                                <AppLink
                                    href="/account/devices"
                                    class="fb-select-item fb-mobile-nav__item"
                                    :aria-current="path === '/account/devices' ? 'page' : undefined"
                                    >My devices</AppLink
                                >
                            </DropdownMenuItem>
                        </template>
                        <template v-else>
                            <DropdownMenuItem as-child>
                                <AuthLink href="/login" class="fb-select-item fb-mobile-nav__item"
                                    >Sign in</AuthLink
                                >
                            </DropdownMenuItem>
                            <DropdownMenuItem v-if="registrationEnabled" as-child>
                                <AuthLink
                                    href="/register"
                                    class="fb-select-item fb-mobile-nav__item"
                                    >Register</AuthLink
                                >
                            </DropdownMenuItem>
                        </template>
                        <DropdownMenuSeparator class="fb-mobile-nav__separator" />
                        <DropdownMenuItem
                            v-for="item in information"
                            :key="item.title"
                            class="fb-select-item fb-mobile-nav__item fb-mobile-nav__item--quiet"
                            @select="openMobileInformation(item)"
                        >
                            {{ item.title }}
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
                <BrandLogo />
                <AppearancePopover />
                <AppLink
                    v-if="versionHref"
                    :href="versionHref"
                    class="fb-footer__version"
                    :title="`Release notes for v${branding.version}`"
                    :aria-label="`Release notes for v${branding.version}`"
                    >v{{ branding.version }}</AppLink
                >
                <span v-else class="fb-footer__version" :title="`v${branding.version}`"
                    >v{{ branding.version }}</span
                >
            </div>
            <CliFooterLauncher />
            <div class="fb-footer__right">
                <nav class="fb-footer__links" aria-label="Information">
                    <button
                        v-for="item in information"
                        :key="item.title"
                        type="button"
                        class="fb-footer__link"
                        @click="openFooterInformation(item, $event)"
                    >
                        {{ item.title }}
                    </button>
                </nav>
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
            </div>
        </footer>
    </div>
</template>

<style scoped>
.fb-header {
    /* Centre the header content at the design's 1400px frame without an extra wrapper. */
    min-height: 5.5rem;
    gap: 2.25rem;
    padding-inline: max(clamp(1rem, 4vw, 3rem), calc((100% - 81.5rem) / 2));
    border-bottom-color: transparent;
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
.fb-header__nav {
    gap: 1.8rem;
}
.fb-header .fb-nav-link {
    position: relative;
    min-height: 2rem;
    padding: 0.5rem 0;
    border-radius: 0.25rem;
    font-size: 0.75rem;
}
.fb-header .fb-nav-link:hover,
.fb-header .fb-nav-link:active {
    background: transparent;
    color: var(--fb-text);
}
.fb-header .fb-nav-link[aria-current='page'] {
    color: var(--fb-text);
}
.fb-header__nav {
    position: relative;
}
.fb-nav-indicator {
    position: absolute;
    bottom: -3px;
    left: 0;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--fb-accent-text);
    pointer-events: none;
    transition: opacity var(--fb-duration-control) ease;
}
.fb-nav-indicator--ready {
    transition:
        transform 420ms var(--fb-ease),
        opacity var(--fb-duration-control) ease;
}
@media (prefers-reduced-motion: reduce) {
    .fb-nav-indicator--ready {
        transition: opacity var(--fb-duration-control) ease;
    }
}
.fb-nav-link--github {
    gap: 0.25rem;
}
.fb-nav-link--github .fb-icon {
    opacity: 0.65;
}
.fb-header__actions {
    gap: 1.25rem;
}
.fb-header__account {
    display: flex;
    align-items: center;
    gap: 1.25rem;
}
.fb-nav-link--inbox {
    gap: 0.3rem;
}
.fb-header__button {
    min-height: 2.625rem;
    padding: 0.5625rem 1.125rem;
    font-size: 0.75rem;
}
.fb-account-link {
    max-width: 10rem;
    display: block;
    white-space: nowrap;
}
.fb-mobile-nav {
    display: none;
}
.fb-footer {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 556px) minmax(0, 1fr);
    column-gap: 20px;
    padding-inline: max(clamp(1rem, 4vw, 3rem), calc((100% - 81.5rem) / 2));
}
.fb-footer__right {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: end;
    gap: 20px;
}
.fb-footer__links {
    display: flex;
    align-items: center;
    gap: 0.9rem;
}
.fb-footer__link {
    min-height: 2rem;
    padding: 0 0.1rem;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    color: var(--fb-text-muted);
    font: inherit;
    cursor: pointer;
    transition: color var(--fb-duration-control) ease;
}
.fb-footer__link:hover {
    color: var(--fb-text);
}
.fb-footer__copyright {
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
.fb-footer__identity :deep(.fb-brand__glyph) {
    flex: none;
}
.fb-footer__identity :deep(.fb-brand__wordmark) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.fb-footer__version {
    flex: none;
    min-width: 0;
    max-width: min(12rem, 25vw);
    overflow: hidden;
    border-radius: 0.375rem;
    background: transparent;
    color: var(--fb-text-muted);
    font-size: 0.6875rem;
    text-decoration: none;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition:
        color var(--fb-duration-control) ease,
        background-color var(--fb-duration-control) ease;
}
a.fb-footer__version:hover {
    background: var(--fb-surface-hover);
    color: var(--fb-accent-text);
}
@media (min-width: 1200px) and (max-width: 1350px) {
    .fb-footer {
        grid-template-columns: minmax(0, 1fr) minmax(0, 400px) minmax(0, 1fr);
    }
}
@media (min-width: 901px) and (max-width: 1199px) {
    .fb-footer {
        grid-template-columns: minmax(0, 1fr) minmax(0, 220px) minmax(0, 1fr);
    }
    .fb-footer :deep(.cli-footer-launcher code),
    .fb-footer :deep(.cli-footer-divider) {
        display: none;
    }
}

/* Account links collapse into the menu first; one action stays beside the primary links. */
@media (max-width: 1100px) {
    .fb-header {
        gap: 1.6rem;
    }
    .fb-header__nav {
        gap: 1.5rem;
    }
    .fb-header__account > :not(.fb-header__slot) {
        display: none;
    }
    .fb-header .fb-mobile-nav {
        display: inline-flex;
        flex: none;
        margin-left: -0.75rem;
    }
}
@media (max-width: 850px) {
    .fb-header {
        min-height: 4.875rem;
        gap: 1.25rem;
    }
    .fb-header__nav {
        gap: 1.2rem;
    }
    .fb-header .fb-nav-link {
        font-size: 0.6875rem;
    }
    .fb-header__button {
        padding-inline: 0.75rem;
        font-size: 0.6875rem;
    }
}
@media (max-width: 900px) {
    .fb-footer {
        grid-template-columns: minmax(18rem, 1fr) minmax(0, 1fr);
    }
    .fb-footer__right {
        flex-wrap: wrap;
    }
}
@media (max-width: 650px) {
    .fb-header {
        min-height: 4.75rem;
        gap: 1rem;
    }
    .fb-header .fb-nav-link--github,
    .fb-header__actions {
        display: none;
    }
    .fb-header .fb-mobile-nav {
        margin-left: -0.25rem;
    }
}
@media (max-width: 560px) {
    .fb-footer {
        grid-template-columns: minmax(0, 1fr);
    }
    .fb-footer__right {
        justify-content: start;
    }
}
@media (max-width: 400px) {
    .fb-header .fb-primary-nav {
        display: none;
    }
    .fb-header .fb-mobile-nav {
        margin-left: auto;
    }
    .fb-header__brand :deep(.fb-brand__lockup) {
        max-width: 7rem;
        height: auto;
    }
}

.fb-mobile-nav__content {
    min-width: 11rem;
}
.fb-mobile-nav__item {
    width: 100%;
    box-sizing: border-box;
    gap: 0.5rem;
    color: var(--fb-text-muted);
    text-decoration: none;
}
.fb-mobile-nav__item:hover,
.fb-mobile-nav__item[data-highlighted] {
    color: var(--fb-text);
}
.fb-mobile-nav__item[aria-current='page'] {
    color: var(--fb-text);
}
.fb-mobile-nav__item[aria-current='page']::after {
    content: '';
    width: 4px;
    height: 4px;
    margin-left: auto;
    border-radius: 50%;
    background: var(--fb-accent-text);
}
.fb-mobile-nav__item--action {
    color: var(--fb-accent-text);
    font-weight: 600;
}
.fb-mobile-nav__item--quiet {
    font-size: 0.75rem;
}
.fb-mobile-nav__count {
    margin-left: auto;
    padding: 0 0.4rem;
    border-radius: 999px;
    background: var(--fb-accent-surface);
    color: var(--fb-accent-text);
    font-size: 0.6875rem;
}
.fb-mobile-nav__separator {
    height: 1px;
    margin: 0.25rem 0.5rem;
    background: var(--fb-line-faint);
}
@media (min-width: 901px) and (max-height: 1000px) {
    .fb-header {
        min-height: 4.5rem;
    }
    .fb-footer {
        min-height: 4rem;
    }
}
@media (min-width: 901px) and (max-height: 760px) {
    .fb-header {
        min-height: 4rem;
    }
    .fb-footer {
        min-height: 3.5rem;
    }
}
</style>
