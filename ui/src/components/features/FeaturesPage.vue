<script setup lang="ts">
import { usePage } from '@inertiajs/vue3';
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import Icon from '../primitives/Icon.vue';
import Toast from '../primitives/Toast.vue';
import AppearanceCard from './AppearanceCard.vue';
import CliCard from './CliCard.vue';
import DevicePanel from './DevicePanel.vue';
import FeaturesHero from './FeaturesHero.vue';
import FriendsCard from './FriendsCard.vue';
import HistoryCard from './HistoryCard.vue';
import NotesCard from './NotesCard.vue';
import PrivacyCard from './PrivacyCard.vue';
import TransportPanel from './TransportPanel.vue';
import { instanceHost } from './tabs';
import { useBranding } from '../../lib/branding';
import type { FilebeamConfig, TransferDriver } from '../../types';

const props = withDefaults(defineProps<{ devicesEnabled?: boolean }>(), {
    devicesEnabled: false,
});
const page = usePage<{ filebeam: FilebeamConfig; auth: { user: unknown | null } }>();
const branding = useBranding();
// Product documentation lives in the upstream project, independent of a fork's branding link.
const project = 'https://github.com/Vented-Labs/filebeam';
const host = computed(() => instanceHost(page.props.filebeam.main_site_url));
const signedIn = computed(() => Boolean(page.props.auth.user));
const registrationEnabled = computed(() => page.props.filebeam.registration_enabled !== false);
const drivers = computed<TransferDriver[]>(
    () => page.props.filebeam.transport_policy?.enabled_drivers ?? ['http'],
);

const chapters = [
    { id: 'devices', label: 'Devices', icon: 'webrtc-p2p' },
    { id: 'sharing', label: 'Sharing', icon: 'fast-and-simple' },
    { id: 'notes-friends', label: 'Notes & friends', icon: 'note' },
    { id: 'privacy', label: 'Privacy', icon: 'eye-off' },
    { id: 'open-source', label: 'Open source', icon: 'heart' },
] as const;
const active = ref<string>('devices');
const toast = ref({ open: false, title: '' });
let queued = false;

function spy(): void {
    queued = false;
    let current: string = chapters[0].id;
    for (const chapter of chapters) {
        const element = document.getElementById(chapter.id);
        if (element && element.getBoundingClientRect().top < window.innerHeight * 0.42)
            current = chapter.id;
    }
    active.value = current;
}
function onScroll(): void {
    if (queued) return;
    queued = true;
    requestAnimationFrame(spy);
}
function go(id: string, event?: MouseEvent): void {
    const target = document.getElementById(id);
    if (!target || (event && (event.metaKey || event.ctrlKey || event.shiftKey))) return;
    event?.preventDefault();
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    target.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'start' });
    window.history.replaceState(window.history.state, '', `#${id}`);
}
function notify(message: string): void {
    toast.value = { open: false, title: message };
    void nextTick(() => (toast.value.open = true));
}

onMounted(() => {
    spy();
    window.addEventListener('scroll', onScroll, { passive: true });
});
onBeforeUnmount(() => window.removeEventListener('scroll', onScroll));
</script>

<template>
    <div class="fb-features">
        <div class="page-width">
            <FeaturesHero :host="host" @explore="go('explore')" />
            <nav id="explore" class="section-nav" aria-label="Explore features">
                <a
                    v-for="chapter in chapters"
                    :key="chapter.id"
                    :href="`#${chapter.id}`"
                    :class="{ active: active === chapter.id }"
                    :aria-current="active === chapter.id ? 'location' : undefined"
                    @click="go(chapter.id, $event)"
                    ><Icon class="icon" :name="chapter.icon" /> {{ chapter.label }}</a
                >
            </nav>

            <DevicePanel
                :enabled="props.devicesEnabled"
                :signed-in="signedIn"
                :registration-enabled="registrationEnabled"
            />
            <TransportPanel
                :enabled-drivers="drivers"
                :cli-docs-url="`${project}/blob/master/crates/cli/README.md`"
            />

            <div class="mini-feature-rail" role="group" aria-label="More sharing features">
                <details>
                    <summary>
                        <Icon class="icon" name="lock" /> <span>Passwords &amp; expiry</span
                        ><Icon class="icon" name="plus" />
                    </summary>
                    <p>
                        Add a password and choose a transfer’s lifetime. Available limits follow
                        this server and your account plan.
                    </p>
                </details>
                <details>
                    <summary>
                        <Icon class="icon" name="copy" /> <span>Paste text or images</span
                        ><Icon class="icon" name="plus" />
                    </summary>
                    <p>
                        On web and CLI, paste text into Notes or add a clipboard image to Files.
                        Pasting never sends automatically.
                    </p>
                </details>
                <details>
                    <summary>
                        <Icon class="icon" name="note" /> <span>Burn-on-read notes</span
                        ><Icon class="icon" name="plus" />
                    </summary>
                    <p>
                        Request deletion after a note is read. Removal is best-effort; a recipient
                        may already have a copy.
                    </p>
                </details>
            </div>

            <section
                id="notes-friends"
                class="feature-section collaboration-grid"
                aria-label="Notes and friends"
            >
                <NotesCard @notify="notify" />
                <FriendsCard :signed-in="signedIn" :registration-enabled="registrationEnabled" />
            </section>

            <section
                id="privacy"
                class="feature-section privacy-history-grid"
                aria-label="Privacy and transfer history"
            >
                <PrivacyCard :host="host" :security-url="`${project}/blob/master/SECURITY.md`" />
                <HistoryCard :signed-in="signedIn" />
            </section>

            <section class="feature-section extras-grid" aria-label="Terminal and appearance">
                <CliCard
                    :host="host"
                    :docs-url="`${project}/blob/master/crates/cli/README.md`"
                    @notify="notify"
                />
                <AppearanceCard />
            </section>

            <section
                id="open-source"
                class="open-source-section feature-section"
                aria-labelledby="open-source-title"
            >
                <div class="open-source-copy">
                    <div class="section-icon"><Icon class="icon" name="heart" /></div>
                    <h2 id="open-source-title">Open Source.</h2>
                    <p>
                        MIT-licensed. Read the code, run your own instance,<br
                            class="desktop-only"
                        />
                        or make something with it.
                    </p>
                    <div class="source-actions">
                        <a
                            :href="branding.github_url"
                            class="button"
                            target="_blank"
                            rel="noopener noreferrer"
                            ><Icon class="icon" name="social-github" /> View the source
                            <Icon class="icon" name="arrow-up-right"
                        /></a>
                        <a
                            :href="`${project}/blob/master/docs/deployment.md`"
                            class="text-link"
                            target="_blank"
                            rel="noopener noreferrer"
                            >Self-hosting docs <Icon class="icon" name="arrow-right"
                        /></a>
                    </div>
                </div>
                <div class="license-art" aria-hidden="true">
                    <div class="license-back" />
                    <div class="license-page">
                        <span>LICENSE</span><strong>MIT</strong>
                        <div class="license-lines"><i /><i /><i /></div>
                        <div><Icon class="icon" name="heart" /><span>Filebeam / Vented</span></div>
                    </div>
                </div>
            </section>
            <div class="platforms-footer">
                <span>Also in the repo</span>
                <a
                    :href="`${project}/blob/master/docs/desktop.md`"
                    target="_blank"
                    rel="noopener noreferrer"
                    ><Icon class="icon" name="monitor" /> Desktop
                    <span class="maturity">Beta</span></a
                >
                <a
                    :href="`${project}/tree/master/mobile/android`"
                    target="_blank"
                    rel="noopener noreferrer"
                    ><Icon class="icon" name="mobile" /> Android
                    <span class="maturity">Pre-alpha</span></a
                >
                <a
                    :href="`${project}/tree/master/mobile/ios`"
                    target="_blank"
                    rel="noopener noreferrer"
                    ><Icon class="icon" name="mobile" /> iOS
                    <span class="maturity">Pre-alpha</span></a
                >
            </div>
        </div>
        <Toast v-model:open="toast.open" :title="toast.title" />
    </div>
</template>

<style src="./features.css"></style>
