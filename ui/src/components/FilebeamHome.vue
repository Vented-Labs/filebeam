<script setup lang="ts">
import {
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
    TabsList,
    TabsRoot,
    TabsTrigger,
} from 'reka-ui';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { FilebeamConfig, PublicRecipient, TransferDriver } from '../types';
import { useEncryptedUpload } from '../composables/useEncryptedUpload';
import { ciphertextBytes, formatBytes } from '../lib/format';
import AppShell from './layout/AppShell.vue';
import AnimatedHeight from './layout/AnimatedHeight.vue';
import TrustFeatures from './layout/TrustFeatures.vue';
import NoteComposer from './notes/NoteComposer.vue';
import FilePond from './upload/FilePond.vue';
import FileQueue from './upload/FileQueue.vue';
import TransferOptions from './upload/TransferOptions.vue';
import TransferMethod from './upload/TransferMethod.vue';
import ShareReady from './sharing/ShareReady.vue';
import WebRtcConsent from './sharing/WebRtcConsent.vue';
import Icon from './primitives/Icon.vue';
import AppLink from './primitives/AppLink.vue';
import Toast from './primitives/Toast.vue';
import Button from './primitives/Button.vue';
import SmoothProgress from './primitives/SmoothProgress.vue';
import { enabledTransferDrivers, transferLimits } from '../lib/transfer-policy';

const props = defineProps<{
    config: FilebeamConfig;
    recipient?: PublicRecipient;
    user?: { name: string; username?: string | null } | null;
}>();
const mode = ref<'files' | 'note'>('files');
const note = ref('');
const noteTitle = ref('');
const language = ref('plain');
const filePassword = ref('');
const notePassword = ref('');
const fileIncludeKey = ref(true);
const noteIncludeKey = ref(true);
const fileRetentionHours = ref(props.config.file_retention_hours);
const noteRetentionHours = ref(props.config.note_retention_hours);
const burnOnRead = ref(false);
const enabledDrivers = computed(() =>
    props.recipient
        ? enabledTransferDrivers(props.config).filter((driver) => driver === 'http')
        : enabledTransferDrivers(props.config),
);
const driver = ref<TransferDriver>(
    props.recipient ? 'http' : (props.config.transport_policy?.default_driver ?? 'http'),
);
const webrtcConsent = ref(false);
const webrtcConsentDialog = ref<InstanceType<typeof WebRtcConsent>>();
const restartHttpOpen = ref(false);
let restartHttpTrigger: HTMLElement | undefined;
const webRtcSupported = typeof RTCPeerConnection !== 'undefined';
const dragDepth = ref(0);
const deleting = ref(false);
const transferDeletedToastOpen = ref(false);
const filesUpload = useEncryptedUpload(() => props.config, driver);
const noteUpload = useEncryptedUpload(() => props.config, driver);
const activeUpload = computed(() => (mode.value === 'files' ? filesUpload : noteUpload));
const activeStatus = computed(() => activeUpload.value.status.value);
const activeShare = computed(() => activeUpload.value.share.value);
const activeError = computed(() => activeUpload.value.error.value);
const activeProgress = computed(() => activeUpload.value.progress.value);
const activeActivity = computed(() => activeUpload.value.activity.value);
const activeIsUploading = computed(() => activeUpload.value.isUploading.value);
const activeSessions = computed(() => activeUpload.value.sessions.value);
const activeMonitoringUnavailable = computed(() => activeUpload.value.monitoringUnavailable.value);
const dragging = computed(() => dragDepth.value > 0);
const canCreateTransfers = computed(
    () => Boolean(props.user) || props.config.anonymous_uploads_enabled,
);
const transfersAvailable = computed(
    () => !props.recipient || enabledDrivers.value.includes('http'),
);
const isBusy = computed(
    () => deleting.value || filesUpload.isUploading.value || noteUpload.isUploading.value,
);
const noteCiphertextBytes = computed(() =>
    ciphertextBytes(new Blob([note.value]).size, props.config.chunk_bytes),
);
const limits = computed(() => transferLimits(props.config, driver.value));
const maximumBytes = computed(() =>
    mode.value === 'files' ? limits.value.maximum_transfer_bytes : limits.value.maximum_note_bytes,
);
const activeCiphertextBytes = computed(() =>
    mode.value === 'files' ? filesUpload.totalCiphertextBytes.value : noteCiphertextBytes.value,
);
const hasContent = computed(() =>
    mode.value === 'files' ? filesUpload.entries.value.length > 0 : note.value.length > 0,
);
const canUpload = computed(() => {
    const countAllowed =
        mode.value !== 'files' ||
        limits.value.maximum_file_count === null ||
        filesUpload.entries.value.length <= limits.value.maximum_file_count;
    const bytesAllowed =
        maximumBytes.value === null || activeCiphertextBytes.value <= maximumBytes.value;
    return (
        hasContent.value &&
        bytesAllowed &&
        countAllowed &&
        enabledDrivers.value.includes(driver.value) &&
        (driver.value !== 'webrtc' || webRtcSupported) &&
        !isBusy.value
    );
});
watch(driver, () => {
    webrtcConsent.value = false;
});
const selectedPassword = computed({
    get: () => (mode.value === 'files' ? filePassword.value : notePassword.value),
    set: (value: string) => {
        if (mode.value === 'files') filePassword.value = value;
        else notePassword.value = value;
    },
});
const selectedIncludeKey = computed({
    get: () => (mode.value === 'files' ? fileIncludeKey.value : noteIncludeKey.value),
    set: (value: boolean) => {
        if (mode.value === 'files') fileIncludeKey.value = value;
        else noteIncludeKey.value = value;
    },
});
const selectedRetentionHours = computed({
    get: () => (mode.value === 'files' ? fileRetentionHours.value : noteRetentionHours.value),
    set: (value: number) => {
        if (mode.value === 'files') fileRetentionHours.value = value;
        else noteRetentionHours.value = value;
    },
});

function chooseFiles(): void {
    document.getElementById('filebeam-picker')?.click();
}
function goToFiles(): void {
    if (isBusy.value) return;
    resetDragging();
    mode.value = 'files';
}
function addFiles(files: FileList): void {
    resetDragging();
    filesUpload.addFiles(files);
}
function hasDraggedFiles(event: DragEvent): boolean {
    return Array.from(event.dataTransfer?.types ?? []).includes('Files');
}
function resetDragging(): void {
    dragDepth.value = 0;
}
function onDragEnter(event: DragEvent): void {
    if (
        canCreateTransfers.value &&
        mode.value === 'files' &&
        !isBusy.value &&
        hasDraggedFiles(event)
    ) {
        event.preventDefault();
        dragDepth.value++;
    }
}
function onDragLeave(event: DragEvent): void {
    if (hasDraggedFiles(event)) dragDepth.value = Math.max(0, dragDepth.value - 1);
}
function onDrop(event: DragEvent): void {
    if (!canCreateTransfers.value || !hasDraggedFiles(event)) return;
    event.preventDefault();
    resetDragging();
    if (mode.value === 'files' && !isBusy.value && event.dataTransfer?.files)
        filesUpload.addFiles(event.dataTransfer.files);
}
async function submit(turbo = false): Promise<void> {
    if (driver.value === 'webrtc') {
        if (!webRtcSupported) {
            activeUpload.value.error.value = 'WebRTC is not supported by this browser.';
            return;
        }
        if (!(await webrtcConsentDialog.value?.requestConsent())) return;
        if (driver.value !== 'webrtc' || !canUpload.value) return;
    }
    const uploader = activeUpload.value;
    await uploader.upload({
        mode: mode.value,
        note: note.value,
        title: noteTitle.value,
        language: language.value,
        password: selectedPassword.value,
        includeKey: selectedIncludeKey.value,
        retentionHours:
            mode.value === 'files' ? fileRetentionHours.value : noteRetentionHours.value,
        burnOnRead: mode.value === 'note' && burnOnRead.value,
        recipient: props.recipient,
        turbo,
        webrtcConsent: webrtcConsent.value,
    });
}
async function removeTransfer(): Promise<void> {
    const share = activeShare.value;
    if (!share || isBusy.value) return;
    transferDeletedToastOpen.value = false;
    deleting.value = true;
    try {
        const response = await fetch(`/api/v1/transfers/${share.transferId}`, {
            method: 'DELETE',
            headers: { 'X-Filebeam-Delete-Token': share.deleteToken },
        });
        if (!response.ok) throw new Error('Could not delete this transfer. Try again.');
        deleting.value = false;
        resetActive();
        transferDeletedToastOpen.value = true;
    } catch (reason) {
        activeUpload.value.error.value =
            reason instanceof Error ? reason.message : 'Could not delete this transfer. Try again.';
    } finally {
        deleting.value = false;
    }
}
function resetActive(): void {
    if (isBusy.value) return;
    activeUpload.value.clear();
    driver.value = props.recipient
        ? 'http'
        : (props.config.transport_policy?.default_driver ?? 'http');
    webrtcConsent.value = false;
    if (mode.value === 'files') {
        filePassword.value = '';
        fileIncludeKey.value = true;
        fileRetentionHours.value = props.config.file_retention_hours;
        return;
    }
    note.value = '';
    noteTitle.value = '';
    language.value = 'plain';
    notePassword.value = '';
    noteIncludeKey.value = true;
    noteRetentionHours.value = props.config.note_retention_hours;
    burnOnRead.value = false;
}
function cancelUpload(): void {
    activeUpload.value.cancel();
}
async function restartHttp(confirmed = false): Promise<void> {
    if (confirmed && !restartHttpOpen.value) return;
    if (activeShare.value?.driver !== 'webrtc' || !enabledDrivers.value.includes('http')) {
        restartHttpOpen.value = false;
        return;
    }
    const httpLimits = transferLimits(props.config, 'http');
    const httpMaximumBytes =
        mode.value === 'files' ? httpLimits.maximum_transfer_bytes : httpLimits.maximum_note_bytes;
    const exceedsBytes =
        httpMaximumBytes !== null && activeCiphertextBytes.value > httpMaximumBytes;
    const exceedsFiles =
        mode.value === 'files' &&
        httpLimits.maximum_file_count !== null &&
        filesUpload.entries.value.length > httpLimits.maximum_file_count;
    if (exceedsBytes || exceedsFiles) {
        restartHttpOpen.value = false;
        activeUpload.value.error.value =
            'This live transfer exceeds the stored HTTP limits and cannot be restarted as HTTP.';
        return;
    }
    if (!confirmed) {
        restartHttpTrigger =
            document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
        restartHttpOpen.value = true;
        return;
    }
    restartHttpOpen.value = false;
    activeUpload.value.cancel();
    driver.value = 'http';
    webrtcConsent.value = false;
    await submit();
}
function restoreRestartFocus(event: Event): void {
    event.preventDefault();
    restartHttpTrigger?.focus();
}
function preventFileNavigation(event: DragEvent): void {
    if (canCreateTransfers.value && hasDraggedFiles(event)) event.preventDefault();
}
function resetDraggingOnHidden(): void {
    if (document.visibilityState !== 'visible') resetDragging();
}
onMounted(() => {
    window.addEventListener('dragenter', onDragEnter);
    window.addEventListener('dragleave', onDragLeave);
    window.addEventListener('dragover', preventFileNavigation);
    window.addEventListener('drop', onDrop);
    window.addEventListener('dragend', resetDragging);
    window.addEventListener('blur', resetDragging);
    document.addEventListener('visibilitychange', resetDraggingOnHidden);
});
onBeforeUnmount(() => {
    window.removeEventListener('dragenter', onDragEnter);
    window.removeEventListener('dragleave', onDragLeave);
    window.removeEventListener('dragover', preventFileNavigation);
    window.removeEventListener('drop', onDrop);
    window.removeEventListener('dragend', resetDragging);
    window.removeEventListener('blur', resetDragging);
    document.removeEventListener('visibilitychange', resetDraggingOnHidden);
});
</script>

<template>
    <AppShell
        :github-url="config.github_url"
        :copyright-holder="config.copyright_holder"
        :user="user"
        :home-action="recipient ? undefined : goToFiles"
        :registration-enabled="config.registration_enabled"
    >
        <section class="mx-auto w-full max-w-[1370px] px-5 pb-14 pt-8 sm:px-10">
            <section
                v-if="!transfersAvailable"
                class="mx-auto max-w-xl rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] px-6 py-12 text-center shadow-2xl shadow-black/30"
            >
                <h1 class="text-2xl font-semibold text-[var(--fb-text)]">
                    Stored transfers unavailable
                </h1>
                <p class="mt-3 text-[var(--fb-text-muted)]">
                    This recipient inbox accepts stored HTTP transfers, but HTTP is disabled by the
                    server.
                </p>
            </section>
            <section
                v-else-if="!canCreateTransfers"
                class="mx-auto max-w-xl rounded-2xl border border-[var(--fb-border)] bg-[var(--fb-surface)] px-6 py-12 text-center shadow-2xl shadow-black/30"
            >
                <h1 class="text-2xl font-semibold text-[var(--fb-text)]">Sign in to share files</h1>
                <p class="mt-3 text-[var(--fb-text-muted)]">
                    File and note sharing is available to account holders.
                </p>
                <div class="mt-6 flex justify-center gap-3">
                    <AppLink class="fb-button fb-button--primary" href="/login">Sign in</AppLink>
                    <AppLink
                        v-if="config.registration_enabled"
                        class="fb-button fb-button--secondary"
                        href="/register"
                        >Register</AppLink
                    >
                </div>
            </section>
            <template v-else>
                <header v-if="recipient" class="mx-auto mb-8 max-w-2xl text-center">
                    <h1
                        class="break-words text-3xl font-semibold text-[var(--fb-text)] sm:text-4xl"
                    >
                        Send files to @{{ recipient.username }}
                    </h1>
                    <p class="mt-4 text-[var(--fb-text-muted)]">
                        Files are encrypted in your browser for this recipient. Their private key is
                        required to open them.
                    </p>
                    <details class="mt-4 text-xs text-[var(--fb-text-muted)]">
                        <summary class="cursor-pointer">Recipient key fingerprint</summary>
                        <p class="mt-2 break-all font-mono">{{ recipient.fingerprint }}</p>
                    </details>
                </header>
                <TabsRoot v-else v-model="mode" class="mx-auto mb-5 block w-fit">
                    <TabsList
                        class="mode-tabs flex rounded-[1.5rem] border border-[var(--fb-border)] bg-[var(--fb-surface)] p-1.5"
                        aria-label="Transfer type"
                    >
                        <TabsTrigger
                            value="files"
                            :disabled="isBusy"
                            class="flex items-center gap-2 rounded-xl px-8 py-3 font-medium text-[var(--fb-text-muted)] transition data-[state=active]:bg-[var(--fb-action)] data-[state=active]:text-[var(--fb-on-action)]"
                            ><Icon name="folder" :size="19" />Files</TabsTrigger
                        >
                        <TabsTrigger
                            value="note"
                            :disabled="isBusy"
                            class="flex items-center gap-2 rounded-xl px-8 py-3 font-medium text-[var(--fb-text-muted)] transition data-[state=active]:bg-[var(--fb-action)] data-[state=active]:text-[var(--fb-on-action)]"
                            ><Icon name="note" :size="19" />Notes</TabsTrigger
                        >
                    </TabsList>
                </TabsRoot>
                <AnimatedHeight>
                    <Transition name="content-switch" mode="out-in">
                        <div
                            :key="`${activeShare && !recipient ? 'share' : activeStatus === 'complete' ? 'complete' : 'composer'}-${mode}`"
                        >
                            <template v-if="activeShare && !recipient">
                                <ShareReady
                                    :share="activeShare"
                                    :deleting="deleting"
                                    :uploading="activeIsUploading"
                                    :progress="activeProgress"
                                    :activity="activeActivity"
                                    :sessions="activeSessions"
                                    :monitoring-unavailable="activeMonitoringUnavailable"
                                    :can-restart-http="
                                        activeShare.driver === 'webrtc' &&
                                        enabledDrivers.includes('http')
                                    "
                                    @reset="resetActive"
                                    @delete="removeTransfer"
                                    @cancel="cancelUpload"
                                    @restart-http="restartHttp"
                                />
                                <p
                                    v-if="activeError"
                                    class="mt-3 text-center text-sm text-[var(--fb-danger)]"
                                    aria-live="polite"
                                >
                                    {{ activeError }}
                                </p>
                            </template>
                            <section
                                v-else-if="activeStatus === 'complete' && recipient"
                                class="mx-auto max-w-xl rounded-3xl border border-[var(--fb-border)] bg-[var(--fb-surface)] p-8 text-center sm:p-12"
                            >
                                <Icon
                                    name="check"
                                    :size="36"
                                    class="mx-auto text-[var(--fb-success)]"
                                />
                                <h2 class="mt-5 text-3xl font-semibold">Files sent</h2>
                                <p class="mt-3 text-[var(--fb-text-muted)]">
                                    Your encrypted files are now in @{{ recipient.username }}'s
                                    inbox.
                                </p>
                                <Button class="mt-6" @click="resetActive">Send more files</Button>
                            </section>
                            <template v-else>
                                <div
                                    v-if="mode === 'files'"
                                    class="files-layout grid items-start gap-5"
                                    :class="{
                                        'files-layout--queued': filesUpload.entries.value.length,
                                    }"
                                >
                                    <FilePond
                                        :disabled="isBusy"
                                        :dragging="dragging"
                                        :compact="filesUpload.entries.value.length > 0"
                                        :maximum-files="limits.maximum_file_count"
                                        :maximum-bytes="limits.maximum_transfer_bytes"
                                        @choose="chooseFiles"
                                        @files="addFiles"
                                    >
                                        <template #transfer-method>
                                            <TransferMethod
                                                v-model="driver"
                                                class="mt-8 text-left"
                                                :enabled-drivers="enabledDrivers"
                                                :web-rtc-supported="webRtcSupported"
                                                :disabled="isBusy"
                                                :limits="limits"
                                                mode="files"
                                            />
                                        </template>
                                    </FilePond>
                                    <div class="min-w-0 overflow-hidden">
                                        <Transition name="queue-panel"
                                            ><FileQueue
                                                v-if="filesUpload.entries.value.length"
                                                :entries="filesUpload.entries.value"
                                                :disabled="isBusy"
                                                @choose="chooseFiles"
                                                @remove="filesUpload.removeFile"
                                        /></Transition>
                                    </div>
                                </div>
                                <section v-else class="mx-auto w-full max-w-[896px]">
                                    <NoteComposer
                                        v-model="note"
                                        v-model:title="noteTitle"
                                        v-model:language="language"
                                        :disabled="isBusy"
                                        :maximum-bytes="limits.maximum_note_bytes"
                                        :retention-hours="noteRetentionHours"
                                    />
                                </section>
                                <Transition name="queue-panel"
                                    ><TransferOptions
                                        v-if="hasContent || mode === 'note'"
                                        v-model:password="selectedPassword"
                                        v-model:include-key="selectedIncludeKey"
                                        v-model:retention-hours="selectedRetentionHours"
                                        v-model:burn-on-read="burnOnRead"
                                        v-model:driver="driver"
                                        :disabled="isBusy"
                                        :can-upload="canUpload"
                                        :uploading="activeIsUploading"
                                        :mode="mode"
                                        :recipient="Boolean(recipient)"
                                        :enabled-drivers="enabledDrivers"
                                        :web-rtc-supported="webRtcSupported"
                                        :limits="limits"
                                        :retention-options="
                                            mode === 'files'
                                                ? (config.file_retention_options ?? [])
                                                : (config.note_retention_options ?? [])
                                        "
                                        @submit="submit()"
                                        @turbo="submit(true)"
                                        @cancel="cancelUpload"
                                /></Transition>
                                <div class="mt-3 min-h-6 text-center" aria-live="polite">
                                    <SmoothProgress
                                        v-if="activeIsUploading"
                                        class="mx-auto max-w-md"
                                        :value="activeProgress"
                                        label="Encrypting and uploading"
                                    >
                                        <template #label="{ percentage }">
                                            <p class="mb-2 text-sm text-[var(--fb-text-muted)]">
                                                {{ activeActivity }}
                                                <span class="tabular-nums">{{ percentage }}%</span>
                                            </p>
                                        </template>
                                    </SmoothProgress>
                                    <p
                                        v-else-if="activeError"
                                        class="text-sm text-[var(--fb-danger)]"
                                    >
                                        {{ activeError }}
                                    </p>
                                    <p
                                        v-else-if="
                                            hasContent &&
                                            maximumBytes !== null &&
                                            activeCiphertextBytes > maximumBytes
                                        "
                                        class="text-sm text-[var(--fb-danger)]"
                                    >
                                        This encrypted {{ driver === 'webrtc' ? 'WebRTC' : 'HTTP' }}
                                        {{ mode === 'files' ? 'transfer' : 'note' }} exceeds the
                                        {{ formatBytes(maximumBytes) }} limit.
                                    </p>
                                    <p
                                        v-else-if="
                                            mode === 'files' &&
                                            limits.maximum_file_count !== null &&
                                            filesUpload.entries.value.length >
                                                limits.maximum_file_count
                                        "
                                        class="text-sm text-[var(--fb-danger)]"
                                    >
                                        This transfer exceeds the
                                        {{ limits.maximum_file_count }} file limit.
                                    </p>
                                </div>
                            </template>
                        </div>
                    </Transition>
                </AnimatedHeight>
            </template>
            <TrustFeatures class="mt-14" />
        </section>
        <Toast
            v-model:open="transferDeletedToastOpen"
            title="Transfer deleted"
            description="The sharing link is no longer available."
        />
        <WebRtcConsent ref="webrtcConsentDialog" v-model="webrtcConsent" />
        <DialogRoot v-model:open="restartHttpOpen">
            <DialogPortal>
                <DialogOverlay class="fb-dialog__overlay" />
                <DialogContent class="fb-dialog__content" @close-auto-focus="restoreRestartFocus">
                    <DialogTitle class="fb-dialog__title">Restart as stored HTTP?</DialogTitle>
                    <DialogDescription class="fb-dialog__description">
                        This creates a new link and applies HTTP limits. Your encrypted content will
                        be stored on the server, and the current live share will end.
                    </DialogDescription>
                    <div class="mt-6 flex flex-wrap justify-end gap-3">
                        <DialogClose as-child><Button variant="ghost">Cancel</Button></DialogClose>
                        <Button @click="restartHttp(true)">Restart with HTTP</Button>
                    </div>
                    <DialogClose class="fb-dialog__close" aria-label="Close restart confirmation">
                        <Icon name="x" :size="18" />
                    </DialogClose>
                </DialogContent>
            </DialogPortal>
        </DialogRoot>
    </AppShell>
</template>

<style scoped>
.files-layout {
    grid-template-columns: minmax(0, 1fr);
    gap: 0;
}
.files-layout--queued {
    gap: 1.25rem;
}
.mode-tabs {
    width: min(25rem, calc(100vw - 2.5rem));
}
.mode-tabs > button {
    border-radius: 17px;
    flex: 1;
    justify-content: center;
}
.queue-panel-enter-active,
.queue-panel-leave-active {
    transition:
        opacity 0.2s ease,
        transform 0.2s ease;
}
.queue-panel-enter-from,
.queue-panel-leave-to {
    opacity: 0;
    transform: translateX(10px);
}
.content-switch-enter-active,
.content-switch-leave-active {
    transition:
        opacity 0.18s ease,
        transform 0.18s ease;
}
.content-switch-enter-from,
.content-switch-leave-to {
    opacity: 0;
    transform: translateY(6px);
}
@media (min-width: 1024px) {
    .files-layout {
        grid-template-columns: minmax(0, 1fr) minmax(0, 0fr);
        transition:
            grid-template-columns 0.28s ease,
            gap 0.28s ease;
    }
    .files-layout--queued {
        grid-template-columns: minmax(0, 1fr) minmax(0, 0.58fr);
    }
}
button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
}
@media (prefers-reduced-motion: reduce) {
    * {
        transition-duration: 0s !important;
    }
}
</style>
