<script setup lang="ts">
import Button from '../primitives/Button.vue';
import FilebeamIcon from '../primitives/FilebeamIcon.vue';
import SmoothProgress from '../primitives/SmoothProgress.vue';

const props = defineProps<{
    progress: number;
    downloading: boolean;
    phase: 'downloading' | 'waiting' | 'verifying' | 'completed';
    turbo: boolean;
    uploadProgress: number;
    uploaderStatus: 'uploading' | 'stalled' | 'completed' | 'unavailable';
}>();
defineEmits<{ cancel: [] }>();

function phaseLabel(): string {
    if (!props.downloading && props.phase !== 'completed') return 'Ready to download';
    return {
        downloading: 'Decrypting download',
        waiting: 'Waiting to resume',
        verifying: 'Verifying download',
        completed: 'Download completed',
    }[props.phase];
}

function phaseDescription(): string {
    if (!props.downloading && props.phase !== 'completed')
        return 'Choose Download files to decrypt this transfer in your browser.';
    return {
        downloading: 'Decrypting encrypted data in this browser.',
        waiting: 'This transfer will resume automatically when more encrypted data is available.',
        verifying: 'Checking the decrypted download before it is saved.',
        completed: 'Download finished and integrity verified.',
    }[props.phase];
}

function showLoader(): boolean {
    return props.downloading && (props.phase === 'downloading' || props.phase === 'verifying');
}

function uploaderLabel(): string {
    return {
        uploading: 'Sender is still uploading',
        stalled: 'Sender connection lost',
        completed: 'Sender upload finished',
        unavailable: 'Sender upload progress unavailable',
    }[props.uploaderStatus];
}
</script>

<template>
    <div class="mt-6 rounded-xl border border-[var(--fb-border)] bg-[var(--fb-surface-raised)] p-4">
        <section v-if="turbo" class="download-status__sender" aria-label="Sender upload progress">
            <SmoothProgress
                :value="uploadProgress"
                :active="uploaderStatus === 'uploading'"
                label="Sender upload progress"
            >
                <template #label="{ percentage }">
                    <div
                        class="mb-2 flex items-center justify-between gap-4 text-sm text-[var(--fb-text)]"
                    >
                        <span>{{ uploaderLabel() }}</span
                        ><span class="download-status__percentage tabular-nums"
                            >{{ percentage }}%</span
                        >
                    </div>
                </template>
            </SmoothProgress>
        </section>
        <section :class="{ 'mt-5': turbo }">
            <SmoothProgress :value="progress" :active="downloading" :label="phaseLabel()">
                <template #label="{ percentage }">
                    <div
                        class="mb-3 flex items-center justify-between gap-4 text-sm text-[var(--fb-text)]"
                    >
                        <span class="flex min-w-0 items-center gap-2"
                            ><span
                                class="download-status__title"
                                :class="{ 'text-[var(--fb-success)]': phase === 'completed' }"
                                ><Transition name="status-copy" mode="out-in"
                                    ><span :key="phaseLabel()">{{ phaseLabel() }}</span></Transition
                                ></span
                            ><span
                                class="grid size-[17px] shrink-0 place-items-center"
                                aria-hidden="true"
                                ><FilebeamIcon v-if="showLoader()" name="loader" :size="17" />
                                <FilebeamIcon
                                    v-else-if="phase === 'completed'"
                                    name="check"
                                    :size="17"
                                    class="text-[var(--fb-success)]" /></span></span
                        ><span class="download-status__percentage tabular-nums"
                            >{{ percentage }}%</span
                        >
                    </div>
                </template>
            </SmoothProgress>
            <p
                class="download-status__description mt-3 text-sm text-[var(--fb-text-muted)]"
                role="status"
                aria-live="polite"
                aria-atomic="true"
            >
                <Transition name="status-copy" mode="out-in"
                    ><span :key="phaseLabel()">{{ phaseDescription() }}</span></Transition
                >
            </p>
            <div class="download-status__controls mt-4">
                <Button v-if="downloading" variant="ghost" @click="$emit('cancel')">Cancel</Button>
            </div>
        </section>
    </div>
</template>

<style scoped>
.download-status__sender {
    border-bottom: 1px solid var(--fb-border);
    padding-bottom: 1.25rem;
}
.download-status__controls {
    min-height: 2.75rem;
}
.download-status__title {
    min-width: 0;
    min-height: 1.25rem;
}
.download-status__percentage {
    display: inline-block;
    width: 3.25rem;
    flex: none;
    text-align: right;
}
.download-status__description {
    min-height: 3rem;
}
.status-copy-enter-active,
.status-copy-leave-active {
    transition:
        opacity 120ms ease,
        transform 120ms ease;
}
.status-copy-enter-from,
.status-copy-leave-to {
    opacity: 0;
    transform: translateY(2px);
}
@media (prefers-reduced-motion: reduce) {
    .status-copy-enter-active,
    .status-copy-leave-active {
        transition: none;
    }
}
</style>
