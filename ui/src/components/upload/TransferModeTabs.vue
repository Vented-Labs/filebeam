<script setup lang="ts">
import { TabsList, TabsRoot, TabsTrigger } from 'reka-ui';
import Icon from '../primitives/Icon.vue';

withDefaults(defineProps<{ disabled?: boolean }>(), { disabled: false });
const mode = defineModel<'files' | 'note'>({ required: true });
</script>

<template>
    <TabsRoot v-model="mode" class="prism-mode" data-testid="prism-mode-tabs">
        <TabsList class="prism-mode__list" :data-mode="mode" aria-label="Transfer type">
            <span class="prism-mode__indicator" aria-hidden="true" />
            <TabsTrigger value="files" :disabled="disabled" class="prism-mode__trigger">
                <Icon name="folder" :size="16" />Files
            </TabsTrigger>
            <TabsTrigger value="note" :disabled="disabled" class="prism-mode__trigger">
                <Icon name="note" :size="16" />Notes
            </TabsTrigger>
        </TabsList>
    </TabsRoot>
</template>

<style scoped>
.prism-mode {
    display: block;
    width: fit-content;
    margin-right: auto;
    margin-bottom: 1.375rem;
    margin-left: auto;
}
.prism-mode__list {
    position: relative;
    isolation: isolate;
    display: grid;
    width: min(16.875rem, calc(100vw - 2.5rem));
    height: 3rem;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    box-sizing: border-box;
    padding: 0.3125rem;
    border: 1px solid var(--fb-border);
    border-radius: 1rem;
    background: #16131d;
    box-shadow: inset 0 1px 2px #0002;
}
.prism-mode__indicator {
    position: absolute;
    z-index: -1;
    top: 0.3125rem;
    left: 0.3125rem;
    width: calc(50% - 0.3125rem);
    height: 2.25rem;
    border: 1px solid #70518d80;
    border-radius: 0.6875rem;
    background: var(--fb-selected-surface);
    box-shadow: inset 0 1px 0 #ffffff0d;
    pointer-events: none;
    transition: transform var(--fb-duration-selection) var(--fb-ease);
}
.prism-mode__list[data-mode='note'] .prism-mode__indicator {
    transform: translateX(100%);
}
.prism-mode__trigger {
    z-index: 1;
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: center;
    gap: 0.5625rem;
    border: 0;
    border-radius: 0.6875rem;
    color: var(--fb-text-muted);
    background: transparent;
    font-size: 0.875rem;
    cursor: pointer;
    transition:
        color var(--fb-duration-control) ease,
        background var(--fb-duration-control) ease;
}
.prism-mode__trigger:hover:not(:disabled) {
    background: #ffffff05;
}
.prism-mode__trigger[data-state='active'] {
    color: #f5ecff;
}
@media (max-width: 730px) {
    .prism-mode {
        margin-bottom: 1.125rem;
    }
    .prism-mode__list {
        height: 2.8125rem;
    }
    .prism-mode__indicator {
        height: 2.0625rem;
    }
}
@media (prefers-reduced-motion: reduce) {
    .prism-mode__indicator {
        transition: none;
    }
}
</style>
