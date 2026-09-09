<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { cancelAccountCrypto, openRecipientKey } from '../lib/account-crypto';
import type { FilebeamConfig } from '../types';
import { useEncryptedDownload } from '../composables/useEncryptedDownload';
import AppShell from './layout/AppShell.vue';
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

const props = defineProps<{
    transferId: string;
    inbox?: boolean;
    config: FilebeamConfig;
    user?: { name: string; username?: string | null } | null;
}>();
const download = useEncryptedDownload(props.transferId, props.inbox);
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
    disposed = true;
    secret.value = '';
    cancelAccountCrypto();
});

onMounted(() => {
    void download.load();
});
</script>

<template>
    <AppShell
        :github-url="config.github_url"
        :copyright-holder="config.copyright_holder"
        :user="user"
        :registration-enabled="config.registration_enabled"
    >
        <section class="mx-auto w-full max-w-4xl px-5 pb-14 pt-10 sm:px-10 lg:pt-14">
            <AppLink v-if="inbox" href="/account/inbox" class="fb-text-link mb-5 inline-block"
                >Back to inbox</AppLink
            >
            <div
                class="rounded-3xl border border-[var(--fb-border)] bg-[var(--fb-surface)] p-6 shadow-2xl sm:p-10"
            >
                <div
                    v-if="download.state.value === 'loading'"
                    class="py-20 text-center text-[var(--fb-text-muted)]"
                >
                    <Icon name="loader" class="mx-auto mb-3" />
                    <p>Loading encrypted transfer...</p>
                </div>
                <form
                    v-else-if="download.state.value === 'key' && inbox"
                    class="mx-auto max-w-md space-y-5"
                    @submit.prevent="unlockRecipient"
                >
                    <Icon name="lock" :size="32" />
                    <h1 class="text-3xl font-semibold">Unlock received files</h1>
                    <p class="text-sm text-[var(--fb-text-muted)]">
                        Decryption happens in this browser. Your unlock secret is not sent to
                        Filebeam.
                    </p>
                    <label for="inbox-secret" class="block text-sm font-medium">{{
                        download.transfer.value?.recipient_key?.bundle.custody_mode === 'password'
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
                        Key version {{ download.transfer.value?.recipient_key?.bundle.version }}.
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
                    <h1 class="break-words text-3xl font-semibold">
                        {{
                            download.isNote.value
                                ? download.manifest.value.title || 'Secure note'
                                : fileHeading
                        }}
                    </h1>
                    <div class="mt-3 flex min-h-6 items-center gap-2" data-testid="transfer-meta">
                        <span
                            v-if="download.webrtc.value"
                            class="text-sm text-[var(--fb-text-muted)]"
                        >
                            WebRTC live transfer. The sender must keep their browser open.
                        </span>
                        <span
                            v-if="
                                !download.webrtc.value &&
                                download.turbo.value &&
                                download.uploaderStatus.value !== 'completed'
                            "
                            class="text-sm text-[var(--fb-text-muted)]"
                        >
                            Retention starts when uploading finishes.
                        </span>
                        <TransferExpiry
                            v-else-if="!download.webrtc.value"
                            :expires-at="download.transfer.value.expires_at"
                        />
                        <slot name="report" :transfer-id="transferId" />
                    </div>
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
                        class="mt-7"
                        :content="download.note.value"
                        :language="download.manifest.value.language || 'plain'"
                    />
                    <DownloadList
                        v-else-if="!download.isNote.value"
                        class="mt-7"
                        :items="download.manifest.value.items"
                    />
                    <p
                        v-if="download.turbo.value && !download.webrtc.value"
                        class="mt-5 text-sm text-[var(--fb-text-muted)]"
                    >
                        Your download progress is shared anonymously with the sender while this
                        transfer is active.
                    </p>
                    <DownloadStatus
                        v-if="
                            (download.turbo.value && !download.webrtc.value) ||
                            download.isDownloading.value
                        "
                        :progress="download.progress.value"
                        :downloading="download.isDownloading.value"
                        :phase="download.downloadPhase.value"
                        :turbo="download.turbo.value"
                        :upload-progress="download.uploadProgress.value"
                        :uploader-status="download.uploaderStatus.value"
                        @cancel="download.cancel"
                    />
                    <div
                        v-if="!download.isDownloading.value"
                        class="mt-7 flex flex-wrap justify-center gap-3"
                    >
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
                        <Button v-if="download.isNote.value" size="large" @click="downloadNote"
                            ><Icon name="download" :size="18" />{{
                                download.note.value ? 'Download note' : 'Decrypt note'
                            }}</Button
                        >
                        <Button
                            v-else-if="download.manifest.value.items.length === 1"
                            size="large"
                            :variant="
                                download.downloadedItemIds.value.length ? 'secondary' : 'primary'
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
                                download.downloadedItemIds.value.includes(item.id) ? ' again' : ''
                            }}</Button
                        >
                    </div>
                </template>
                <TransferUnavailable v-else :message="download.error.value" />
                <p
                    v-if="download.error.value && download.state.value !== 'error'"
                    class="mt-5 text-sm text-[var(--fb-danger)]"
                    role="alert"
                >
                    {{ displayError }}
                </p>
            </div>
            <WebRtcConsent ref="webrtcConsentDialog" v-model="download.webrtcConsent.value" />
        </section>
    </AppShell>
</template>

<style scoped>
@media (prefers-reduced-motion: reduce) {
    * {
        transition-duration: 0s !important;
    }
}
</style>
