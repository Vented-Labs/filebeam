<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { cancelAccountCrypto, openRecipientKey } from '../lib/account-crypto';
import type { FilebeamConfig } from '../types';
import { useEncryptedDownload } from '../composables/useEncryptedDownload';
import NoteViewer from './notes/NoteViewer.vue';
import Button from './primitives/Button.vue';
import Icon from './primitives/Icon.vue';
import DownloadList from './download/DownloadList.vue';
import DownloadStatus from './download/DownloadStatus.vue';
import TransferExpiry from './download/TransferExpiry.vue';
import TransferUnlock from './download/TransferUnlock.vue';
import TransferUnavailable from './download/TransferUnavailable.vue';
import Input from './primitives/Input.vue';
import AppLink from './primitives/AppLink.vue';
import WebRtcConsent from './sharing/WebRtcConsent.vue';
import AnimatedHeight from './layout/AnimatedHeight.vue';
import AnimatedReveal from './layout/AnimatedReveal.vue';
import CliDownloadCard from './cli/CliDownloadCard.vue';
import { buildShareLink } from '../lib/share-link';

const props = defineProps<{
    transferId: string;
    inbox?: boolean;
    config: FilebeamConfig;
    user?: { name: string; username?: string | null } | null;
}>();
const download = useEncryptedDownload(props.transferId, props.inbox);
const cliTarget = ref('');
function syncCliTarget(): void {
    cliTarget.value = buildShareLink(
        `/${props.transferId}`,
        window.location.origin,
        window.location.hash,
    );
}
const fileHeading = computed(() => {
    const count = download.manifest.value?.items.length ?? 0;
    if (download.isDownloading.value) {
        const selected = download.downloadItemCount.value;
        return `${selected} file${selected === 1 ? '' : 's'} downloading`;
    }
    const completed = download.downloadedItemIds.value.length;
    if (completed && completed < count) return `${completed} of ${count} files downloaded`;
    return `${count} file${count === 1 ? '' : 's'} ${completed ? 'downloaded' : 'ready to download'}`;
});
const displayError = computed(() =>
    download.state.value === 'key' &&
    /atob|base64url|not correctly encoded|key must be 32 bytes/i.test(download.error.value)
        ? 'Invalid decryption key - please enter the decryption key'
        : download.error.value,
);
const presentationKey = computed(() => {
    if (download.state.value === 'loading') return 'loading';
    if (download.state.value === 'error') return 'error';
    if (download.state.value === 'key') return props.inbox ? 'inbox-key' : 'key';
    return download.isNote.value ? 'note' : 'files';
});
const secret = ref('');
const unlockingRecipient = ref(false);
const webrtcConsentDialog = ref<InstanceType<typeof WebRtcConsent>>();
let disposed = false;

async function requestWebRtcConsent(): Promise<boolean> {
    if (!download.webrtc.value) return true;
    if (typeof RTCPeerConnection === 'undefined') {
        download.error.value = 'WebRTC is not supported by this browser.';
        return false;
    }
    const accepted = await webrtcConsentDialog.value?.requestConsent();
    if (accepted) download.webrtcConsent.value = true;
    return Boolean(accepted);
}
async function downloadFiles(items?: Parameters<typeof download.downloadFiles>[0]): Promise<void> {
    if (!download.webrtc.value) return download.downloadFiles(items);
    if (!(await requestWebRtcConsent())) return;
    await download.downloadFiles(items);
}
async function decryptNote(): Promise<void> {
    if (!download.webrtc.value) return download.decryptNote();
    if (!(await requestWebRtcConsent())) return;
    await download.decryptNote();
}
async function downloadNote(): Promise<void> {
    if (download.note.value) {
        await download.downloadNote();
        return;
    }
    await decryptNote();
}

async function unlockRecipient(): Promise<void> {
    const recipientKey = download.transfer.value?.recipient_key;
    if (!recipientKey || unlockingRecipient.value) return;
    unlockingRecipient.value = true;
    download.error.value = '';
    try {
        const key = await openRecipientKey(recipientKey, secret.value, props.transferId);
        secret.value = '';
        if (!disposed) await download.unlock(key, '');
    } catch {
        if (!disposed)
            download.error.value =
                'The password or private key could not unlock this transfer. Use the secret for the key version shown below.';
    } finally {
        secret.value = '';
        unlockingRecipient.value = false;
    }
}

onBeforeUnmount(() => {
    window.removeEventListener('hashchange', syncCliTarget);
    disposed = true;
    secret.value = '';
    cancelAccountCrypto();
});

onMounted(() => {
    syncCliTarget();
    window.addEventListener('hashchange', syncCliTarget);
    void download.load();
});

function makeOutgoingInert(element: Element): void {
    const pane = element as HTMLElement;
    pane.inert = true;
    pane.setAttribute('aria-hidden', 'true');
}
</script>

<template>
    <section class="transfer-page">
        <AppLink v-if="inbox" href="/account/inbox" class="fb-text-link mb-5 inline-block"
            >Back to inbox</AppLink
        >
        <div class="transfer-card">
            <AnimatedHeight>
                <Transition name="transfer-state" @before-leave="makeOutgoingInert">
                    <div :key="presentationKey" class="transfer-state">
                        <div
                            v-if="download.state.value === 'loading'"
                            class="transfer-loading"
                            role="status"
                        >
                            <span><Icon name="loader" :size="23" /></span>
                            <strong>Loading encrypted transfer</strong>
                            <p>Preparing the browser-only decryption workspace.</p>
                        </div>
                        <form
                            v-else-if="download.state.value === 'key' && inbox"
                            class="mx-auto max-w-md space-y-5"
                            @submit.prevent="unlockRecipient"
                        >
                            <Icon name="lock" :size="32" />
                            <h1 class="text-3xl font-semibold">Unlock received files</h1>
                            <p class="text-sm text-[var(--fb-text-muted)]">
                                Decryption happens in this browser. Your unlock secret is not sent
                                to Filebeam.
                            </p>
                            <label for="inbox-secret" class="block text-sm font-medium">{{
                                download.transfer.value?.recipient_key?.bundle.custody_mode ===
                                'password'
                                    ? 'Key password'
                                    : 'Private key export'
                            }}</label>
                            <Input
                                id="inbox-secret"
                                v-model="secret"
                                type="password"
                                :autocomplete="
                                    download.transfer.value?.recipient_key?.bundle.custody_mode ===
                                    'password'
                                        ? 'current-password'
                                        : 'off'
                                "
                                :disabled="unlockingRecipient"
                                required
                            />
                            <p class="text-xs text-[var(--fb-text-muted)]">
                                Key version
                                {{ download.transfer.value?.recipient_key?.bundle.version }}.
                                {{
                                    download.transfer.value?.recipient_key?.bundle.custody_mode ===
                                    'password'
                                        ? 'Use the password that protected this key, even if you have since reset your login password.'
                                        : 'Paste the saved key beginning with fbsk1.'
                                }}
                            </p>
                            <Button
                                type="submit"
                                :disabled="unlockingRecipient || download.isUnlocking.value"
                                >{{ unlockingRecipient ? 'Unlocking...' : 'Unlock files' }}</Button
                            >
                        </form>
                        <TransferUnlock
                            v-else-if="download.state.value === 'key'"
                            :unlocking="download.isUnlocking.value"
                            :password-required="download.passwordRequired.value"
                            @unlock="download.unlock"
                            @clear-error="download.error.value = ''"
                        />
                        <template
                            v-else-if="
                                download.state.value !== 'error' &&
                                download.transfer.value &&
                                download.manifest.value
                            "
                        >
                            <header class="transfer-content__header">
                                <span class="transfer-content__emblem">
                                    <Icon
                                        :name="download.isNote.value ? 'note' : 'lock'"
                                        :size="24"
                                    />
                                </span>
                                <div class="transfer-content__heading">
                                    <h1>
                                        {{
                                            download.isNote.value
                                                ? download.manifest.value.title || 'Secure note'
                                                : fileHeading
                                        }}
                                    </h1>
                                    <div class="transfer-content__meta" data-testid="transfer-meta">
                                        <span v-if="download.webrtc.value">
                                            WebRTC live transfer. The sender must keep their browser
                                            open.
                                        </span>
                                        <span
                                            v-if="
                                                !download.webrtc.value &&
                                                download.turbo.value &&
                                                download.uploaderStatus.value !== 'completed'
                                            "
                                        >
                                            Retention starts when uploading finishes.
                                        </span>
                                        <TransferExpiry
                                            v-else-if="!download.webrtc.value"
                                            :expires-at="download.transfer.value.expires_at"
                                        />
                                    </div>
                                </div>
                                <div v-if="$slots.report" class="transfer-content__report">
                                    <slot name="report" :transfer-id="transferId" />
                                </div>
                            </header>
                            <p
                                v-if="download.transfer.value.burn_on_read"
                                class="mt-4 rounded-xl border border-[var(--fb-warning)] bg-[var(--fb-surface-raised)] p-3 text-sm text-[var(--fb-warning)]"
                                role="status"
                            >
                                {{
                                    download.burned.value
                                        ? download.webrtc.value
                                            ? 'The live share was revoked. Your decrypted copy remains in this tab.'
                                            : 'Removed from the server. Your decrypted copy remains in this tab.'
                                        : download.webrtc.value
                                          ? 'Burn on read: this live share is revoked after the first successful decrypt.'
                                          : 'Burn on read: this note is removed from the server after successful decryption.'
                                }}
                            </p>
                            <NoteViewer
                                v-if="download.isNote.value && download.note.value"
                                class="mt-6"
                                :content="download.note.value"
                                :language="download.manifest.value.language || 'plain'"
                            />
                            <DownloadList
                                v-else-if="!download.isNote.value"
                                class="mt-6"
                                :items="download.manifest.value.items"
                            />
                            <p
                                v-if="download.turbo.value && !download.webrtc.value"
                                class="mt-5 text-sm text-[var(--fb-text-muted)]"
                            >
                                Your download progress is shared anonymously with the sender while
                                this transfer is active.
                            </p>
                            <AnimatedReveal
                                :show="
                                    (download.turbo.value && !download.webrtc.value) ||
                                    download.isDownloading.value
                                "
                            >
                                <DownloadStatus
                                    :progress="download.progress.value"
                                    :downloading="download.isDownloading.value"
                                    :phase="download.downloadPhase.value"
                                    :turbo="download.turbo.value"
                                    :upload-progress="download.uploadProgress.value"
                                    :uploader-status="download.uploaderStatus.value"
                                    @cancel="download.cancel"
                                />
                            </AnimatedReveal>
                            <AnimatedReveal :show="!download.isDownloading.value">
                                <div class="mt-7 flex flex-wrap justify-center gap-3">
                                    <Button
                                        v-if="
                                            download.note.value &&
                                            download.manifest.value.read_token &&
                                            !download.burned.value
                                        "
                                        variant="secondary"
                                        :disabled="download.isBurning.value"
                                        @click="download.burnNote"
                                        >Retry removal</Button
                                    >
                                    <Button
                                        v-if="download.isNote.value"
                                        size="large"
                                        @click="downloadNote"
                                        ><Icon name="download" :size="18" />{{
                                            download.note.value ? 'Download note' : 'Decrypt note'
                                        }}</Button
                                    >
                                    <Button
                                        v-else-if="download.manifest.value.items.length === 1"
                                        size="large"
                                        :variant="
                                            download.downloadedItemIds.value.length
                                                ? 'secondary'
                                                : 'primary'
                                        "
                                        @click="downloadFiles()"
                                        ><Icon name="download" :size="18" />{{
                                            download.downloadedItemIds.value.length
                                                ? 'Download files again'
                                                : 'Download files'
                                        }}</Button
                                    >
                                    <Button
                                        v-for="item in download.manifest.value.items"
                                        v-else
                                        :key="item.id"
                                        variant="secondary"
                                        @click="downloadFiles([item])"
                                        ><Icon name="download" :size="17" />Download {{ item.name
                                        }}{{
                                            download.downloadedItemIds.value.includes(item.id)
                                                ? ' again'
                                                : ''
                                        }}</Button
                                    >
                                </div>
                            </AnimatedReveal>
                        </template>
                        <TransferUnavailable v-else :message="download.error.value" />
                        <AnimatedReveal
                            :show="
                                Boolean(download.error.value && download.state.value !== 'error')
                            "
                        >
                            <p class="mt-5 text-sm text-[var(--fb-danger)]" role="alert">
                                {{ displayError }}
                            </p>
                        </AnimatedReveal>
                        <CliDownloadCard
                            v-if="
                                download.transfer.value &&
                                !['loading', 'error'].includes(download.state.value)
                            "
                            :target="cliTarget"
                            :transfer="{
                                kind: download.transfer.value.kind,
                                driver: download.transfer.value.driver ?? 'http',
                                available:
                                    !download.burned.value &&
                                    download.transfer.value.status !== 'pending' &&
                                    download.transfer.value.status !== 'ended',
                                expiresAt: download.transfer.value.expires_at,
                                inbox: Boolean(inbox),
                                burnOnRead: download.transfer.value.burn_on_read,
                            }"
                            :password-protected="download.passwordRequired.value"
                        />
                    </div>
                </Transition>
            </AnimatedHeight>
        </div>
        <WebRtcConsent ref="webrtcConsentDialog" v-model="download.webrtcConsent.value" />
    </section>
</template>

<style scoped>
.transfer-page {
    width: 100%;
    max-width: 56rem;
    box-sizing: border-box;
    margin-inline: auto;
    padding: 2.75rem 1.25rem 4rem;
}
.transfer-card {
    overflow: hidden;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
}
.transfer-state {
    box-sizing: border-box;
    min-height: 26rem;
    padding: 2rem;
}
.transfer-loading {
    display: flex;
    min-height: 22rem;
    align-items: center;
    flex-direction: column;
    justify-content: center;
    text-align: center;
}
.transfer-loading > span,
.transfer-content__emblem {
    display: grid;
    place-items: center;
    border: 1px solid #78598666;
    color: var(--fb-accent-text);
    background: var(--fb-selected-surface);
}
.transfer-loading > span {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 1rem;
}
.transfer-loading strong {
    margin-top: 1rem;
    font-size: 1rem;
}
.transfer-loading p {
    margin: 0.375rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
}
.transfer-content__header {
    display: grid;
    grid-template-columns: 3.5rem minmax(0, 1fr);
    align-items: start;
    gap: 1rem;
    padding-bottom: 1.5rem;
    border-bottom: 1px solid #ffffff0a;
}
.transfer-content__emblem {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 1rem;
}
.transfer-content__heading h1 {
    overflow-wrap: anywhere;
    margin: 0;
    font-size: 1.75rem;
    font-weight: 600;
    letter-spacing: -0.035em;
}
.transfer-content__meta {
    display: flex;
    min-height: 2rem;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.625rem;
    margin-top: -0.25rem;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
}
.transfer-content__report {
    grid-column: 3;
    grid-row: 1;
}
.transfer-state-enter-active,
.transfer-state-leave-active {
    transition:
        opacity var(--fb-duration-pane) var(--fb-ease),
        transform var(--fb-duration-pane) var(--fb-ease),
        filter var(--fb-duration-pane) var(--fb-ease);
}
.transfer-state-leave-active {
    position: absolute;
    inset: 0;
    width: 100%;
    pointer-events: none;
}
.transfer-state-enter-from {
    opacity: 0;
    filter: blur(3px);
    transform: translateY(18px);
}
.transfer-state-leave-to {
    opacity: 0;
    filter: blur(2px);
    transform: translateY(-12px);
}
@media (min-width: 640px) {
    .transfer-page {
        padding-inline: 2.5rem;
    }
    .transfer-state {
        padding: 2.5rem;
    }
}
@media (max-width: 639px) {
    .transfer-page {
        padding-top: 1.5rem;
    }
    .transfer-state {
        min-height: 22rem;
        padding: 1.25rem;
    }
    .transfer-content__header {
        grid-template-columns: 3rem minmax(0, 1fr);
        gap: 0.75rem;
    }
    .transfer-content__emblem {
        width: 3rem;
        height: 3rem;
    }
    .transfer-content__heading h1 {
        font-size: 1.375rem;
    }
}
@media (prefers-reduced-motion: reduce) {
    .transfer-state-enter-active,
    .transfer-state-leave-active {
        transition: none;
    }
}
</style>
