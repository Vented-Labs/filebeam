<script setup lang="ts">
import {
    ToastClose,
    ToastDescription,
    ToastProvider,
    ToastRoot,
    ToastTitle,
    ToastViewport,
} from 'reka-ui';
import Icon from './Icon.vue';

withDefaults(
    defineProps<{
        title: string;
        description?: string;
        tone?: 'success';
    }>(),
    { description: undefined, tone: 'success' },
);

const open = defineModel<boolean>('open', { required: true });
</script>

<template>
    <ToastProvider :duration="4000">
        <ToastRoot
            v-model:open="open"
            type="background"
            :class="[
                'fb-toast pointer-events-auto flex w-full items-start gap-3 rounded-xl border bg-[var(--fb-surface-raised)] p-4 text-[var(--fb-text)] shadow-xl shadow-black/25',
                tone === 'success' ? 'border-[var(--fb-success)]' : 'border-[var(--fb-border)]',
            ]"
        >
            <span
                class="grid size-6 shrink-0 place-items-center rounded-full bg-[var(--fb-selected-surface)] text-[var(--fb-success)]"
            >
                <Icon name="check" :size="16" />
            </span>
            <div class="min-w-0 flex-1">
                <ToastTitle class="font-medium">{{ title }}</ToastTitle>
                <ToastDescription
                    v-if="description"
                    class="mt-1 text-sm text-[var(--fb-text-muted)]"
                >
                    {{ description }}
                </ToastDescription>
            </div>
            <ToastClose
                aria-label="Dismiss notification"
                class="rounded-md p-1 text-[var(--fb-text-muted)] outline-none transition-colors hover:text-[var(--fb-text)] focus-visible:ring-2 focus-visible:ring-[var(--fb-focus)]"
            >
                <Icon name="x" :size="16" />
            </ToastClose>
        </ToastRoot>
        <ToastViewport
            class="fixed bottom-[max(1rem,env(safe-area-inset-bottom))] right-[max(1rem,env(safe-area-inset-right))] z-50 flex w-[calc(100vw-2rem)] max-w-sm flex-col outline-none sm:bottom-6 sm:right-6"
        />
    </ToastProvider>
</template>

<style scoped>
.fb-toast[data-state='open'] {
    animation: toast-enter 180ms ease-out;
}
.fb-toast[data-state='closed'] {
    animation: toast-exit 140ms ease-in;
}
@keyframes toast-enter {
    from {
        opacity: 0;
        transform: translateY(6px);
    }
}
@keyframes toast-exit {
    to {
        opacity: 0;
        transform: translateY(4px);
    }
}
@media (prefers-reduced-motion: reduce) {
    .fb-toast[data-state='open'],
    .fb-toast[data-state='closed'] {
        animation: none;
    }
}
</style>
