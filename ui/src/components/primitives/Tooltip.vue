<script setup lang="ts">
import { useAttrs } from 'vue';
import {
    TooltipContent,
    TooltipPortal,
    TooltipProvider,
    TooltipRoot,
    TooltipTrigger,
} from 'reka-ui';

defineOptions({ inheritAttrs: false });

withDefaults(
    defineProps<{
        content: string;
        side?: 'top' | 'right' | 'bottom' | 'left';
        align?: 'start' | 'center' | 'end';
        delay?: number;
        toggleOnClick?: boolean;
        disabled?: boolean;
        inline?: boolean;
    }>(),
    { side: 'top', align: 'center', delay: 300, toggleOnClick: false, disabled: false },
);

const open = defineModel<boolean>('open', { default: false });
const emit = defineEmits<{ escapeKeyDown: [event: KeyboardEvent] }>();
const attrs = useAttrs();
</script>

<template>
    <TooltipProvider :delay-duration="delay">
        <TooltipRoot
            :disabled="disabled"
            v-model:open="open"
            :delay-duration="delay"
            :disable-closing-trigger="toggleOnClick"
        >
            <TooltipTrigger as-child v-bind="attrs" @click="open = toggleOnClick ? !open : false">
                <slot />
            </TooltipTrigger>
            <TooltipPortal :disabled="inline">
                <TooltipContent
                    :aria-label="content"
                    :side="side"
                    :align="align"
                    :side-offset="8"
                    class="fb-tooltip"
                    @escape-key-down="emit('escapeKeyDown', $event)"
                >
                    {{ content }}
                </TooltipContent>
            </TooltipPortal>
        </TooltipRoot>
    </TooltipProvider>
</template>

<style>
.fb-tooltip {
    z-index: 90;
    max-width: min(20rem, calc(100vw - 2rem));
    border: 1px solid var(--fb-border);
    border-radius: var(--fb-radius-sm);
    background: var(--fb-surface-raised);
    color: var(--fb-text);
    padding: 0.5rem 0.75rem;
    font-size: 0.75rem;
    line-height: 1.5;
    box-shadow: 0 8px 24px rgb(0 0 0 / 20%);
    animation: fb-tooltip-in var(--fb-duration-fast) ease;
}
.fb-tooltip[data-state='closed'] {
    animation: fb-tooltip-out var(--fb-duration-fast) ease;
}
@keyframes fb-tooltip-in {
    from {
        opacity: 0;
        transform: translateY(2px);
    }
    to {
        opacity: 1;
        transform: translateY(0);
    }
}
@keyframes fb-tooltip-out {
    to {
        opacity: 0;
    }
}
@media (prefers-reduced-motion: reduce) {
    .fb-tooltip {
        animation: none;
    }
}
</style>
