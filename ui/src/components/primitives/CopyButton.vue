<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue';
import Button from './Button.vue';
import Icon from './Icon.vue';
import Tooltip from './Tooltip.vue';

const props = withDefaults(
    defineProps<{
        value: string;
        label: string;
        variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
        disabled?: boolean;
        iconOnly?: boolean;
    }>(),
    { variant: 'primary', disabled: false, iconOnly: false },
);

const state = ref<'idle' | 'pending' | 'copied' | 'error'>('idle');
let resetTimer: number | undefined;
let disposed = false;

function resetAfterDelay(): void {
    window.clearTimeout(resetTimer);
    resetTimer = window.setTimeout(() => {
        if (disposed) return;
        state.value = 'idle';
    }, 2500);
}

async function copy(): Promise<void> {
    if (props.disabled || state.value === 'pending') return;
    window.clearTimeout(resetTimer);
    state.value = 'pending';
    try {
        if (!navigator.clipboard) throw new Error('Clipboard API is unavailable.');
        await navigator.clipboard.writeText(props.value);
        if (disposed) return;
        state.value = 'copied';
    } catch {
        if (disposed) return;
        state.value = 'error';
    }
    if (disposed) return;
    resetAfterDelay();
}

onBeforeUnmount(() => {
    disposed = true;
    window.clearTimeout(resetTimer);
});
</script>

<template>
    <span class="copy-button">
        <Tooltip
            :content="state === 'copied' ? 'Copied' : state === 'error' ? 'Copy failed' : label"
            :disabled="!iconOnly"
        >
            <Button
                :variant="variant"
                :disabled="disabled || state === 'pending'"
                :aria-label="state === 'idle' ? label : `${label}: ${state}`"
                class="copy-button__button"
                :icon="iconOnly"
                :class="{ 'copy-button__button--icon': iconOnly }"
                @click="copy"
            >
                <Transition name="copy-button-state">
                    <span
                        :key="state"
                        class="copy-button__content"
                        :class="{ 'copy-button__content--icon': iconOnly }"
                    >
                        <Icon
                            :name="state === 'copied' ? 'check' : 'copy'"
                            :size="16"
                            class="copy-button__icon"
                        />
                        <span v-if="!iconOnly" class="copy-button__label">
                            {{
                                state === 'pending'
                                    ? 'Copying'
                                    : state === 'copied'
                                      ? 'Copied'
                                      : state === 'error'
                                        ? 'Copy failed'
                                        : label
                            }}
                        </span>
                    </span>
                </Transition>
            </Button>
        </Tooltip>
        <span class="sr-only" aria-live="polite">
            {{
                state === 'error'
                    ? 'Copy failed. Select the value and copy it manually.'
                    : state === 'copied'
                      ? `${label} copied.`
                      : ''
            }}
        </span>
    </span>
</template>

<style scoped>
.copy-button {
    display: inline-flex;
}
.copy-button__button {
    position: relative;
    width: max-content;
    min-width: 8.5rem;
    flex: none;
    white-space: nowrap;
}
.copy-button__button--icon {
    width: 2.5rem;
    min-width: 2.5rem;
}
.copy-button__content {
    display: inline-grid;
    grid-template-columns: 1rem 1fr;
    align-items: center;
    gap: 0.375rem;
}
.copy-button__content--icon {
    grid-template-columns: 1rem;
    gap: 0;
}
.copy-button-state-enter-active,
.copy-button-state-leave-active {
    transition:
        opacity 160ms ease,
        transform 160ms ease;
}
.copy-button-state-leave-active {
    position: absolute;
    pointer-events: none;
}
.copy-button-state-enter-from,
.copy-button-state-leave-to {
    opacity: 0;
    transform: translateY(1px);
}
@media (prefers-reduced-motion: reduce) {
    .copy-button-state-enter-active,
    .copy-button-state-leave-active {
        transition: none;
    }
}
</style>
