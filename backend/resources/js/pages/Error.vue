<script setup lang="ts">
import { Head } from '@inertiajs/vue3';
import { BrandLogo } from '@filebeam/ui';
import { computed } from 'vue';

const props = defineProps<{ status: number }>();

const errors: Record<number, { title: string; description: string }> = {
    403: {
        title: 'You do not have access',
        description:
            'This area is not available to your account. If you think this is a mistake, sign in with the correct account and try again.',
    },
    404: {
        title: 'Page not found',
        description:
            'The link may be incorrect, or the page may have moved. Check the address or return to the start.',
    },
    419: {
        title: 'Your session has expired',
        description:
            'For your security, this request can no longer be completed. Return home and try again.',
    },
    429: {
        title: 'Too many requests',
        description:
            'This service has received too many requests from you. Wait a moment before trying again.',
    },
    500: {
        title: 'Something went wrong',
        description: 'The service could not complete your request. Try again shortly.',
    },
    503: {
        title: 'Temporarily unavailable',
        description: 'The service is offline for a short while. Please check back soon.',
    },
};

const error = computed(() => {
    if (errors[props.status]) {
        return errors[props.status];
    }

    return props.status >= 500
        ? errors[500]
        : {
              title: 'Request could not be completed',
              description:
                  'The service could not complete this request. Check the address or return to the start.',
          };
});
</script>

<template>
    <div class="fb-shell">
        <Head :title="error.title" />

        <header class="fb-header">
            <a href="/" class="fb-header__brand" aria-label="Home">
                <BrandLogo />
            </a>
        </header>

        <main class="grid flex-1 place-items-center px-4 py-12 sm:py-24">
            <section class="w-full max-w-xl text-center" aria-labelledby="error-title">
                <div
                    class="relative mx-auto mb-8 grid size-28 place-items-center overflow-hidden rounded-full border border-[var(--fb-border)] bg-[var(--fb-selected-surface)]"
                    aria-hidden="true"
                >
                    <svg
                        class="absolute inset-0 size-full text-[var(--fb-accent-text)]"
                        viewBox="0 0 112 112"
                        fill="none"
                    >
                        <path d="M8 85 104 27" stroke="currentColor" opacity=".9" />
                        <path d="m8 27 96 58" stroke="currentColor" opacity=".4" />
                    </svg>
                    <span
                        class="fb-code relative z-10 rounded-lg border border-[var(--fb-control-border)] bg-[var(--fb-surface)] px-2 py-1 text-sm font-semibold tracking-widest"
                        >{{ status }}</span
                    >
                </div>
                <h1 id="error-title" class="text-4xl font-semibold tracking-tight sm:text-5xl">
                    {{ error.title }}
                </h1>
                <p
                    class="mx-auto mt-4 max-w-lg text-base leading-relaxed text-[var(--fb-text-muted)]"
                >
                    {{ error.description }}
                </p>
                <a href="/" class="fb-button fb-button--secondary mt-8 gap-2">
                    Return home
                    <svg
                        class="size-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.8"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                    >
                        <path d="M5 12h14M13 6l6 6-6 6" />
                    </svg>
                </a>
            </section>
        </main>

        <footer
            class="flex min-h-[4.5rem] items-center justify-center border-t border-[var(--fb-border)] px-4 text-[0.8125rem] text-[var(--fb-text-muted)]"
        >
            Error {{ status }}
        </footer>
    </div>
</template>
