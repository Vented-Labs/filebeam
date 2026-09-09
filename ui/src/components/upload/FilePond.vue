<script setup lang="ts">
import Button from '../primitives/Button.vue';
import Icon from '../primitives/Icon.vue';
import BrandLogo from '../brand/BrandLogo.vue';

const props = defineProps<{
    disabled: boolean;
    dragging: boolean;
    compact?: boolean;
}>();
const emit = defineEmits<{ choose: []; files: [files: FileList] }>();

function dropped(event: DragEvent): void {
    const files = event.dataTransfer?.files;
    if (!files?.length) return;
    event.preventDefault();
    event.stopPropagation();
    if (!props.disabled) emit('files', files);
}
function onDragOver(event: DragEvent): void {
    if (Array.from(event.dataTransfer?.types ?? []).includes('Files')) event.preventDefault();
}
function selected(event: Event): void {
    const input = event.target as HTMLInputElement;
    if (input.files?.length) emit('files', input.files);
    input.value = '';
}
</script>

<template>
    <section
        class="file-pond"
        data-testid="file-pond"
        :data-disabled="disabled || undefined"
        :class="{ 'file-pond--dragging': dragging, 'file-pond--compact': compact }"
        @dragover="onDragOver"
        @drop="dropped"
    >
        <input
            id="filebeam-picker"
            class="sr-only"
            type="file"
            multiple
            :disabled="disabled"
            @change="selected"
        />
        <slot v-if="compact" />
        <div v-else class="file-pond__empty">
            <div class="file-pond__art" aria-hidden="true">
                <span class="file-pond__card file-pond__card--left">
                    <Icon name="image" :size="21" />
                </span>
                <span class="file-pond__card file-pond__card--right">
                    <Icon name="code" :size="21" />
                </span>
                <span class="file-pond__card file-pond__card--main">
                    <BrandLogo compact />
                </span>
                <span class="file-pond__seal"><Icon name="lock" :size="12" /></span>
            </div>
            <h1>Drop your files <span>here</span></h1>
            <p>They are encrypted in your browser before they leave your device.</p>
            <Button class="file-pond__choose" :disabled="disabled" @click="emit('choose')">
                <Icon name="folder" :size="17" />Choose files<Icon name="arrow-up" :size="17" />
            </Button>
            <small>Or drag and drop anywhere</small>
        </div>
    </section>
</template>

<style scoped>
.file-pond {
    position: relative;
    min-width: 0;
    overflow: hidden;
    background: var(--fb-surface);
}
.file-pond::after {
    content: '';
    position: absolute;
    inset: 0.75rem 1.125rem;
    border: 1px dashed transparent;
    border-radius: 1rem;
    pointer-events: none;
    transition:
        border-color var(--fb-duration-control) ease,
        background var(--fb-duration-control) ease;
}
.file-pond::before {
    content: '';
    position: absolute;
    inset: 0;
    background: var(--fb-dropzone-glow);
    opacity: 0.65;
    pointer-events: none;
    transition: opacity var(--fb-duration-pane) var(--fb-ease-hover);
}
.file-pond--compact::before {
    display: none;
}
.file-pond:not([data-disabled]):is(:hover, :focus-within)::before {
    opacity: 1;
}
.file-pond:not([data-disabled]):is(:hover, :focus-within) .file-pond__card--left {
    transform: translate(-4px, -3px) rotate(-21deg);
}
.file-pond:not([data-disabled]):is(:hover, :focus-within) .file-pond__card--right {
    transform: translate(5px, -3px) rotate(23deg);
}
.file-pond:not([data-disabled]):is(:hover, :focus-within) .file-pond__card--main {
    transform: translateY(-5px);
}
.file-pond:not(.file-pond--compact):hover::after {
    border-color: #a99bb624;
    background: #ffffff01;
}
.file-pond--dragging::after {
    border-color: #9c79be77;
    background: #ffffff03;
}
.file-pond__empty {
    position: relative;
    z-index: 1;
    display: flex;
    min-height: 21.125rem;
    box-sizing: border-box;
    align-items: center;
    flex-direction: column;
    justify-content: center;
    padding: 0.9375rem 1.75rem 1.625rem;
    text-align: center;
}
.file-pond__art {
    position: relative;
    width: 15.75rem;
    height: 7.75rem;
    margin-bottom: 0.4375rem;
}
.file-pond__card {
    position: absolute;
    display: grid;
    width: 4.5rem;
    height: 5.375rem;
    place-items: center;
    border: 1px solid #63517188;
    border-radius: 0.8125rem;
    color: var(--fb-text-muted);
    background: linear-gradient(145deg, #2e2639, #211b2b);
    box-shadow:
        inset 0 1px 0 #ffffff0d,
        0 9px 17px #0002;
    transition: transform var(--fb-duration-pane) var(--fb-ease-hover);
}
.file-pond__card--left {
    top: 1.9375rem;
    left: 3.1875rem;
    transform: translate(0, 0) rotate(-16deg);
}
.file-pond__card--right {
    top: 1.625rem;
    right: 3.0625rem;
    transform: translate(0, 0) rotate(17deg);
}
.file-pond__card--main {
    transform: translateY(0);
    z-index: 1;
    top: 0.6875rem;
    left: 5.1875rem;
    width: 5.375rem;
    height: 6.125rem;
    border-color: #9b79b573;
    background: linear-gradient(140deg, #3d2c4c, #2c2138);
    box-shadow:
        inset 0 1px 0 #ffffff16,
        0 12px 18px #0003;
}
.file-pond__card--main :deep(.fb-brand) {
    padding: 0;
}
.file-pond__card--main :deep(.fb-brand__glyph) {
    width: 2.25rem;
    height: 2.25rem;
}
.file-pond__seal {
    position: absolute;
    z-index: 2;
    right: 4.125rem;
    bottom: 0.5625rem;
    display: grid;
    width: 1.5rem;
    height: 1.5rem;
    place-items: center;
    border: 1px solid #665576;
    border-radius: 999px;
    color: var(--fb-accent-text);
    background: #211d2a;
    box-shadow: 0 2px 5px #0004;
}
.file-pond__empty h1 {
    margin: 0;
    color: var(--fb-text);
    font-size: 2.375rem;
    font-weight: 600;
    letter-spacing: -0.045em;
    line-height: 1.15;
}
.file-pond__empty h1 span {
    color: var(--fb-accent-text);
}
.file-pond__empty > p {
    margin: 0.625rem 0 0;
    color: var(--fb-text-muted);
    font-size: 0.8125rem;
    line-height: 1.7;
}
.file-pond__choose {
    min-width: 12.625rem;
    min-height: 2.75rem;
    margin-top: 1.4375rem;
}
.file-pond__empty > small {
    margin-top: 0.6875rem;
    color: var(--fb-text-subtle);
    font-size: 0.625rem;
}
.file-pond--dragging .file-pond__card--left {
    transform: translateY(-2px) rotate(-18deg);
}
.file-pond--dragging .file-pond__card--right {
    transform: translateY(-2px) rotate(19deg);
}
.file-pond--dragging .file-pond__card--main {
    transform: translateY(-4px);
}
@media (min-width: 1500px) {
    .file-pond__empty {
        min-height: 21.875rem;
    }
}
@media (max-width: 730px) {
    .file-pond__empty {
        min-height: 21.5625rem;
        padding: 1.1875rem 0.9375rem 1.6875rem;
    }
    .file-pond__empty h1 {
        font-size: 1.9375rem;
    }
    .file-pond__empty > p {
        max-width: 21rem;
        padding-inline: 0.3125rem;
        font-size: 0.75rem;
    }
}
@media (max-width: 380px) {
    .file-pond__empty h1 {
        font-size: 1.75rem;
    }
}
@media (prefers-reduced-motion: reduce) {
    .file-pond::before,
    .file-pond::after,
    .file-pond__card {
        transition: none;
    }
    .file-pond--dragging .file-pond__card {
        transform: none;
    }
    .file-pond:not([data-disabled]):is(:hover, :focus-within) .file-pond__card {
        transform: none;
    }
}
</style>
