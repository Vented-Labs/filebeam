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
            class="fb-toast-viewport fixed bottom-[max(1rem,env(safe-area-inset-bottom))] right-[max(1rem,env(safe-area-inset-right))] z-50 flex w-[calc(100vw-2rem)] max-w-sm flex-col outline-none sm:bottom-6 sm:right-6"
        />
    </ToastProvider>
</template>

<style>
.fb-toast[data-state='open'] {
    animation: toast-enter var(--fb-duration-toast-in) var(--fb-ease);
}
.fb-toast[data-state='closed'] {
    animation: toast-exit var(--fb-duration-toast-out) cubic-bezier(0.4, 0, 1, 1);
}
.fb-toast[data-swipe='move'] {
    transform: translateX(var(--reka-toast-swipe-move-x));
}
.fb-toast[data-swipe='cancel'] {
    transform: translateX(0);
    transition: transform var(--fb-duration-switch) var(--fb-ease);
}
.fb-toast[data-swipe='end'] {
    animation: toast-swipe-out var(--fb-duration-toast-out) ease-out;
}
.fb-toast-viewport {
    gap: 0.625rem;
    perspective: 60rem;
}
@keyframes toast-enter {
    from {
        opacity: 0;
        filter: blur(4px);
        transform: translate3d(2rem, 1.25rem, 0) scale(0.94);
    }
    55% {
        opacity: 1;
    }
}
@keyframes toast-exit {
    to {
        opacity: 0;
        filter: blur(2px);
        transform: translate3d(1.5rem, 0.5rem, 0) scale(0.97);
    }
}
@keyframes toast-swipe-out {
    to {
        opacity: 0;
        transform: translateX(calc(var(--reka-toast-swipe-end-x) + 2rem));
    }
}
@media (prefers-reduced-motion: reduce) {
    .fb-toast[data-state='open'],
    .fb-toast[data-state='closed'],
    .fb-toast[data-swipe='end'] {
        animation: none;
    }
}
</style>
