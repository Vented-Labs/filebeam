<script setup lang="ts">
import {
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogOverlay,
    DialogPortal,
    DialogRoot,
    DialogTitle,
} from 'reka-ui';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { FilebeamConfig, PublicRecipient, TransferDriver } from '../types';
import { useEncryptedUpload } from '../composables/useEncryptedUpload';
import { ciphertextBytes, formatBytes } from '../lib/format';
import AnimatedHeight from './layout/AnimatedHeight.vue';
import AnimatedReveal from './layout/AnimatedReveal.vue';
import TrustFeatures from './layout/TrustFeatures.vue';
import NoteComposer from './notes/NoteComposer.vue';
import FilePond from './upload/FilePond.vue';
import FileQueue from './upload/FileQueue.vue';
import TransferOptions from './upload/TransferOptions.vue';
import TransferMethod from './upload/TransferMethod.vue';
import TransferModeTabs from './upload/TransferModeTabs.vue';
import ShareReady from './sharing/ShareReady.vue';
import WebRtcConsent from './sharing/WebRtcConsent.vue';
import Icon from './primitives/Icon.vue';
import AuthLink from './auth/AuthLink.vue';
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
const defaultDriver: TransferDriver = props.recipient
    ? 'http'
    : (props.config.transport_policy?.default_driver ?? 'http');
const driver = ref<TransferDriver>(
    enabledDrivers.value.includes(defaultDriver)
        ? defaultDriver
        : (enabledDrivers.value[0] ?? 'http'),
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
const activePassword = computed(() =>
    mode.value === 'files' ? filePassword.value : notePassword.value,
);
const passwordValid = computed(
    () => activePassword.value.length === 0 || Array.from(activePassword.value).length >= 8,
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
        passwordValid.value &&
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
const exceedsCurrentBytes = computed(
    () => maximumBytes.value !== null && activeCiphertextBytes.value > maximumBytes.value,
);
const exceedsCurrentCount = computed(
    () =>
        mode.value === 'files' &&
        limits.value.maximum_file_count !== null &&
        filesUpload.entries.value.length > limits.value.maximum_file_count,
);
const policyError = computed(() => {
    if (!hasContent.value) return '';
    if (exceedsCurrentBytes.value)
        return `This encrypted ${driver.value === 'webrtc' ? 'WebRTC' : 'HTTP'} ${mode.value === 'files' ? 'transfer' : 'note'} exceeds the ${formatBytes(maximumBytes.value!)} limit.`;
    if (exceedsCurrentCount.value)
        return `This transfer exceeds the ${limits.value.maximum_file_count} file limit.`;
    return '';
});
const canUseWebRtcRecovery = computed(() => {
    if (driver.value !== 'http' || !policyError.value) return false;
    if (!enabledDrivers.value.includes('webrtc') || !webRtcSupported) return false;
    const webRtcLimits = transferLimits(props.config, 'webrtc');
    const maximum =
        mode.value === 'files'
            ? webRtcLimits.maximum_transfer_bytes
            : webRtcLimits.maximum_note_bytes;
    const countAllowed =
        mode.value !== 'files' ||
        webRtcLimits.maximum_file_count === null ||
        filesUpload.entries.value.length <= webRtcLimits.maximum_file_count;
    return (maximum === null || activeCiphertextBytes.value <= maximum) && countAllowed;
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
    driver.value = enabledDrivers.value.includes(defaultDriver)
        ? defaultDriver
        : (enabledDrivers.value[0] ?? 'http');
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
function hideOutgoing(element: Element): void {
    const pane = element as HTMLElement;
    pane.inert = true;
    pane.setAttribute('aria-hidden', 'true');
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
    window.addEventListener('filebeam:home', goToFiles);
});
onBeforeUnmount(() => {
    window.removeEventListener('dragenter', onDragEnter);
    window.removeEventListener('dragleave', onDragLeave);
    window.removeEventListener('dragover', preventFileNavigation);
    window.removeEventListener('drop', onDrop);
    window.removeEventListener('dragend', resetDragging);
    window.removeEventListener('blur', resetDragging);
    document.removeEventListener('visibilitychange', resetDraggingOnHidden);
    window.removeEventListener('filebeam:home', goToFiles);
});
</script>

<template>
    <section class="prism-page mx-auto w-full px-5 pb-14 pt-7 sm:px-8">
        <section v-if="!transfersAvailable" class="prism-gate">
            <h1>Stored transfers unavailable</h1>
            <p>
                This recipient inbox accepts stored HTTP transfers, but HTTP is disabled by the
                server.
            </p>
        </section>
        <section v-else-if="!canCreateTransfers" class="prism-gate">
            <h1>Sign in to share files</h1>
            <p>File and note sharing is available to account holders.</p>
            <div class="prism-gate__actions">
                <AuthLink class="fb-button fb-button--primary" href="/login">Sign in</AuthLink>
                <AuthLink
                    v-if="config.registration_enabled"
                    class="fb-button fb-button--secondary"
                    href="/register"
                    >Register</AuthLink
                >
            </div>
        </section>
        <template v-else>
            <header v-if="recipient" class="prism-recipient-heading">
                <h1>Send files to @{{ recipient.username }}</h1>
                <p>
                    Files are encrypted in your browser for this recipient. Their private key is
                    required to open them.
                </p>
                <details>
                    <summary>Recipient key fingerprint</summary>
                    <p>{{ recipient.fingerprint }}</p>
                </details>
            </header>
            <TransferModeTabs v-else v-model="mode" :disabled="isBusy" />

            <section class="prism-composer" data-testid="prism-composer">
                <TransferMethod
                    v-model="driver"
                    :enabled-drivers="enabledDrivers"
                    :web-rtc-supported="webRtcSupported"
                    :disabled="isBusy"
                    :limits="limits"
                    :mode="mode"
                />
                <AnimatedHeight class="prism-stage-height">
                    <div class="prism-stage" data-testid="prism-stage">
                        <Transition name="prism-card" @before-leave="hideOutgoing">
                            <div
                                v-if="activeShare && !recipient"
                                class="prism-stage__pane prism-stage__pane--result"
                            >
                                <ShareReady
                                    :share="activeShare"
                                    :mode="mode"
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
                                <AnimatedReveal :show="Boolean(activeError)">
                                    <p class="prism-result-error" aria-live="polite">
                                        {{ activeError }}
                                    </p>
                                </AnimatedReveal>
                            </div>
                        </Transition>
                        <Transition name="prism-card" @before-leave="hideOutgoing">
                            <section
                                v-if="activeStatus === 'complete' && recipient"
                                class="prism-stage__pane prism-recipient-complete"
                            >
                                <Icon
                                    name="check"
                                    :size="36"
                                    class="mx-auto text-[var(--fb-success)]"
                                />
                                <h2>Files sent</h2>
                                <p>
                                    Your encrypted files are now in @{{ recipient.username }}'s
                                    inbox.
                                </p>
                                <Button @click="resetActive">Send more files</Button>
                            </section>
                        </Transition>
                        <div
                            v-show="!activeShare && !(activeStatus === 'complete' && recipient)"
                            class="prism-stage__pane prism-stage__pane--editor"
                            :inert="
                                Boolean(
                                    activeShare || (activeStatus === 'complete' && recipient),
                                ) || undefined
                            "
                            :aria-hidden="
                                Boolean(
                                    activeShare || (activeStatus === 'complete' && recipient),
                                ) || undefined
                            "
                        >
                            <div class="prism-mode-stage">
                                <Transition name="prism-mode-card">
                                    <div
                                        v-show="mode === 'files'"
                                        class="prism-mode-pane"
                                        :inert="mode !== 'files' || undefined"
                                        :aria-hidden="mode !== 'files' || undefined"
                                    >
                                        <FilePond
                                            :disabled="isBusy"
                                            :dragging="dragging"
                                            :compact="filesUpload.entries.value.length > 0"
                                            @choose="chooseFiles"
                                            @files="addFiles"
                                        >
                                            <FileQueue
                                                :entries="filesUpload.entries.value"
                                                :disabled="isBusy"
                                                @choose="chooseFiles"
                                                @remove="filesUpload.removeFile"
                                            />
                                        </FilePond>
                                    </div>
                                </Transition>
                                <Transition name="prism-mode-card">
                                    <div
                                        v-show="mode === 'note'"
                                        class="prism-mode-pane"
                                        :inert="mode !== 'note' || undefined"
                                        :aria-hidden="mode !== 'note' || undefined"
                                    >
                                        <NoteComposer
                                            v-model="note"
                                            v-model:title="noteTitle"
                                            v-model:language="language"
                                            :disabled="isBusy"
                                            :maximum-bytes="limits.maximum_note_bytes"
                                            :retention-hours="noteRetentionHours"
                                        />
                                    </div>
                                </Transition>
                            </div>
                            <AnimatedReveal
                                :show="Boolean(activeIsUploading || activeError || policyError)"
                            >
                                <div
                                    class="prism-inline-status"
                                    :class="{
                                        'prism-inline-status--error': activeError || policyError,
                                    }"
                                    :data-testid="policyError ? 'prism-policy-error' : undefined"
                                    aria-live="polite"
                                >
                                    <SmoothProgress
                                        v-if="activeIsUploading"
                                        class="prism-inline-status__progress"
                                        :value="activeProgress"
                                        label="Encrypting and uploading"
                                    >
                                        <template #label="{ percentage }">
                                            <p>
                                                {{ activeActivity }}
                                                <span class="tabular-nums">{{ percentage }}%</span>
                                            </p>
                                        </template>
                                    </SmoothProgress>
                                    <template v-else>
                                        <Icon name="alert" :size="16" />
                                        <span>{{ activeError || policyError }}</span>
                                        <Button
                                            v-if="canUseWebRtcRecovery"
                                            variant="secondary"
                                            @click="driver = 'webrtc'"
                                        >
                                            Use WebRTC
                                        </Button>
                                    </template>
                                </div>
                            </AnimatedReveal>
                        </div>
                    </div>
                </AnimatedHeight>
                <TransferOptions
                    v-if="!activeShare && !(activeStatus === 'complete' && recipient)"
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
                    :retention-options="
                        mode === 'files'
                            ? (config.file_retention_options ?? [])
                            : (config.note_retention_options ?? [])
                    "
                    @submit="submit()"
                    @turbo="submit(true)"
                    @cancel="cancelUpload"
                />
            </section>
        </template>
        <TrustFeatures class="prism-trust" />
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
    </section>
</template>

<style scoped>
.prism-page {
    max-width: 74.5rem;
}
.prism-gate {
    max-width: 36rem;
    margin-inline: auto;
    padding: 3rem 1.5rem;
    border: 1px solid var(--fb-border);
    border-radius: 1.25rem;
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
    text-align: center;
}
.prism-gate h1,
.prism-recipient-heading h1 {
    margin: 0;
    color: var(--fb-text);
    font-size: 1.75rem;
    font-weight: 600;
    letter-spacing: -0.035em;
}
.prism-gate p,
.prism-recipient-heading > p {
    margin: 0.75rem 0 0;
    color: var(--fb-text-muted);
}
.prism-gate__actions {
    display: flex;
    justify-content: center;
    gap: 0.75rem;
    margin-top: 1.5rem;
}
.prism-recipient-heading {
    max-width: 42rem;
    margin: 0 auto 1.5rem;
    text-align: center;
}
.prism-recipient-heading details {
    margin-top: 1rem;
    color: var(--fb-text-subtle);
    font-size: 0.75rem;
}
.prism-recipient-heading summary {
    cursor: pointer;
}
.prism-recipient-heading details p {
    margin-top: 0.5rem;
    overflow-wrap: anywhere;
    font-family: var(--fb-font-code);
}
.prism-composer {
    overflow: clip;
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-panel);
    background: var(--fb-surface);
    box-shadow: var(--fb-shadow-panel);
    isolation: isolate;
}
.prism-stage-height,
.prism-stage {
    min-width: 0;
}
.prism-stage {
    position: relative;
    display: grid;
    isolation: isolate;
    overflow: clip;
}
.prism-stage__pane,
.prism-mode-pane {
    min-width: 0;
}
.prism-stage__pane {
    grid-area: 1 / 1;
}
.prism-stage__pane--editor {
    position: relative;
    z-index: 1;
}
.prism-stage__pane--result,
.prism-recipient-complete {
    position: relative;
    z-index: 2;
    background: var(--fb-surface);
}
.prism-recipient-complete h2 {
    margin: 1.25rem 0 0;
    font-size: 1.875rem;
    font-weight: 600;
}
.prism-recipient-complete p {
    margin: 0.75rem 0 0;
    color: var(--fb-text-muted);
}
.prism-recipient-complete .fb-button {
    margin-top: 1.5rem;
}
.prism-result-error {
    margin: 0 1.5rem 1rem;
    color: var(--fb-danger);
    font-size: 0.8125rem;
    text-align: center;
}
.prism-recipient-complete {
    padding: 3rem 2rem;
    text-align: center;
}
.prism-mode-stage {
    display: grid;
    min-width: 0;
}
.prism-mode-pane {
    grid-area: 1 / 1;
    background: var(--fb-surface);
}
.prism-inline-status {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin: 0 1.5rem 0.9375rem;
    padding: 0.75rem;
    border: 1px solid #ffffff08;
    border-radius: 0.625rem;
    color: var(--fb-text-muted);
    background: #ffffff03;
    font-size: 0.75rem;
}
.prism-inline-status--error {
    border-color: #ad71813d;
    color: var(--fb-danger);
    background: #38252b55;
}
.prism-inline-status > span {
    min-width: 0;
    flex: 1;
}
.prism-inline-status .fb-button {
    min-height: 2.125rem;
    flex: none;
    font-size: 0.6875rem;
}
.prism-inline-status__progress {
    width: 100%;
}
.prism-inline-status__progress p {
    display: flex;
    justify-content: space-between;
    margin: 0 0 0.5rem;
}
.prism-trust {
    margin-top: 1.75rem;
    padding: 0 0.375rem 1.6875rem;
}
.prism-card-enter-active,
.prism-card-leave-active,
.prism-mode-card-enter-active,
.prism-mode-card-leave-active {
    transition:
        opacity var(--fb-duration-pane) var(--fb-ease),
        transform var(--fb-duration-pane) var(--fb-ease);
}
.prism-card-leave-active,
.prism-mode-card-leave-active {
    position: absolute;
    inset: 0;
    width: 100%;
    pointer-events: none;
}
.prism-card-enter-from,
.prism-mode-card-enter-from {
    opacity: 0;
    transform: translateY(16px) scale(0.995);
}
.prism-card-leave-to,
.prism-mode-card-leave-to {
    opacity: 0;
    transform: translateY(-10px) scale(0.992);
}
button:disabled {
    cursor: not-allowed;
    opacity: 0.5;
}
@media (prefers-reduced-motion: reduce) {
    .prism-card-enter-active,
    .prism-card-leave-active,
    .prism-mode-card-enter-active,
    .prism-mode-card-leave-active {
        transition: none;
    }
    .prism-card-enter-from,
    .prism-card-leave-to,
    .prism-mode-card-enter-from,
    .prism-mode-card-leave-to {
        transform: none;
    }
}
@media (min-width: 1500px) {
    .prism-page {
        max-width: 77.75rem;
    }
}
@media (max-width: 730px) {
    .prism-inline-status {
        align-items: flex-start;
        flex-wrap: wrap;
        margin-inline: 1.0625rem;
    }
}
</style>
