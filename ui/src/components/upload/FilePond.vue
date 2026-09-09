<script setup lang="ts">
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';
import { formatBytes } from '../../lib/format';
const props = defineProps<{
    disabled: boolean;
    dragging: boolean;
    maximumFiles: number | null;
    maximumBytes: number | null;
    compact?: boolean;
}>();
const emit = defineEmits<{ choose: []; files: [files: FileList] }>();
function dropped(event: DragEvent): void {
    const files = event.dataTransfer?.files;
    if (files?.length) {
        event.preventDefault();
        event.stopPropagation();
        if (!props.disabled) emit('files', files);
    }
}
function onDragOver(event: DragEvent): void {
    if (Array.from(event.dataTransfer?.types ?? []).includes('Files')) event.preventDefault();
}
</script>

<template>
    <section
        class="file-pond relative grid min-h-[29rem] place-items-center overflow-hidden rounded-[1.5rem] border border-dashed border-[var(--fb-control-border)] bg-[var(--fb-surface)] px-6 py-12 text-center transition-colors"
        data-testid="file-pond"
        :class="{
            'border-[var(--fb-focus)] bg-[var(--fb-selected-surface)]': dragging,
            'file-pond--dragging': dragging,
            'file-pond--compact': compact,
        }"
        @dragover="onDragOver"
        @drop="dropped"
    >
        <input
            id="filebeam-picker"
            class="sr-only"
            type="file"
            multiple
            :disabled="disabled"
            @change="emit('files', ($event.target as HTMLInputElement).files!)"
        />
        <div class="max-w-xl">
            <Icon
                name="upload"
                :size="96"
                class="pond-illustration mx-auto text-[var(--fb-brand-bright)]"
            />
            <h1
                class="mt-7 text-4xl font-semibold tracking-tight text-[var(--fb-text)] sm:text-5xl"
            >
                Drop your files <span class="text-[var(--fb-accent-text)]">here</span>
            </h1>
            <p class="mt-3 text-lg text-[var(--fb-text-muted)]">
                They are encrypted in your browser before they leave your device.
            </p>
            <Button
                class="pond-choose mt-12 gap-3"
                variant="primary"
                :disabled="disabled"
                @click="emit('choose')"
                ><Icon name="folder" :size="22" />Choose files</Button
            >
            <slot name="transfer-method">
                <p class="mt-8 text-xs text-[var(--fb-text-muted)]">
                    {{
                        maximumBytes === null
                            ? 'Unlimited size'
                            : `${formatBytes(maximumBytes)} per transfer`
                    }}
                    &middot;
                    {{ maximumFiles === null ? 'Unlimited files' : `Up to ${maximumFiles} files` }}
                </p>
            </slot>
        </div>
    </section>
</template>

<style scoped>
.file-pond {
    min-height: 33.5rem;
}
.file-pond::before {
    content: '';
    position: absolute;
    inset: 0;
    pointer-events: none;
    opacity: 0;
    background: radial-gradient(
        ellipse at 50% 35%,
        color-mix(in srgb, var(--fb-brand) 14%, transparent),
        transparent 70%
    );
    transition: opacity var(--fb-duration-normal) ease;
}
.file-pond > div {
    position: relative;
}
.pond-choose {
    min-width: 16rem;
    min-height: 3.4rem;
    font-size: 1rem;
    border-radius: 0.9rem;
}
.file-pond :deep(.pond-illustration) {
    width: 8rem;
    height: 8rem;
}
.file-pond--dragging {
    border-color: var(--fb-focus);
    box-shadow: inset 0 0 36px color-mix(in srgb, var(--fb-brand) 12%, transparent);
    animation: pond-glow 1800ms ease-in-out infinite alternate;
}
.file-pond--dragging::before {
    opacity: 1;
}
@keyframes pond-glow {
    to {
        box-shadow: inset 0 0 56px color-mix(in srgb, var(--fb-brand) 20%, transparent);
    }
}
.file-pond :deep(.fb-icon) {
    transition: transform 240ms ease;
}
.file-pond--dragging :deep(.fb-icon) {
    transform: translateY(-2px);
}
@media (prefers-reduced-motion: reduce) {
    .file-pond--dragging {
        animation: none;
    }
    .file-pond :deep(.fb-icon) {
        transition: none;
        transform: none;
    }
}
@media (max-width: 640px) {
    .file-pond {
        min-height: 29rem;
    }
    .file-pond :deep(.pond-illustration) {
        width: 6rem;
        height: 6rem;
    }
}
@media (max-width: 1023px) {
    .file-pond--compact {
        min-height: 0;
        padding-block: 1.5rem;
    }
    .file-pond--compact :deep(.pond-illustration) {
        width: 3rem;
        height: 3rem;
    }
    .file-pond--compact h1 {
        margin-top: 1rem;
        font-size: 1.8rem;
    }
    .file-pond--compact h1 + p {
        display: none;
    }
    .file-pond--compact .pond-choose {
        margin-top: 1.25rem;
    }
    .file-pond--compact p:last-child {
        margin-top: 1.25rem;
    }
}
</style>
